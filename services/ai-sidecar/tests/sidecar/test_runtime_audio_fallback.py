"""Regression tests for the ASR-fallback wiring in RuntimeService.

Covers:
- The resolved snapshot now carries ``api_key`` (capability_resolver), and the
  ASR-fallback path falls back to the env ``OPENAI_API_KEY`` (same source as the
  main LLM) when the entity config carries no provider key.
- The ``audio_client_factory`` injection point lets tests substitute the
  AudioClient without network access (per-entity providers => per-call build).
"""

from __future__ import annotations

import base64
from types import SimpleNamespace

import pytest

from src.infrastructure.ai.runtime_service import RuntimeService


class _FakeAudioClient:
    """Records construction args and returns a canned transcript."""

    last_kwargs: dict = {}

    def __init__(self, **kwargs):
        type(self).last_kwargs = kwargs

    async def transcribe(self, model, audio_bytes, mime, language=None):
        return f"transcribed:{model}:{len(audio_bytes)}b"


def _resolved(api_key: str):
    return SimpleNamespace(
        base_url="https://gw.example/v1",
        api_key=api_key,
        timeout=12.0,
        model=SimpleNamespace(input_modalities=["text"], capabilities={"asr_model": "whisper-x"}),
    )


@pytest.mark.asyncio
async def test_transcribe_uses_injected_factory_and_entity_key():
    svc = RuntimeService(audio_client_factory=_FakeAudioClient)
    payload = base64.b64encode(b"abcd").decode()

    text = await svc._transcribe_attachment(_resolved("entity-key"), payload, "audio/wav")

    assert text == "transcribed:whisper-x:4b"
    assert _FakeAudioClient.last_kwargs["api_key"] == "entity-key"
    assert _FakeAudioClient.last_kwargs["base_url"] == "https://gw.example/v1"
    assert _FakeAudioClient.last_kwargs["timeout"] == 12.0


@pytest.mark.asyncio
async def test_transcribe_falls_back_to_env_key(monkeypatch):
    monkeypatch.setenv("OPENAI_API_KEY", "env-key")
    svc = RuntimeService(audio_client_factory=_FakeAudioClient)
    payload = base64.b64encode(b"xy").decode()

    text = await svc._transcribe_attachment(_resolved(""), payload, "audio/wav")

    assert text == "transcribed:whisper-x:2b"
    assert _FakeAudioClient.last_kwargs["api_key"] == "env-key"
