"""Tests for the ASR/TTS audio execution adapter (AudioClient).

httpx is mocked via a MockTransport patched into httpx.AsyncClient so no real
network calls are made.
"""

from __future__ import annotations

import socket

import httpx
import pytest

from src.infrastructure.ai import audio_client as audio_client_module
from src.infrastructure.ai.audio_client import AudioClient


@pytest.fixture(autouse=True)
def _mock_public_dns(monkeypatch):
    def fake_getaddrinfo(host, port, type=0):
        return [(socket.AF_INET, socket.SOCK_STREAM, socket.IPPROTO_TCP, "", ("93.184.216.34", port))]

    monkeypatch.setattr(socket, "getaddrinfo", fake_getaddrinfo)


def _patch_transport(monkeypatch, handler):
    """Make httpx.AsyncClient(...) use a MockTransport with `handler`."""
    real_async_client = httpx.AsyncClient

    def _factory(*args, **kwargs):
        kwargs["transport"] = httpx.MockTransport(handler)
        return real_async_client(*args, **kwargs)

    monkeypatch.setattr(audio_client_module.httpx, "AsyncClient", _factory)


@pytest.mark.asyncio
async def test_transcribe_returns_text(monkeypatch):
    captured = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["url"] = str(request.url)
        captured["method"] = request.method
        captured["auth"] = request.headers.get("Authorization")
        captured["body"] = request.content
        return httpx.Response(200, json={"text": "hello world"})

    _patch_transport(monkeypatch, handler)
    client = AudioClient(base_url="https://gw.example/v1/", api_key="sk-test")

    result = await client.transcribe(
        model="whisper-1",
        audio_bytes=b"RIFF....",
        mime="audio/wav",
        language="en",
    )

    assert result == "hello world"
    assert captured["url"] == "https://gw.example/v1/audio/transcriptions"
    assert captured["method"] == "POST"
    assert captured["auth"] == "Bearer sk-test"
    # multipart body carries model, language and the raw audio bytes
    assert b"whisper-1" in captured["body"]
    assert b"RIFF" in captured["body"]


@pytest.mark.asyncio
async def test_transcribe_http_error_prefix(monkeypatch):
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(400, text="bad audio")

    _patch_transport(monkeypatch, handler)
    client = AudioClient(base_url="https://gw.example/v1", api_key="sk-test")

    with pytest.raises(RuntimeError) as exc_info:
        await client.transcribe(model="whisper-1", audio_bytes=b"x", mime="audio/wav")

    assert str(exc_info.value).startswith("AI_ASR_HTTP_400")


@pytest.mark.asyncio
async def test_transcribe_missing_text_field(monkeypatch):
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json={"unexpected": 1})

    _patch_transport(monkeypatch, handler)
    client = AudioClient(base_url="https://gw.example/v1", api_key="sk-test")

    with pytest.raises(RuntimeError) as exc_info:
        await client.transcribe(model="whisper-1", audio_bytes=b"x", mime="audio/wav")

    assert str(exc_info.value).startswith("AI_ASR_DECODE")


@pytest.mark.asyncio
async def test_synthesize_returns_bytes(monkeypatch):
    captured = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["url"] = str(request.url)
        captured["json"] = request.content
        captured["auth"] = request.headers.get("Authorization")
        return httpx.Response(200, content=b"\x00\x01AUDIO")

    _patch_transport(monkeypatch, handler)
    client = AudioClient(base_url="https://gw.example/v1", api_key="sk-test")

    audio = await client.synthesize(model="tts-1", text="hi", voice="alloy", fmt="wav")

    assert audio == b"\x00\x01AUDIO"
    assert captured["url"] == "https://gw.example/v1/audio/speech"
    assert captured["auth"] == "Bearer sk-test"
    assert b"tts-1" in captured["json"]
    assert b"alloy" in captured["json"]
    assert b"wav" in captured["json"]


@pytest.mark.asyncio
async def test_synthesize_http_error_prefix(monkeypatch):
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(503, text="overloaded")

    _patch_transport(monkeypatch, handler)
    client = AudioClient(base_url="https://gw.example/v1", api_key="sk-test")

    with pytest.raises(RuntimeError) as exc_info:
        await client.synthesize(model="tts-1", text="hi")

    assert str(exc_info.value).startswith("AI_TTS_HTTP_503")


@pytest.mark.asyncio
async def test_synthesize_transport_error_prefix(monkeypatch):
    def handler(request: httpx.Request) -> httpx.Response:
        raise httpx.ConnectError("connection refused")

    _patch_transport(monkeypatch, handler)
    client = AudioClient(base_url="https://gw.example/v1", api_key="sk-test")

    with pytest.raises(RuntimeError) as exc_info:
        await client.synthesize(model="tts-1", text="hi")

    assert str(exc_info.value).startswith("AI_TTS_TRANSPORT")


def test_audio_client_rejects_private_base_url_by_default():
    with pytest.raises(ValueError, match="Unsafe audio base_url"):
        AudioClient(base_url="https://169.254.169.254/latest", api_key="sk-test")
