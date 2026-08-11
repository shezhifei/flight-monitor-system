"""Live Python sidecar API contract smoke tests using FastAPI TestClient.

These tests exercise the real application stack (ai_sidecar_entrypoint app)
without spawning a real uvicorn process, avoiding port and process management.

Coverage:
- /internal/ai/v1/health        no auth -> 200
- /internal/ai/v1/runs          no X-Service-Identity -> 401/403
- /internal/ai/v1/runs          valid identity + valid envelope -> 200 succeeded
- /internal/ai/v1/runs          no OPENAI_API_KEY -> degraded=true, status=succeeded
- /internal/ai/v1/runs          invalid envelope -> status=failed, success=false
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any

import jwt
import pytest
from fastapi.testclient import TestClient

# Ensure project root is on path before any src imports
_project_root = Path(__file__).parent.parent.parent
os.environ["JWT_SECRET"] = os.environ.get("JWT_SECRET", "test-secret-for-smoke-tests")

from scripts.host.ai_sidecar_entrypoint import app

from src.infrastructure.ai.service_identity import (
    SERVICE_AUDIENCE,
    SERVICE_IDENTITY_HEADER,
    SERVICE_ISSUER,
    SERVICE_SUBJECT,
)


@pytest.fixture(scope="module")
def client() -> TestClient:
    return TestClient(app)


def _create_token(path: str, secret: str | None = None, expired: bool = False) -> str:
    secret = secret or os.environ["JWT_SECRET"]
    now = int(time.time())
    payload = {
        "iss": SERVICE_ISSUER,
        "sub": SERVICE_SUBJECT,
        "aud": SERVICE_AUDIENCE,
        "iat": now,
        "exp": now - 10 if expired else now + 300,
        "path": path,
    }
    return jwt.encode(payload, secret, algorithm="HS256")


def _valid_envelope_body(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "contract_version": "ai-runtime.v1",
        "job_id": "job_smoke_001",
        "run_id": "run_smoke_001",
        "correlation_id": "corr_smoke",
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
                    "object_id": "FL123",
                    "data": {"flight_number": "CA1234", "status": "scheduled"},
                }
            ],
            "limits": {},
        },
        "task": {"task_type": "nl_query", "user_message": "What is the status of CA1234?"},
    }
    base.update(overrides)
    return base


class TestHealthEndpoint:
    def test_health_no_auth_returns_200(self, client: TestClient) -> None:
        resp = client.get("/internal/ai/v1/health")
        assert resp.status_code == 200
        data = resp.json()
        assert data["status"] == "healthy"
        assert data["service"] == "ai-runtime"


class TestRunsAuth:
    def test_runs_no_identity_returns_401(self, client: TestClient) -> None:
        resp = client.post("/internal/ai/v1/runs", json=_valid_envelope_body())
        assert resp.status_code == 401
        detail = resp.json().get("detail", {})
        assert detail.get("code") == "MISSING_SERVICE_IDENTITY"

    def test_runs_wrong_path_identity_returns_403(self, client: TestClient) -> None:
        token = _create_token("/different/path")
        resp = client.post(
            "/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 403
        detail = resp.json().get("detail", {})
        assert detail.get("code") == "PATH_MISMATCH"

    def test_runs_expired_identity_returns_401(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs", expired=True)
        resp = client.post(
            "/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 401


class TestRunsSuccessPath:
    def test_runs_valid_identity_and_envelope_returns_200_succeeded(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs")
        resp = client.post(
            "/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is True
        assert data["status"] == "succeeded"
        assert data["answer"]
        assert data["run_id"] == "run_smoke_001"
        assert isinstance(data["reasoning_steps"], list)
        assert isinstance(data["evidence"], list)
        assert isinstance(data["proposals"], list)
        assert "metrics" in data

    def test_runs_note_request_generates_proposal(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs")
        body = _valid_envelope_body(task={"task_type": "nl_query", "user_message": "请为航班添加备注: 延误因天气"})
        resp = client.post(
            "/internal/ai/v1/runs",
            json=body,
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is True
        proposals = data.get("proposals", [])
        assert len(proposals) >= 1
        prop = proposals[0]
        assert prop["object_type"] == "Flight"
        assert prop["action_name"] == "add_note"
        assert "note_content" in prop["arguments"]
        assert prop["risk_level"] == "low"
        assert 0.0 <= prop["confidence"] <= 1.0


class TestDegradedPath:
    def test_runs_no_openai_key_returns_degraded_succeeded(
        self, client: TestClient, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        # Reset default runtime service singleton so it picks up the new env
        import src.infrastructure.ai.runtime_service as rs_mod

        rs_mod._default_runtime_service = None

        token = _create_token("/internal/ai/v1/runs")
        resp = client.post(
            "/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is True
        assert data["status"] == "succeeded"
        assert data.get("degraded") is True
        assert data["limitations"]
        assert data["metrics"]["model"] == "heuristic-runtime-v1"


class TestInvalidEnvelope:
    def test_runs_invalid_envelope_returns_failed_success_false(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs")
        body = _valid_envelope_body(run_id="", job_id="")
        resp = client.post(
            "/internal/ai/v1/runs",
            json=body,
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["success"] is False
        assert data["status"] == "failed"
        assert "required" in data.get("error", "").lower()

    def test_runs_malformed_envelope_returns_422(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs")
        resp = client.post(
            "/internal/ai/v1/runs",
            json={"contract_version": "ai-runtime.v1"},
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 422


class TestGracefulDegradation:
    """Ensure enhancement failures do NOT cause HTTP 500."""

    def test_runs_no_500_when_ontology_and_schema_unavailable(
        self, client: TestClient, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_ontology_registry",
            lambda: None,
        )
        monkeypatch.setattr(
            "src.infrastructure.ai.runtime_context._get_schema_mirror",
            lambda: None,
        )
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        import src.infrastructure.ai.runtime_service as rs_mod

        rs_mod._default_runtime_service = None

        token = _create_token("/internal/ai/v1/runs")
        resp = client.post(
            "/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 200, "must NOT return 500 on enhancement failure"
        data = resp.json()
        assert data["success"] is True
        assert data["status"] == "succeeded"
        assert data.get("degraded") is True
        assert data["limitations"], "should report limitations when dependencies unavailable"


class TestFixtureRoundTrip:
    """Ensure the response shape matches the shared fixture expectations."""

    def test_response_matches_expected_top_level_keys(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs")
        resp = client.post(
            "/internal/ai/v1/runs",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        data = resp.json()
        required = {
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
            "success",
        }
        assert required.issubset(data.keys()), f"Missing keys: {required - set(data.keys())}"

        # JSON round-trip stability
        serialized = json.dumps(data)
        parsed = json.loads(serialized)
        assert parsed["run_id"] == "run_smoke_001"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
