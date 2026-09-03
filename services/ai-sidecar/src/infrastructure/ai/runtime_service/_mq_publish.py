"""MQ durable publish helpers for the AI runtime (P0-5, Task D1)."""

from __future__ import annotations

import asyncio
import hashlib
import json
import logging
import time
from typing import Any

logger = logging.getLogger(__name__)

# Rust `handle_checkpoint` acks-and-drops checkpoints whose snapshot exceeds
# a 64KB budget, so keep sidecar snapshots comfortably below that.
_CHECKPOINT_SNAPSHOT_BUDGET_BYTES = 56_000

# Sidecar checkpoint type names mapped to the Rust `CheckpointType`
# serde representation (snake_case enum variants).
_CHECKPOINT_TYPE_TO_MQ = {
    "before_proposal": "before_proposal_ingest",
}


def _resolve_mq_publisher() -> Any | None:
    """Return the process-wide MQ event publisher, or ``None`` when absent.

    The publisher is registered by the MQ composition root. When
    absent (degraded mode, no mq-gateway URL configured) the run
    terminal events go out only over SSE — the durable guarantee is
    silently skipped.
    """
    try:
        from src.infrastructure.ai.ai_container import get_ai_container

        return get_ai_container().resolve("mq_event_publisher", None)
    except Exception:  # noqa: BLE001 - composition root lookups are best-effort
        return None


def _resolve_mq_gate() -> Any | None:
    """Return the process-wide :class:`ToolMqGate`, or ``None`` when absent."""
    try:
        from src.infrastructure.ai.ai_container import get_ai_container

        return get_ai_container().resolve("tool_mq_gate", None)
    except Exception:  # noqa: BLE001
        return None


def _encode_checkpoint_snapshot(snapshot: dict[str, Any]) -> tuple[dict[str, Any], str, int]:
    """Return ``(snapshot, sha256_hex, size_bytes)`` within the Rust budget.

    When the serialized snapshot exceeds the budget, evidence payloads inside
    the nested working-memory snapshot are stripped (summaries and pointers
    survive; the full content is re-derivable from the workspace on resume).
    """
    encoded = json.dumps(snapshot, sort_keys=True, default=str)
    size = len(encoded.encode("utf-8"))
    if size > _CHECKPOINT_SNAPSHOT_BUDGET_BYTES:
        working_memory = snapshot.get("working_memory")
        if isinstance(working_memory, dict):
            evidence = working_memory.get("evidence.json")
            if isinstance(evidence, list):
                trimmed: list[Any] = []
                for record in evidence:
                    if isinstance(record, dict):
                        record = {**record, "content": "", "content_trimmed": True}
                    trimmed.append(record)
                snapshot = {
                    **snapshot,
                    "working_memory": {**working_memory, "evidence.json": trimmed},
                }
                encoded = json.dumps(snapshot, sort_keys=True, default=str)
                size = len(encoded.encode("utf-8"))
    return snapshot, hashlib.sha256(encoded.encode("utf-8")).hexdigest(), size


async def _publish_checkpoint_mq(
    publisher: Any | None,
    gate: Any | None,
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    checkpoint_type: str,
    snapshot: dict[str, Any],
    tool_call_pk: str | None = None,
    proposal_id: str | None = None,
) -> bool:
    """Publish a ``checkpoint`` MQ event (Task D1).

    Best-effort with retries: unlike the terminal events, a lost checkpoint
    only means resume falls back to an earlier one, so failures are logged
    (and metered) but never raise into the streaming loop. Returns True when
    the event was handed to the gateway.
    """
    if publisher is None:
        return False

    snapshot, snapshot_hash, size_bytes = _encode_checkpoint_snapshot(snapshot)
    mq_type = _CHECKPOINT_TYPE_TO_MQ.get(checkpoint_type, checkpoint_type)
    # Epoch-millis sequence: monotonic across processes (resume included)
    # without a shared counter; the DB UNIQUE(run_id, sequence_no) makes a
    # duplicate harmless.
    sequence_no = int(time.time() * 1000)
    checkpoint_id = f"{run_id}:{sequence_no}:{mq_type}"

    event_sequence = sequence_no
    if gate is not None:
        try:  # noqa: SIM105 - preserve best-effort async error handling
            event_sequence = await gate.next_event_sequence(run_id)
        except Exception:  # noqa: BLE001 - sequence source is best-effort
            pass

    max_retries = 3
    base_delay_ms = 100
    for attempt in range(max_retries):
        try:
            from src.infrastructure.ai.messaging import build_checkpoint

            envelope = build_checkpoint(
                run_id=run_id,
                job_id=job_id,
                round_index=round_index,
                event_sequence=event_sequence,
                idempotency_key=f"{run_id}:checkpoint:{checkpoint_id}",
                checkpoint_id=checkpoint_id,
                sequence_no=sequence_no,
                checkpoint_type=mq_type,
                snapshot_hash=snapshot_hash,
                snapshot=snapshot,
                snapshot_size_bytes=size_bytes,
                tool_call_pk=tool_call_pk,
                proposal_id=proposal_id,
            )
            await publisher.publish(envelope)
            try:
                from src.infrastructure.common.prometheus_metrics import metrics as ai_metrics

                ai_metrics.record_mq_publish_success("checkpoint")
            except Exception:  # noqa: BLE001
                pass
            return True
        except Exception as exc:  # noqa: BLE001 - checkpoint loss degrades resume, not the run
            if attempt == max_retries - 1:
                logger.warning(
                    f"[D1] checkpoint publish failed after {max_retries} attempts (run={run_id}, type={mq_type}): {exc}"
                )
                try:
                    from src.infrastructure.common.prometheus_metrics import metrics as ai_metrics

                    ai_metrics.record_mq_publish_failure("checkpoint", "retry_exhausted")
                except Exception:  # noqa: BLE001
                    pass
                return False
            await asyncio.sleep((base_delay_ms * (2**attempt)) / 1000.0)
    return False


async def _publish_run_complete_mq(
    publisher: Any | None,
    gate: Any | None,
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    output: dict[str, Any],
    proposal_ids: list[str] | None = None,
    terminal_event_id: str | None = None,
    require_durable_ack: bool = True,
) -> bool:
    """Publish a ``run.complete`` MQ event with durability guarantees.

    Returns True if published successfully, False otherwise.
    Raises exception only if require_durable_ack=True and all retries exhausted.
    """
    if publisher is None:
        logger.warning("[P0-5] No MQ publisher available, skipping durable publish")
        return False

    # Retry with exponential backoff (3 attempts)
    max_retries = 3
    base_delay_ms = 100

    for attempt in range(max_retries):
        try:
            from src.infrastructure.ai.messaging import build_run_complete

            envelope = build_run_complete(
                run_id=run_id,
                job_id=job_id,
                round_index=round_index,
                event_sequence=event_sequence,
                idempotency_key=f"{run_id}:complete:{terminal_event_id or ''}".rstrip(":"),
                output_raw=output,
                token_usage=output.get("token_usage") if isinstance(output, dict) else None,
                proposal_ids=proposal_ids or [],
                terminal_event_id=terminal_event_id,
            )
            await publisher.publish(envelope)
            logger.info(f"[P0-5] Durable run.complete published successfully after {attempt + 1} attempt(s)")

            # P0-5-C: Record successful MQ publish
            try:
                from src.infrastructure.common.prometheus_metrics import metrics as ai_metrics

                ai_metrics.record_mq_publish_success("run.complete")
            except Exception:  # noqa: BLE001
                pass  # Metrics are non-critical, don't fail on error

            return True

        except Exception as exc:
            if attempt == max_retries - 1:
                # Final retry failed
                error_msg = f"MQ publish failed after {max_retries} attempts: {exc}"
                if require_durable_ack:
                    logger.error(error_msg)
                    raise RuntimeError(error_msg) from exc
                else:
                    logger.warning(error_msg)

                    # P0-5-C: Record failed MQ publish
                    try:
                        from src.infrastructure.common.prometheus_metrics import metrics as ai_metrics

                        ai_metrics.record_mq_publish_failure("run.complete", "retry_exhausted")
                    except Exception:  # noqa: BLE001
                        pass

                    return False

            # Exponential backoff
            delay_ms = base_delay_ms * (2**attempt)
            logger.warning(
                f"[P0-5] MQ publish attempt {attempt + 1}/{max_retries} failed for run={run_id}, "
                f"retrying in {delay_ms}ms...",
                exc_info=True,
            )
            await asyncio.sleep(delay_ms / 1000.0)


async def _publish_run_fail_mq(
    publisher: Any | None,
    *,
    run_id: str,
    job_id: str,
    round_index: int,
    event_sequence: int,
    error_code: str,
    error_message: str,
    terminal_event_id: str | None = None,
    require_durable_ack: bool = True,
    blocked_by: str | None = None,
    rule: str | None = None,
    detail: str | None = None,
) -> bool:
    """Publish a ``run.fail`` MQ event with durability guarantees.

    Returns True if published successfully, False otherwise.
    Raises exception only if require_durable_ack=True and all retries exhausted.
    """
    if publisher is None:
        logger.warning("[P0-5] No MQ publisher available, skipping durable publish")
        return False

    # Retry with exponential backoff (3 attempts)
    max_retries = 3
    base_delay_ms = 100

    for attempt in range(max_retries):
        try:
            from src.infrastructure.ai.messaging import build_run_fail

            envelope = build_run_fail(
                run_id=run_id,
                job_id=job_id,
                round_index=round_index,
                event_sequence=event_sequence,
                idempotency_key=f"{run_id}:fail:{terminal_event_id or ''}".rstrip(":"),
                error_code=error_code,
                error_message=error_message,
                terminal_event_id=terminal_event_id,
                blocked_by=blocked_by,
                rule=rule,
                detail=detail,
            )
            await publisher.publish(envelope)
            logger.info(f"[P0-5] Durable run.fail published successfully after {attempt + 1} attempt(s)")

            # P0-5-C: Record successful MQ publish
            try:
                from src.infrastructure.common.prometheus_metrics import metrics as ai_metrics

                ai_metrics.record_mq_publish_success("run.fail")
            except Exception:  # noqa: BLE001
                pass  # Metrics are non-critical, don't fail on error

            return True

        except Exception as exc:
            if attempt == max_retries - 1:
                # Final retry failed
                error_msg = f"MQ publish failed after {max_retries} attempts: {exc}"
                if require_durable_ack:
                    logger.error(error_msg)
                    raise RuntimeError(error_msg) from exc
                else:
                    logger.warning(error_msg)
                    return False

            # Exponential backoff
            delay_ms = base_delay_ms * (2**attempt)
            logger.warning(
                f"[P0-5] MQ publish attempt {attempt + 1}/{max_retries} failed for run={run_id}, "
                f"retrying in {delay_ms}ms...",
                exc_info=True,
            )
            await asyncio.sleep(delay_ms / 1000.0)
