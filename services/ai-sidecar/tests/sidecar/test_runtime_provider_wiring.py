"""End-to-end demonstration: the main (tool-calling) LLM now uses the resolved
provider's credentials, not env-only.

Two layers:
- A. `_resolve_gateway` builds a real ``OpenAIClient`` whose ``config`` carries the
  resolved provider's base_url / api_key / model (with env fallback + override
  precedence).
- B. A full ``stream_run_with_tools`` run (runner patched to avoid network) proves
  the per-entity provider credentials flow all the way through the streaming path
  into the gateway handed to the runner.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from unittest.mock import patch

import pytest

# Reuse the resolved-config / resolver fakes from the skill-injection suite (same dir).
from test_skill_runtime_injection import (
    FakeCapabilityResolver,
    FakeModel,
    FakeResolvedConfig,
)

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.runtime_service import RuntimeService


def _envelope() -> ContextEnvelope:
    return ContextEnvelope(
        job_id="job-wire",
        run_id="run-wire",
        correlation_id="",
        requester=EnvelopeRequester(user_id="u1"),
        ontology=EnvelopeOntology(),
        context=EnvelopeContext(),
        task=EnvelopeTask(task_type="chat", user_message="Hello"),
        conversation_history=[],
    )


# ---------------------------------------------------------------------------
# A. _resolve_gateway credential wiring
# ---------------------------------------------------------------------------


def test_gateway_uses_resolved_provider_credentials(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    # entity-gw.example resolves to a reserved/sometimes-private address in CI;
    # allow private targets just for this credential-plumbing check.
    monkeypatch.setenv("AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS", "1")
    svc = RuntimeService()  # no override → production resolution path

    cfg = FakeResolvedConfig()
    cfg.api_key = "entity-secret"
    cfg.base_url = "https://entity-gw.example/v1"
    cfg.model = FakeModel(model_id="gpt-4o", provider_model="entity-gpt")

    gw = svc._resolve_gateway(cfg)

    assert gw is not None
    assert gw.config.api_key == "entity-secret"
    assert gw.config.base_url == "https://entity-gw.example/v1"
    # provider_model (the provider-side id) is what the gateway talks to.
    assert gw.config.default_model == "entity-gpt"


def test_gateway_falls_back_to_env_when_entity_has_no_key(monkeypatch):
    monkeypatch.setenv("OPENAI_API_KEY", "env-secret")
    # Some test environments resolve public domains to private benchmark IPs;
    # allow private targets just for this credential-plumbing check.
    monkeypatch.setenv("AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS", "1")
    svc = RuntimeService()

    # FakeResolvedConfig declares no api_key attribute → resolved key is empty.
    gw = svc._resolve_gateway(FakeResolvedConfig())

    assert gw is not None
    assert gw.config.api_key == "env-secret"


def test_gateway_none_without_any_key(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    svc = RuntimeService()
    assert svc._resolve_gateway(FakeResolvedConfig()) is None


def test_injected_override_wins():
    class _FakeGateway:
        def is_configured(self) -> bool:
            return True

    override = _FakeGateway()
    svc = RuntimeService(llm_client=override)
    assert svc._resolve_gateway(FakeResolvedConfig()) is override


# ---------------------------------------------------------------------------
# B. end-to-end: resolved provider gateway reaches the runner via the stream
# ---------------------------------------------------------------------------


@dataclass
class _FakeResult:
    text: str = "ANSWER"
    model: str = "entity-gpt"
    usage: dict[str, Any] | None = None


@dataclass
class _FakeEvent:
    type: str
    result: _FakeResult | None = None
    text_delta: str | None = None
    tool_call: dict[str, Any] | None = None


def _make_client_capturing_runner(capture: dict[str, Any]):
    class _FakeRunner:
        def __init__(self, client=None, **kwargs):
            capture["client"] = client

        async def stream_chat_with_tools(self, *, messages, **kwargs):
            capture["model"] = kwargs.get("model")
            yield _FakeEvent(type="text_delta", text_delta="ANSWER")
            yield _FakeEvent(type="completed", result=_FakeResult())

    return _FakeRunner


@dataclass
class _FakeTool:
    """Minimal resolved-tool stand-in with the same surface as ResolvedToolConfig."""

    name: str
    description: str = "d"
    parameters: dict[str, Any] = field(default_factory=dict)
    risk_level: str = "low"
    cacheable: bool = False
    side_effect: bool = False

    def to_schema(self) -> dict[str, Any]:
        return {
            "type": "function",
            "function": {"name": self.name, "description": self.description, "parameters": self.parameters},
        }


@pytest.mark.asyncio
async def test_stream_run_hands_resolved_gateway_to_runner(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    # entity-gw.example resolves to a reserved/sometimes-private address in CI;
    # allow private targets just for this credential-plumbing check.
    monkeypatch.setenv("AI_SIDECAR_ALLOW_PRIVATE_HTTP_TARGETS", "1")

    cfg = FakeResolvedConfig()
    cfg.api_key = "entity-secret"
    cfg.base_url = "https://entity-gw.example/v1"
    cfg.model = FakeModel(model_id="gpt-4o", provider_model="entity-gpt")
    # Task A2: no heuristic fallback — the stream only runs when the resolved
    # snapshot carries a non-empty tool list.
    cfg.tools = [_FakeTool("flight_status_lookup")]
    resolver = FakeCapabilityResolver(resolved_config=cfg)

    capture: dict[str, Any] = {}
    svc = RuntimeService(capability_resolver=resolver)  # NO llm override

    with patch(
        "src.infrastructure.ai.runtime_service.LLMStreamRunner",
        _make_client_capturing_runner(capture),
    ):
        events = [evt async for evt in svc.stream_run_with_tools(_envelope())]

    # The gateway built from the entity's provider reached the runner.
    gw = capture["client"]
    assert gw is not None
    assert gw.config.api_key == "entity-secret"
    assert gw.config.base_url == "https://entity-gw.example/v1"

    # And the run streamed to completion through the new wiring.
    types = [
        getattr(e, "type", "") or e.get("event") if isinstance(e, dict) else getattr(e, "type", "") for e in events
    ]
    assert any("complete" in str(t).lower() for t in types)
