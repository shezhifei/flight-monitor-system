"""ASR/TTS execution adapter for OpenAI-compatible audio endpoints.

Thin ``httpx``-based client that talks to the gateway's
``/audio/transcriptions`` (speech-to-text) and ``/audio/speech``
(text-to-speech) routes. Network/transport failures are normalized into
``RuntimeError`` carrying an ``AI_ASR_*`` / ``AI_TTS_*`` prefix so callers
can branch on a stable, transport-agnostic error code.
"""

from __future__ import annotations

import httpx

from src.infrastructure.ai.security.url_guard import UnsafeUrlError, validate_external_http_url
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AudioClient:
    """OpenAI-compatible audio (ASR/TTS) gateway client."""

    def __init__(self, base_url: str, api_key: str, timeout: float = 30.0):
        try:
            self.base_url = validate_external_http_url(base_url, purpose="audio base_url")
        except UnsafeUrlError as exc:
            raise ValueError(str(exc)) from exc
        self.api_key = api_key
        self.timeout = float(timeout)

    def _auth_headers(self) -> dict:
        return {"Authorization": f"Bearer {self.api_key}"}

    async def transcribe(
        self,
        model: str,
        audio_bytes: bytes,
        mime: str,
        language: str | None = None,
    ) -> str:
        """Transcribe audio to text via ``POST {base_url}/audio/transcriptions``.

        Sends a multipart request mirroring the OpenAI transcription API and
        returns the recognized ``text``. Errors are raised as ``RuntimeError``
        prefixed with ``AI_ASR_*``.
        """
        url = f"{self.base_url}/audio/transcriptions"
        files = {"file": ("audio", audio_bytes, mime)}
        data: dict = {"model": model}
        if language is not None:
            data["language"] = language

        try:
            async with httpx.AsyncClient(timeout=self.timeout) as client:
                response = await client.post(
                    url,
                    headers=self._auth_headers(),
                    files=files,
                    data=data,
                )
                response.raise_for_status()
        except httpx.HTTPStatusError as exc:
            body = exc.response.text
            logger.warning(
                "ASR transcription failed: status=%s, body=%s",
                exc.response.status_code,
                body,
            )
            raise RuntimeError(f"AI_ASR_HTTP_{exc.response.status_code}: transcription request failed: {body}") from exc
        except httpx.HTTPError as exc:
            logger.warning("ASR transport error: %s", exc)
            raise RuntimeError(f"AI_ASR_TRANSPORT: {exc}") from exc

        try:
            payload = response.json()
        except ValueError as exc:
            raise RuntimeError(f"AI_ASR_DECODE: invalid JSON response: {exc}") from exc

        text = payload.get("text")
        if not isinstance(text, str):
            raise RuntimeError("AI_ASR_DECODE: response missing 'text' field")
        return text

    async def synthesize(
        self,
        model: str,
        text: str,
        voice: str = "alloy",
        fmt: str = "wav",
    ) -> bytes:
        """Synthesize speech from text via ``POST {base_url}/audio/speech``.

        Returns the raw audio bytes in ``fmt``. Errors are raised as
        ``RuntimeError`` prefixed with ``AI_TTS_*``.
        """
        url = f"{self.base_url}/audio/speech"
        body = {
            "model": model,
            "input": text,
            "voice": voice,
            "response_format": fmt,
        }

        try:
            async with httpx.AsyncClient(timeout=self.timeout) as client:
                response = await client.post(
                    url,
                    headers=self._auth_headers(),
                    json=body,
                )
                response.raise_for_status()
        except httpx.HTTPStatusError as exc:
            detail = exc.response.text
            logger.warning(
                "TTS synthesis failed: status=%s, body=%s",
                exc.response.status_code,
                detail,
            )
            raise RuntimeError(f"AI_TTS_HTTP_{exc.response.status_code}: speech request failed: {detail}") from exc
        except httpx.HTTPError as exc:
            logger.warning("TTS transport error: %s", exc)
            raise RuntimeError(f"AI_TTS_TRANSPORT: {exc}") from exc

        return response.content
