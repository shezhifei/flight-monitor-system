"""Unit tests for AI runtime adapter (ContextEnvelope → AiStructuredOutput)."""

from __future__ import annotations

import json

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeLimits,
    EnvelopeObject,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.runtime_service import (
    STATUS_FAILED,
    STATUS_SUCCEEDED,
    FakeLlmClient,
    LlmUnavailableError,
    RuntimeService,
    structured_output_to_response_dict,
    validate_envelope,
)


def _sample_envelope(**overrides) -> ContextEnvelope:
    base = ContextEnvelope(
        contract_version="ai-runtime.v1",
        job_id="job_test_001",
        run_id="run_test_001",
        correlation_id="corr_test",
        requester=EnvelopeRequester(user_id="user_1", roles=["ai:chat"]),
        ontology=EnvelopeOntology(
            version="flight-ops.v1",
            allowed_object_types=["Flight"],
            allowed_actions=[
                "Flight.update_status",
                "Flight.change_stand",
                "Flight.add_note",
            ],
            risk_ceiling="medium",
        ),
        context=EnvelopeContext(
            objects=[
                EnvelopeObject(
                    object_type="Flight",
                    object_id="FL123",
                    data={"flight_number": "CA1234", "status": "scheduled"},
                )
            ],
            limits=EnvelopeLimits(),
        ),
        task=EnvelopeTask(task_type="nl_query", user_message="What is the status of CA1234?"),
    )
    for key, value in overrides.items():
        setattr(base, key, value)
    return base


class TestValidateEnvelope:
    def test_valid_envelope_has_no_errors(self):
        assert validate_envelope(_sample_envelope()) == []

    def test_missing_run_id_fails_validation(self):
        env = _sample_envelope(run_id="")
        assert any("run_id" in err for err in validate_envelope(env))


class TestRuntimeServiceExecute:
    @pytest.mark.asyncio
    async def test_valid_envelope_with_fake_llm_returns_succeeded(self):
        service = RuntimeService(
            llm_client=FakeLlmClient(response="航班 CA1234 按计划执行。"),
        )
        output = await service.execute_run(_sample_envelope())

        assert output.status == STATUS_SUCCEEDED
        assert output.run_id == "run_test_001"
        assert output.answer
        assert output.contract_version == "ai-structured-output.v1"
        assert len(output.reasoning_steps) >= 3
        assert output.evidence
        assert output.metrics is not None
        assert output.metrics.duration_ms >= 1

    @pytest.mark.asyncio
    async def test_missing_required_fields_returns_failed_status(self):
        service = RuntimeService(llm_client=FakeLlmClient())
        env = _sample_envelope(job_id="", run_id="")
        output = await service.execute_run(env)

        assert output.status == STATUS_FAILED
        assert "required" in output.answer.lower()

    @pytest.mark.asyncio
    async def test_llm_unavailable_returns_succeeded_with_degraded_flag(self):
        service = RuntimeService(
            llm_client=FakeLlmClient(
                configured=True,
                raise_on_complete=LlmUnavailableError("provider timeout"),
            ),
        )
        output = await service.execute_run(_sample_envelope())

        assert output.status == STATUS_SUCCEEDED
        assert output.limitations
        assert any("provider timeout" in lim for lim in output.limitations)
        assert output.metrics is not None
        assert output.metrics.model == "heuristic-runtime-v1"
        assert "启发式" in output.answer or "CA1234" in output.answer

    @pytest.mark.asyncio
    async def test_llm_not_configured_returns_succeeded_with_degraded_flag(self):
        service = RuntimeService(llm_client=FakeLlmClient(configured=False))
        output = await service.execute_run(_sample_envelope())

        assert output.status == STATUS_SUCCEEDED
        assert any("LLM not configured" in lim for lim in output.limitations)
        assert output.metrics is not None
        assert output.metrics.model == "heuristic-runtime-v1"

    @pytest.mark.asyncio
    async def test_note_request_builds_rust_compatible_proposal(self):
        env = _sample_envelope(
            task=EnvelopeTask(
                task_type="nl_query",
                user_message="请为航班添加备注: 延误因天气",
            ),
        )
        service = RuntimeService(llm_client=FakeLlmClient())
        output = await service.execute_run(env)

        assert len(output.proposals) == 1
        proposal = output.proposals[0]
        assert proposal.object_type == "Flight"
        assert proposal.object_id == "FL123"
        assert proposal.action_name == "add_note"
        assert proposal.arguments.get("note_content")
        assert proposal.risk_level == "low"
        assert 0.0 <= proposal.confidence <= 1.0
        assert proposal.reasoning

    @pytest.mark.asyncio
    async def test_response_dict_matches_rust_ai_structured_output_shape(self):
        service = RuntimeService(llm_client=FakeLlmClient())
        output = await service.execute_run(_sample_envelope())
        payload = structured_output_to_response_dict(output)

        required_keys = {
            "contract_version",
            "run_id",
            "status",
            "answer",
            "reasoning_steps",
            "evidence",
            "proposals",
            "limitations",
            "metrics",
            "token_usage",
        }
        assert required_keys.issubset(payload.keys())
        assert isinstance(payload["reasoning_steps"], list)
        assert isinstance(payload["proposals"], list)
        assert isinstance(payload["evidence"], list)
        # Round-trip JSON (Rust serde_json compatible field names)
        serialized = json.dumps(payload)
        parsed = json.loads(serialized)
        assert parsed["run_id"] == "run_test_001"


class TestRuntimeServiceEnvironment:
    def test_openai_not_configured_without_api_key(self, monkeypatch):
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        service = RuntimeService()
        assert service._resolve_llm() is None

    @pytest.mark.asyncio
    async def test_openai_client_not_used_when_key_missing(self, monkeypatch):
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        service = RuntimeService()
        output = await service.execute_run(_sample_envelope())
        assert output.status == STATUS_SUCCEEDED
        assert any("LLM not configured" in lim for lim in output.limitations)
        assert output.metrics is not None
        assert output.metrics.model == "heuristic-runtime-v1"
        payload = structured_output_to_response_dict(output)
        assert "heuristic" in payload["metrics"]["model"]


class TestDegradedContract:
    """Contract: status=succeeded + degraded=true = usable heuristic answer."""

    @pytest.mark.asyncio
    async def test_response_dict_has_degraded_when_llm_unavailable(self):
        service = RuntimeService(
            llm_client=FakeLlmClient(
                configured=True,
                raise_on_complete=LlmUnavailableError("provider timeout"),
            ),
        )
        output = await service.execute_run(_sample_envelope())
        payload = structured_output_to_response_dict(output)
        assert payload["status"] == "succeeded"
        assert payload.get("degraded") is True
        assert any("provider timeout" in lim for lim in payload["limitations"])

    @pytest.mark.asyncio
    async def test_response_dict_has_degraded_when_llm_not_configured(self):
        service = RuntimeService(llm_client=FakeLlmClient(configured=False))
        output = await service.execute_run(_sample_envelope())
        payload = structured_output_to_response_dict(output)
        assert payload["status"] == "succeeded"
        assert any("LLM not configured" in lim for lim in payload["limitations"])

    @pytest.mark.asyncio
    async def test_invalid_envelope_returns_failed_success_false(self):
        service = RuntimeService()
        output = await service.execute_run(_sample_envelope(run_id="", job_id=""))
        assert output.status == STATUS_FAILED
        assert "required" in output.answer.lower()


class TestApiRouteContract:
    """Integration contract tests for /internal/ai/v1/runs route."""

    def _build_envelope_body(self) -> dict:
        return {
            "contract_version": "ai-runtime.v1",
            "job_id": "job_contract_test",
            "run_id": "run_contract_test",
            "correlation_id": "corr_contract",
            "requester": {"user_id": "user_1", "roles": ["ai:chat"]},
            "ontology": {
                "version": "flight-ops.v1",
                "allowed_object_types": ["Flight"],
                "allowed_actions": ["Flight.add_note"],
                "risk_ceiling": "medium",
            },
            "context": {
                "objects": [
                    {
                        "object_type": "Flight",
                        "object_id": "FL999",
                        "data": {"flight_number": "CZ9999", "status": "scheduled"},
                    }
                ],
                "limits": {},
            },
            "task": {"task_type": "nl_query", "user_message": "flight status?"},
        }

    def test_no_key_degraded_returns_200_success_true_degraded_true(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.api_routes.require_service_identity",
            lambda req: None,
        )
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        import src.infrastructure.ai.runtime_service as rs_mod

        rs_mod._default_runtime_service = None

        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        from src.infrastructure.ai.api_routes import router

        app = FastAPI()
        app.include_router(router)
        client = TestClient(app)

        resp = client.post("/internal/ai/v1/runs", json=self._build_envelope_body())
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is True
        assert data["status"] == "succeeded"
        assert data.get("degraded") is True
        assert data["limitations"]
        assert data["metrics"]["model"] == "heuristic-runtime-v1"

    def test_invalid_envelope_route_fails_gracefully(self, monkeypatch):
        monkeypatch.setattr(
            "src.infrastructure.ai.api_routes.require_service_identity",
            lambda req: None,
        )
        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        from src.infrastructure.ai.api_routes import router

        app = FastAPI()
        app.include_router(router)
        client = TestClient(app)

        # Valid shape but empty required fields → envelope validates, logic fails
        body = self._build_envelope_body()
        body["run_id"] = ""
        body["job_id"] = ""
        resp = client.post("/internal/ai/v1/runs", json=body)
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is False
        assert data["status"] == "failed"
