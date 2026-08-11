"""The default (non-tool) run entrypoints are now resolver-aware.

``execute_run`` (POST /runs) and ``stream_run`` (POST /runs/stream) used to skip
capability resolution and the attachment security gate entirely and call the
env-only LLM clients. They now run the shared ``_prepare_capabilities`` preamble:

- resolve the entity config and thread it into ``_resolve_llm`` /
  ``_resolve_streaming_llm`` (per-entity provider credentials),
- enforce the MIME/size/modality security gate BEFORE any model call (fail-closed),
- and, with no resolver wired, behave exactly as before (env-only, no gate).

Credential passthrough at the ``_resolve_*`` level is covered by
``test_runtime_env_paths_provider_wiring.py``; here we prove the *host methods*
invoke the preamble and honor its outcome.
"""

from __future__ import annotations

import asyncio
from types import SimpleNamespace
from unittest.mock import MagicMock

# Reuse the resolved-config / resolver fakes from the skill-injection suite.
from test_skill_runtime_injection import (
    FakeCapabilityResolver,
    FakeModel,
    FakeResolvedConfig,
)

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeLimits,
    EnvelopeObject,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.runtime_service import RuntimeService
from src.infrastructure.ai.runtime_service import service as runtime_service_mod


def _sample_envelope() -> ContextEnvelope:
    return ContextEnvelope(
        contract_version="ai-runtime.v1",
        job_id="job_default_paths",
        run_id="run_default_paths",
        correlation_id="corr_default_paths",
        requester=EnvelopeRequester(user_id="u1", roles=["ai:chat"]),
        ontology=EnvelopeOntology(
            version="flight-ops.v1",
            allowed_object_types=["Flight"],
            allowed_actions=["Flight.add_note"],
            risk_ceiling="medium",
        ),
        context=EnvelopeContext(
            objects=[
                EnvelopeObject(
                    object_type="Flight",
                    object_id="FL1",
                    data={"flight_number": "CA1", "status": "scheduled"},
                )
            ],
            limits=EnvelopeLimits(),
        ),
        task=EnvelopeTask(task_type="nl_query", user_message="What is the status of CA1?"),
    )


def _graph_disabled() -> MagicMock:
    gr = MagicMock()
    gr.is_enabled.return_value = False
    return gr


def _entity_cfg(api_key: str = "entity-secret") -> FakeResolvedConfig:
    cfg = FakeResolvedConfig()
    cfg.api_key = api_key
    cfg.model = FakeModel(model_id="gpt-4o", provider_model="entity-gpt")
    return cfg


def _collect_stream(svc: RuntimeService, envelope: ContextEnvelope):
    async def _run():
        return [evt async for evt in svc.stream_run(envelope)]

    return asyncio.run(_run())


def _patch_one_bad_attachment(monkeypatch):
    """Force the input gate to see a single disallowed-MIME attachment regardless
    of envelope shape (decouples the test from the attachment surface layout)."""
    bad = SimpleNamespace(mime_type="application/x-evil", data="QUJD")  # b"ABC"
    monkeypatch.setattr(runtime_service_mod, "_iter_envelope_attachments", lambda env: [bad])


# ===========================================================================
# execute_run (POST /runs) — non-streaming
# ===========================================================================


def test_execute_run_threads_resolved_config_into_resolve_llm(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    cfg = _entity_cfg()
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=cfg),
        graph_runner=_graph_disabled(),
    )

    captured = {}

    def _spy(resolved_config=None):
        captured["cfg"] = resolved_config
        return None  # force the heuristic tail so the run completes deterministically

    monkeypatch.setattr(svc, "_resolve_llm", _spy)

    out = asyncio.run(svc.execute_run(_sample_envelope()))

    # The resolved entity config reached the credential resolver.
    assert captured["cfg"] is cfg
    assert out is not None


def test_execute_run_fail_closed_on_resolver_error():
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(should_fail=True),
        graph_runner=_graph_disabled(),
    )

    out = asyncio.run(svc.execute_run(_sample_envelope()))

    assert "AI_CAPABILITY_RESOLUTION_FAILED" in out.answer


def test_execute_run_rejects_disallowed_mime(monkeypatch):
    _patch_one_bad_attachment(monkeypatch)
    cfg = _entity_cfg(api_key="k")
    cfg.security = {"allowed_input_mime_types": ["image/png"]}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=cfg),
        graph_runner=_graph_disabled(),
    )

    out = asyncio.run(svc.execute_run(_sample_envelope()))

    assert "AI_INPUT_MIME_NOT_ALLOWED" in out.answer


def test_execute_run_no_resolver_skips_gate(monkeypatch):
    """Back-compat: with no resolver wired, the gate never runs (env-only path)."""
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    _patch_one_bad_attachment(monkeypatch)
    svc = RuntimeService(graph_runner=_graph_disabled())  # no capability_resolver

    out = asyncio.run(svc.execute_run(_sample_envelope()))

    assert "AI_INPUT_MIME_NOT_ALLOWED" not in out.answer
    assert "AI_CAPABILITY_RESOLUTION_FAILED" not in out.answer


# ===========================================================================
# stream_run (POST /runs/stream) — streaming, no tools
# ===========================================================================


def test_stream_run_resolves_and_threads_config(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    cfg = _entity_cfg()
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=cfg),
        graph_runner=_graph_disabled(),
    )

    captured = {}

    def _spy(resolved_config=None):
        captured["cfg"] = resolved_config
        return None  # heuristic tail

    monkeypatch.setattr(svc, "_resolve_streaming_llm", _spy)

    events = _collect_stream(svc, _sample_envelope())
    steps = [e["data"].get("step") for e in events if e["event"] == "progress"]

    assert "capability.resolved" in steps
    assert captured["cfg"] is cfg
    assert any(e["event"] == "run.complete" for e in events)


def test_stream_run_fail_closed_on_resolver_error():
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(should_fail=True),
        graph_runner=_graph_disabled(),
    )

    events = _collect_stream(svc, _sample_envelope())
    steps = [e["data"].get("step") for e in events if e["event"] == "progress"]

    assert "capability.rejected" in steps
    assert any(e["event"] == "run.fail" for e in events)
    # Fail-closed: it stops before producing a successful completion.
    assert not any(e["event"] == "run.complete" for e in events)


def test_stream_run_rejects_disallowed_mime(monkeypatch):
    _patch_one_bad_attachment(monkeypatch)
    cfg = _entity_cfg(api_key="k")
    cfg.security = {"allowed_input_mime_types": ["image/png"]}
    svc = RuntimeService(
        capability_resolver=FakeCapabilityResolver(resolved_config=cfg),
        graph_runner=_graph_disabled(),
    )

    events = _collect_stream(svc, _sample_envelope())
    fails = [e for e in events if e["event"] == "run.fail"]

    assert fails
    assert "AI_INPUT_MIME_NOT_ALLOWED" in fails[0]["data"]["answer"]


def test_stream_run_no_resolver_emits_no_capability_events(monkeypatch):
    """Back-compat: no resolver → no capability.* events, run completes normally."""
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    _patch_one_bad_attachment(monkeypatch)
    svc = RuntimeService(graph_runner=_graph_disabled())  # no resolver

    events = _collect_stream(svc, _sample_envelope())
    steps = [e["data"].get("step") for e in events if e["event"] == "progress"]

    assert "capability.resolved" not in steps
    assert "capability.rejected" not in steps
    assert any(e["event"] == "run.complete" for e in events)
