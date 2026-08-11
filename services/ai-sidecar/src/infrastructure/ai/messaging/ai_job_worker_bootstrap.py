"""Composition root for the async AI job worker (ADR-0004).

Wires :class:`AiJobWorker` to its :class:`ServiceIdentityIssuer` and
:class:`AiJobWorkerConfig`. This is the **single composition root** for
the job worker: nothing else constructs ``AiJobWorker`` in production.

Design notes
------------

* **Degrade-closed.** When ``base_url`` or JWT secret is missing, the
  factory returns ``None`` and the caller (typically the worker
  entrypoint) skips the job loop. This matches the MQ control plane's
  degrade-closed policy in :mod:`mq_runtime_bootstrap`.
* **Module-level singleton.** The constructed worker is cached so a
  second call returns the same instance. Resetting requires an explicit
  :func:`reset_ai_job_worker` call (used in tests).
* **No import-time side effects.** All factories are explicit; callers
  decide when the worker loop starts and stops.
"""

from __future__ import annotations

import os
from typing import Any

from src.infrastructure.ai.messaging.ai_job_worker import (
    AiJobWorker,
    AiJobWorkerConfig,
)
from src.infrastructure.ai.messaging.worker_identity import WorkerIdentity
from src.infrastructure.ai.service_identity import get_jwt_secret
from src.infrastructure.ai.service_identity_issuer import ServiceIdentityIssuer
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


def _resolve_rust_api_base_url() -> str | None:
    """Return the Rust internal API base URL from env, or ``None`` when not set."""
    for key in ("AI_INTERNAL_API_URL", "RUST_API_BASE_URL", "AI_API_BASE_URL"):
        value = os.environ.get(key, "").strip()
        if value:
            return value.rstrip("/")
    return None


def _resolve_worker_id() -> str:
    return WorkerIdentity(os.environ.get("WORKER_ID", "").strip()).worker_id


def _resolve_optional_int(key: str, default: int) -> int:
    raw = os.environ.get(key, "").strip()
    if not raw:
        return default
    try:
        return int(raw)
    except ValueError:
        return default


def _resolve_optional_float(key: str, default: float) -> float:
    raw = os.environ.get(key, "").strip()
    if not raw:
        return default
    try:
        return float(raw)
    except ValueError:
        return default


_job_worker: AiJobWorker | None = None


def build_ai_job_worker_from_env() -> AiJobWorker | None:
    """Construct the :class:`AiJobWorker` from environment variables.

    Returns ``None`` (degrade-closed) when:
      * ``AI_INTERNAL_API_URL`` / ``RUST_API_BASE_URL`` / ``AI_API_BASE_URL``
        is not set, or
      * ``JWT_SECRET`` / ``JWT_SECRET_KEY`` is not set.

    Configuration (all optional, with defaults):

    * ``WORKER_ID`` — stable worker identity (auto-generated when absent).
    * ``AI_JOB_TYPE_FILTER`` — only lease jobs of this task type.
    * ``AI_JOB_LEASE_SECONDS`` — lease TTL (default 60).
    * ``AI_JOB_HEARTBEAT_INTERVAL_SECONDS`` — heartbeat period (default 15).
    * ``AI_JOB_POLL_INTERVAL_SECONDS`` — poll interval when no job (default 1.0).
    * ``AI_JOB_REQUEST_TIMEOUT`` — HTTP request timeout (default 10.0).
    * ``AI_JOB_MAX_RETRIES`` — retry count for 5xx/network errors (default 3).
    * ``AI_JOB_MAX_CONCURRENT_RUNS`` — max concurrent run executions (default 1).
    * ``AI_JOB_SHUTDOWN_GRACE_SECONDS`` — drain timeout on shutdown (default 30.0).
    """
    global _job_worker
    if _job_worker is not None:
        return _job_worker

    base_url = _resolve_rust_api_base_url()
    if not base_url:
        logger.warning("ai_job_worker_DEGRADED_no_base_url: AI_INTERNAL_API_URL not set; job worker will not start.")
        return None

    try:
        secret = get_jwt_secret()
    except RuntimeError:
        logger.warning("ai_job_worker_DEGRADED_no_jwt_secret: JWT_SECRET not set; job worker will not start.")
        return None

    worker_id = _resolve_worker_id()
    config = AiJobWorkerConfig(
        base_url=base_url,
        worker_id=worker_id,
        job_type_filter=os.environ.get("AI_JOB_TYPE_FILTER", "").strip() or None,
        lease_seconds=_resolve_optional_int("AI_JOB_LEASE_SECONDS", 60),
        heartbeat_interval_seconds=_resolve_optional_float("AI_JOB_HEARTBEAT_INTERVAL_SECONDS", 15.0),
        poll_interval_seconds=_resolve_optional_float("AI_JOB_POLL_INTERVAL_SECONDS", 1.0),
        request_timeout=_resolve_optional_float("AI_JOB_REQUEST_TIMEOUT", 10.0),
        max_retries=_resolve_optional_int("AI_JOB_MAX_RETRIES", 3),
        max_concurrent_runs=_resolve_optional_int("AI_JOB_MAX_CONCURRENT_RUNS", 1),
        shutdown_grace_seconds=_resolve_optional_float("AI_JOB_SHUTDOWN_GRACE_SECONDS", 30.0),
    )
    issuer = ServiceIdentityIssuer(secret)

    _job_worker = AiJobWorker(config, issuer)
    logger.info(
        "ai_job_worker_constructed worker_id=%s base_url=%s",
        worker_id,
        base_url,
    )
    return _job_worker


def get_ai_job_worker() -> AiJobWorker | None:
    """Return the cached worker, or ``None`` if not constructed."""
    return _job_worker


def reset_ai_job_worker() -> None:
    """Reset the cached worker (used in tests)."""
    global _job_worker
    _job_worker = None


__all__: list[Any] = [
    "build_ai_job_worker_from_env",
    "get_ai_job_worker",
    "reset_ai_job_worker",
]
