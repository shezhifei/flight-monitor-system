"""Eval Lab HTTP routes (Task G5).

Serves the ``/api/v2/ai/eval/*`` public surface (mapped here by the Rust
``map_to_internal_path`` proxy): eval jobs are created/listed/inspected
against the persistent ``ai_eval_jobs`` / ``ai_eval_metrics_summary``
tables. No in-memory job registry — a process restart must not lose jobs.

Fail-closed: without a Postgres pool the routes answer 503 instead of
faking an empty lab; background runs are only started through the injected
``RuntimeServiceEvalRunner``.
"""

from __future__ import annotations

import asyncio
from typing import Any
from uuid import UUID

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

from src.application.services.ai.llm_eval_service import EvalJob, EvaluationService
from src.infrastructure.ai.service_identity import require_service_identity
from src.infrastructure.common.exceptions import JSON_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

router = APIRouter(prefix="/internal/ai/v1/eval", tags=["AI Eval Lab"])
logger = get_logger(__name__)

# Strong references so background eval runs are not garbage-collected mid-flight.
_BACKGROUND_TASKS: set[asyncio.Task[None]] = set()


def _resolve_pool() -> Any:
    """Shared Postgres pool from the composition root (None when degraded)."""
    try:
        from src.infrastructure.ai.ai_container import get_ai_container

        container = get_ai_container()
        pool = container.resolve("pg_shared_context_pool", None)
        if pool is None:
            pool = container.resolve("db_pool", None)
        return pool
    except Exception:  # noqa: BLE001 - composition root lookups are best-effort
        return None


def _resolve_runner() -> Any:
    """Production runner over the runtime service (None when unavailable)."""
    try:
        from src.application.services.ai.llm_eval_service import RuntimeServiceEvalRunner
        from src.infrastructure.ai.runtime_service import get_runtime_service

        return RuntimeServiceEvalRunner(get_runtime_service())
    except Exception:  # noqa: BLE001 - runner construction is best-effort here
        return None


def _build_service() -> EvaluationService | None:
    pool = _resolve_pool()
    if pool is None:
        return None
    return EvaluationService(db_pool=pool, agent_runner=_resolve_runner())


def _db_unavailable() -> JSONResponse:
    return JSONResponse(
        {
            "success": False,
            "code": "EVAL_DB_UNAVAILABLE",
            "message": "eval persistence pool unavailable",
        },
        status_code=503,
    )


async def _run_job_background(service: EvaluationService, job: EvalJob) -> None:
    """Execute a job without blocking the HTTP request (run_job persists state)."""
    try:
        await service.run_job(job)
    except Exception as exc:  # noqa: BLE001 - background task must not die silently
        logger.error(
            "eval_job_background_run_failed",
            job_id=str(job.job_id),
            error=repr(exc),
        )


@router.post("/jobs")
async def create_eval_job(request: Request) -> JSONResponse:
    """Create (and optionally start) a persistent eval job."""
    require_service_identity(request)

    try:
        body = await request.json()
    except JSON_EXCEPTIONS:
        return JSONResponse(
            {"success": False, "code": "MALFORMED_JSON", "message": "Malformed JSON request body"},
            status_code=400,
        )

    service = _build_service()
    if service is None:
        return _db_unavailable()

    dataset_path = str(body.get("dataset_path") or "").strip()
    run_now = bool(body.get("run", False))
    if run_now and not dataset_path:
        return JSONResponse(
            {"success": False, "code": "DATASET_REQUIRED", "message": "run=true requires dataset_path"},
            status_code=400,
        )

    metrics_config = body.get("metrics_config") or {}
    if not isinstance(metrics_config, dict):
        return JSONResponse(
            {"success": False, "code": "INVALID_METRICS_CONFIG", "message": "metrics_config must be an object"},
            status_code=422,
        )

    job = await service.create_job(
        name=str(body.get("name") or "agent eval"),
        dataset_path=dataset_path,
        metrics_config=metrics_config,
        description=str(body.get("description") or ""),
    )

    if run_now:
        task = asyncio.create_task(_run_job_background(service, job))
        _BACKGROUND_TASKS.add(task)
        task.add_done_callback(_BACKGROUND_TASKS.discard)

    return JSONResponse(
        {"success": True, "data": {"job_id": str(job.job_id), "status": job.status}},
        status_code=201,
    )


@router.get("/jobs")
async def list_eval_jobs(request: Request) -> JSONResponse:
    """List persistent eval jobs, newest first."""
    require_service_identity(request)

    service = _build_service()
    if service is None:
        return _db_unavailable()

    try:
        limit = int(request.query_params.get("limit", "30"))
    except ValueError:
        limit = 30
    items = await service.list_jobs(limit)
    return JSONResponse({"success": True, "data": {"items": items}})


def _parse_job_id(job_id: str) -> UUID | None:
    try:
        return UUID(job_id)
    except (ValueError, AttributeError, TypeError):
        return None


@router.get("/jobs/{job_id}")
async def get_eval_job(request: Request, job_id: str) -> JSONResponse:
    """Job detail including its gate table."""
    require_service_identity(request)

    parsed = _parse_job_id(job_id)
    if parsed is None:
        return JSONResponse({"success": False, "code": "INVALID_JOB_ID"}, status_code=422)

    service = _build_service()
    if service is None:
        return _db_unavailable()

    detail = await service.get_job_detail(parsed)
    if detail is None:
        return JSONResponse({"success": False, "code": "JOB_NOT_FOUND"}, status_code=404)
    return JSONResponse({"success": True, "data": detail})


@router.post("/jobs/{job_id}/cancel")
async def cancel_eval_job(request: Request, job_id: str) -> JSONResponse:
    """Cancel an active job (marks it failed with 'cancelled by user')."""
    require_service_identity(request)

    parsed = _parse_job_id(job_id)
    if parsed is None:
        return JSONResponse({"success": False, "code": "INVALID_JOB_ID"}, status_code=422)

    service = _build_service()
    if service is None:
        return _db_unavailable()

    cancelled = await service.cancel_job(parsed)
    if cancelled is None:
        return JSONResponse({"success": False, "code": "JOB_NOT_CANCELLABLE"}, status_code=409)
    return JSONResponse({"success": True, "data": cancelled})
