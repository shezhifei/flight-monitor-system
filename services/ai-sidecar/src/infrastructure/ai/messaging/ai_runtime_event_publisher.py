"""RocketMQ publisher for the ``ai.runtime.events`` control channel.

Mirrors the Rust ``MessageQueueGatewayClient`` shape: the sidecar does
not speak RocketMQ directly, it talks to the Rust ``mq-gateway`` HTTP
endpoint, which performs the actual publish. The gateway is responsible
for ordered routing by ``Message Key`` (= ``run_id``) and the Rust
consumer handles per-run serialization, retry, and DLQ.

Produces the envelope dict, hands it to the gateway, and retries
on transient transport errors.
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any, Literal

import httpx
import ulid

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


AI_RUNTIME_EVENTS_TOPIC: str = "ai.runtime.events"
SCHEMA_VERSION: int = 1

EventTypeStr = Literal[
    "tool_call_requested",
    "tool_result",
    "checkpoint",
    "heartbeat",
    "run_complete",
    "run_fail",
]

TagStr = Literal[
    "tool.call.requested",
    "tool.result",
    "checkpoint",
    "heartbeat",
    "run.complete",
    "run.fail",
]


_EVENT_TYPE_TO_TAG: dict[EventTypeStr, TagStr] = {
    "tool_call_requested": "tool.call.requested",
    "tool_result": "tool.result",
    "checkpoint": "checkpoint",
    "heartbeat": "heartbeat",
    "run_complete": "run.complete",
    "run_fail": "run.fail",
}


class AiRuntimeEventPublishError(RuntimeError):
    """Raised when the gateway rejected or failed to deliver an event."""


@dataclass(frozen=True)
class PublishConfig:
    base_url: str
    api_key: str | None = None
    timeout: float = 5.0
    max_retries: int = 3
    backoff_seconds: float = 0.25
    publish_path: str = "/mq/messages"

    def url(self) -> str:
        return f"{self.base_url.rstrip('/')}{self.publish_path}"


def _new_event_id() -> str:
    new_fn = getattr(ulid, "new", None)
    if callable(new_fn):
        return str(new_fn())
    ulid_cls = getattr(ulid, "ULID", None)
    if ulid_cls is not None:
        return str(ulid_cls())
    raise RuntimeError("No supported ULID generator found in 'ulid' module")


def _utcnow_iso() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


def _make_envelope(
    event_type: EventTypeStr,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    payload: dict[str, Any],
    event_id: str | None = None,
) -> dict[str, Any]:
    return {
        "event_id": event_id or _new_event_id(),
        "event_type": event_type,
        "run_id": run_id,
        "job_id": job_id,
        "round_index": round_index,
        "event_sequence": event_sequence,
        "idempotency_key": idempotency_key,
        "emitted_at": _utcnow_iso(),
        "schema_version": SCHEMA_VERSION,
        "payload": payload,
    }


def _envelope_to_wire(
    envelope: dict[str, Any],
    *,
    message_key: str | None = None,
    tag: TagStr | None = None,
) -> dict[str, Any]:
    wire: dict[str, Any] = {
        "topic": AI_RUNTIME_EVENTS_TOPIC,
        "body": envelope,
    }
    if message_key is None:
        message_key = envelope.get("run_id")
    if message_key is not None:
        wire["message_key"] = message_key
    if tag is None:
        tag = _EVENT_TYPE_TO_TAG[envelope["event_type"]]
    wire["tag"] = tag
    return wire


def build_tool_call_requested(
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    tool_call_pk: str,
    tool_call_id: str,
    tool_name: str,
    tool_type: str,
    args_hash: str,
    args_summary: dict[str, Any],
    authorization_mode: Literal["public_direct", "rust_pdp"],
    max_retries: int = 2,
    timeout_seconds: int = 30,
    parent_tool_call_pk: str | None = None,
    depth: int = 0,
    event_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "tool_call_pk": tool_call_pk,
        "tool_call_id": tool_call_id,
        "tool_name": tool_name,
        "tool_type": tool_type,
        "parent_tool_call_pk": parent_tool_call_pk,
        "depth": depth,
        "args_hash": args_hash,
        "args_summary": args_summary,
        "authorization_mode": authorization_mode,
        "max_retries": max_retries,
        "timeout_seconds": timeout_seconds,
    }
    return _make_envelope(
        "tool_call_requested",
        run_id,
        job_id,
        round_index,
        event_sequence,
        idempotency_key,
        payload,
        event_id=event_id,
    )


def build_tool_result(
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    tool_call_pk: str,
    tool_call_id: str,
    tool_name: str,
    status: Literal["succeeded", "failed", "cancelled", "expired", "denied", "proposal_only"],
    duration_ms: int,
    retry_count: int = 0,
    result_hash: str | None = None,
    result_summary: dict[str, Any] | None = None,
    error_code: str | None = None,
    error_message: str | None = None,
    proposal_ids: list[str] | None = None,
    event_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "tool_call_pk": tool_call_pk,
        "tool_call_id": tool_call_id,
        "tool_name": tool_name,
        "status": status,
        "result_hash": result_hash,
        "result_summary": result_summary,
        "error_code": error_code,
        "error_message": error_message,
        "retry_count": retry_count,
        "proposal_ids": proposal_ids or [],
        "duration_ms": duration_ms,
    }
    return _make_envelope(
        "tool_result",
        run_id,
        job_id,
        round_index,
        event_sequence,
        idempotency_key,
        payload,
        event_id=event_id,
    )


def build_checkpoint(
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    checkpoint_id: str,
    sequence_no: int,
    checkpoint_type: str,
    snapshot_hash: str,
    snapshot: dict[str, Any],
    snapshot_size_bytes: int,
    tool_call_pk: str | None = None,
    proposal_id: str | None = None,
    event_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "checkpoint_id": checkpoint_id,
        "sequence_no": sequence_no,
        "checkpoint_type": checkpoint_type,
        "tool_call_pk": tool_call_pk,
        "proposal_id": proposal_id,
        "snapshot_hash": snapshot_hash,
        "snapshot": snapshot,
        "snapshot_size_bytes": snapshot_size_bytes,
    }
    return _make_envelope(
        "checkpoint",
        run_id,
        job_id,
        round_index,
        event_sequence,
        idempotency_key,
        payload,
        event_id=event_id,
    )


def build_heartbeat(
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    tool_call_pk: str,
    progress_pct: int | None = None,
    note: str | None = None,
    event_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "tool_call_pk": tool_call_pk,
        "progress_pct": progress_pct,
        "note": note,
    }
    return _make_envelope(
        "heartbeat",
        run_id,
        job_id,
        round_index,
        event_sequence,
        idempotency_key,
        payload,
        event_id=event_id,
    )


def build_run_complete(
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    output_raw: dict[str, Any],
    token_usage: dict[str, Any] | None = None,
    proposal_ids: list[str] | None = None,
    terminal_event_id: str | None = None,
    event_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "output_raw": output_raw,
        "token_usage": token_usage,
        "proposal_ids": proposal_ids or [],
        "terminal_event_id": terminal_event_id,
    }
    return _make_envelope(
        "run_complete",
        run_id,
        job_id,
        round_index,
        event_sequence,
        idempotency_key,
        payload,
        event_id=event_id,
    )


def build_run_fail(
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    idempotency_key: str,
    error_code: str,
    error_message: str,
    terminal_event_id: str | None = None,
    event_id: str | None = None,
) -> dict[str, Any]:
    payload = {
        "error_code": error_code,
        "error_message": error_message,
        "terminal_event_id": terminal_event_id,
    }
    return _make_envelope(
        "run_fail",
        run_id,
        job_id,
        round_index,
        event_sequence,
        idempotency_key,
        payload,
        event_id=event_id,
    )


class AiRuntimeEventPublisher:
    """HTTP client for the Rust ``mq-gateway``.

    The publisher accepts already-built envelope dicts (see the
    ``build_*`` helpers in this module) and POSTs them to the gateway.
    The gateway is responsible for the actual RocketMQ publish; the
    sidecar only owns retry on transient transport errors.
    """

    def __init__(
        self,
        config: PublishConfig,
        *,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        self._config = config
        self._owns_client = client is None
        self._client = client or httpx.AsyncClient(timeout=config.timeout)

    async def aclose(self) -> None:
        if self._owns_client:
            await self._client.aclose()

    async def __aenter__(self) -> AiRuntimeEventPublisher:
        return self

    async def __aexit__(self, exc_type: type[BaseException] | None, exc: BaseException | None, tb: Any) -> None:
        await self.aclose()

    async def publish(self, event: dict[str, Any]) -> str:
        envelope = event
        wire = _envelope_to_wire(envelope)
        last_exc: Exception | None = None
        attempts = self._config.max_retries + 1

        for attempt in range(1, attempts + 1):
            try:
                response = await self._client.post(
                    self._config.url(),
                    json=wire,
                    headers=self._headers(),
                )
                response.raise_for_status()
                message_id = self._extract_message_id(response)
                logger.info(
                    "ai_runtime_event_published",
                    extra={
                        "event_id": envelope.get("event_id"),
                        "event_type": envelope.get("event_type"),
                        "run_id": envelope.get("run_id"),
                        "tag": wire.get("tag"),
                        "message_id": message_id,
                    },
                )
                return message_id
            except httpx.HTTPStatusError as exc:
                status = exc.response.status_code
                if 500 <= status < 600 and attempt < attempts:
                    await self._sleep_backoff(attempt)
                    last_exc = exc
                    continue
                body = exc.response.text
                raise AiRuntimeEventPublishError(f"AI_MQ_HTTP_{status}: gateway rejected event: {body}") from exc
            except httpx.HTTPError as exc:
                if attempt < attempts:
                    await self._sleep_backoff(attempt)
                    last_exc = exc
                    continue
                raise AiRuntimeEventPublishError(
                    f"AI_MQ_TRANSPORT: failed to publish event after {attempts} attempts: {exc}"
                ) from exc

        raise AiRuntimeEventPublishError(f"AI_MQ_TRANSPORT: exhausted retries: {last_exc}") from last_exc

    def _headers(self) -> dict[str, str]:
        headers = {"Content-Type": "application/json"}
        if self._config.api_key:
            headers["Authorization"] = f"Bearer {self._config.api_key}"
        return headers

    @staticmethod
    def _extract_message_id(response: httpx.Response) -> str:
        try:
            payload = response.json()
        except ValueError:
            return str(response.headers.get("X-MQ-Message-Id", ""))
        if isinstance(payload, dict):
            message_id = payload.get("message_id") or payload.get("messageId")
            if isinstance(message_id, str):
                return message_id
        return str(response.headers.get("X-MQ-Message-Id", ""))

    async def _sleep_backoff(self, attempt: int) -> None:
        delay = self._config.backoff_seconds * (2 ** (attempt - 1))
        await asyncio.sleep(delay)


__all__ = [
    "AI_RUNTIME_EVENTS_TOPIC",
    "SCHEMA_VERSION",
    "AiRuntimeEventPublishError",
    "AiRuntimeEventPublisher",
    "PublishConfig",
    "build_checkpoint",
    "build_heartbeat",
    "build_run_complete",
    "build_run_fail",
    "build_tool_call_requested",
    "build_tool_result",
]
