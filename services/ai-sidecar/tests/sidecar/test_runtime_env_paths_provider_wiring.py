"""The two remaining env-only LLM paths now honor the resolved provider.

Mirrors ``test_runtime_provider_wiring.py`` (which covers the tool-calling
gateway path). Here we prove the *non-tool* clients built by
``_resolve_llm`` / ``_resolve_streaming_llm`` carry the entity's resolved
provider credentials (api_key / base_url / model), fall back to the OPENAI_*
env vars when the entity declares none, and still respect the injected
override precedence so test fakes flow straight through.

Assertions read the constructed client's stored attributes (see
``runtime_llm.py``: ``_api_key`` / ``_base_url`` / ``_model``).
"""

from __future__ import annotations

import socket

import pytest

# Reuse the resolved-config / model fakes from the skill-injection suite (same dir).
from test_skill_runtime_injection import FakeModel, FakeResolvedConfig

from src.infrastructure.ai.runtime_llm import (
    OpenAiLlmClient,
    OpenAiStreamingLlmClient,
)
from src.infrastructure.ai.runtime_service import RuntimeService


@pytest.fixture(autouse=True)
def _mock_public_dns(monkeypatch):
    def fake_getaddrinfo(host, port, type=0):
        return [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", ("93.184.216.34", port))]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)


def _entity_config() -> FakeResolvedConfig:
    cfg = FakeResolvedConfig()
    cfg.api_key = "entity-secret"
    cfg.base_url = "https://entity-gw.example/v1"
    cfg.model = FakeModel(model_id="gpt-4o", provider_model="entity-gpt")
    return cfg


# ---------------------------------------------------------------------------
# _resolve_llm — non-streaming .complete() path
# ---------------------------------------------------------------------------


def test_resolve_llm_uses_resolved_provider_credentials(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
    monkeypatch.delenv("OPENAI_MODEL", raising=False)
    svc = RuntimeService()  # no override → production resolution path

    client = svc._resolve_llm(_entity_config())

    assert isinstance(client, OpenAiLlmClient)
    assert client._api_key == "entity-secret"
    assert client._base_url == "https://entity-gw.example/v1"
    # provider_model (the provider-side id) is what we talk to.
    assert client._model == "entity-gpt"


def test_resolve_llm_falls_back_to_env_for_api_key_only(monkeypatch):
    """A config with no api_key but its own base_url/model: only the key falls
    back to env (mirrors _resolve_gateway — base_url/model come from the config
    when present, env is the per-field fallback)."""
    monkeypatch.setenv("OPENAI_API_KEY", "env-secret")
    monkeypatch.setenv("OPENAI_BASE_URL", "https://env-gw.example/v1")
    monkeypatch.setenv("OPENAI_MODEL", "env-model")
    svc = RuntimeService()

    # FakeResolvedConfig declares no api_key attribute → resolved key is empty,
    # but it does carry its own default base_url and model.
    client = svc._resolve_llm(FakeResolvedConfig())

    assert isinstance(client, OpenAiLlmClient)
    assert client._api_key == "env-secret"  # only the key fell back to env
    assert client._base_url == "https://api.openai.com/v1"  # from the config
    assert client._model == "gpt-4o"  # provider_model from the config's FakeModel


def test_resolve_llm_full_env_fallback_with_bare_config(monkeypatch):
    """When the config carries no base_url/model either, every field falls back
    to env. Use a minimal stand-in with only an (empty) api_key surface."""
    monkeypatch.setenv("OPENAI_API_KEY", "env-secret")
    monkeypatch.setenv("OPENAI_BASE_URL", "https://env-gw.example/v1")
    monkeypatch.setenv("OPENAI_MODEL", "env-model")
    svc = RuntimeService()

    client = svc._resolve_llm()  # no config at all → pure env path

    assert isinstance(client, OpenAiLlmClient)
    assert client._api_key == "env-secret"
    assert client._base_url == "https://env-gw.example/v1"
    assert client._model == "env-model"


def test_resolve_llm_none_without_any_key(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    svc = RuntimeService()
    # No env key and FakeResolvedConfig has no api_key → not configured → None.
    assert svc._resolve_llm(FakeResolvedConfig()) is None


def test_resolve_llm_no_arg_is_env_only(monkeypatch):
    """Default (no resolved_config) preserves byte-for-byte env behavior."""
    monkeypatch.setenv("OPENAI_API_KEY", "env-secret")
    monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
    monkeypatch.setenv("OPENAI_MODEL", "env-model")
    svc = RuntimeService()

    client = svc._resolve_llm()

    assert isinstance(client, OpenAiLlmClient)
    assert client._api_key == "env-secret"
    assert client._base_url == ""  # no base_url env, none passed
    assert client._model == "env-model"


def test_resolve_llm_override_wins(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)

    class _FakeLlm:
        def is_configured(self) -> bool:
            return True

    override = _FakeLlm()
    svc = RuntimeService(llm_client=override)
    # Even with a resolved config present, the injected override takes precedence.
    assert svc._resolve_llm(_entity_config()) is override


# ---------------------------------------------------------------------------
# _resolve_streaming_llm — no-tool .stream_complete() path
# ---------------------------------------------------------------------------


def test_resolve_streaming_llm_uses_resolved_provider_credentials(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
    monkeypatch.delenv("OPENAI_MODEL", raising=False)
    svc = RuntimeService()

    client = svc._resolve_streaming_llm(_entity_config())

    assert isinstance(client, OpenAiStreamingLlmClient)
    assert client._api_key == "entity-secret"
    assert client._base_url == "https://entity-gw.example/v1"
    assert client._model == "entity-gpt"
    assert client.model == "entity-gpt"  # public property mirrors it


def test_resolve_streaming_llm_falls_back_to_env_for_api_key_only(monkeypatch):
    monkeypatch.setenv("OPENAI_API_KEY", "env-secret")
    monkeypatch.setenv("OPENAI_BASE_URL", "https://env-gw.example/v1")
    monkeypatch.setenv("OPENAI_MODEL", "env-model")
    svc = RuntimeService()

    client = svc._resolve_streaming_llm(FakeResolvedConfig())

    assert isinstance(client, OpenAiStreamingLlmClient)
    assert client._api_key == "env-secret"  # only the key fell back to env
    assert client._base_url == "https://api.openai.com/v1"  # from the config
    assert client._model == "gpt-4o"  # provider_model from the config's FakeModel


def test_resolve_streaming_llm_none_without_any_key(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    svc = RuntimeService()
    assert svc._resolve_streaming_llm(FakeResolvedConfig()) is None


def test_resolve_streaming_llm_no_arg_is_env_only(monkeypatch):
    monkeypatch.setenv("OPENAI_API_KEY", "env-secret")
    monkeypatch.delenv("OPENAI_BASE_URL", raising=False)
    monkeypatch.setenv("OPENAI_MODEL", "env-model")
    svc = RuntimeService()

    client = svc._resolve_streaming_llm()

    assert isinstance(client, OpenAiStreamingLlmClient)
    assert client._api_key == "env-secret"
    assert client._base_url == ""
    assert client._model == "env-model"


def test_resolve_streaming_llm_streaming_override_wins(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)

    class _FakeStream:
        def is_configured(self) -> bool:
            return True

        def stream_complete(self, system_prompt, user_message):  # pragma: no cover
            yield "x"

    override = _FakeStream()
    svc = RuntimeService(streaming_llm_client=override)
    assert svc._resolve_streaming_llm(_entity_config()) is override


# ---------------------------------------------------------------------------
# provider_model preference (Task ② symmetry): provider_model beats model_id
# ---------------------------------------------------------------------------


def test_resolve_llm_prefers_provider_model_over_model_id(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    svc = RuntimeService()

    cfg = FakeResolvedConfig()
    cfg.api_key = "k"
    cfg.model = FakeModel(model_id="display-id", provider_model="provider-side-id")

    client = svc._resolve_llm(cfg)
    assert client is not None
    assert client._model == "provider-side-id"


def test_resolve_llm_falls_back_to_model_id_when_provider_model_blank(monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    svc = RuntimeService()

    # When provider_model is blank, model resolution falls back to the config's
    # top-level model_id (same chain as _resolve_gateway).
    cfg = FakeResolvedConfig()
    cfg.api_key = "k"
    cfg.model_id = "only-model-id"
    cfg.model = FakeModel(model_id="ignored", provider_model="")

    client = svc._resolve_llm(cfg)
    assert client is not None
    assert client._model == "only-model-id"
