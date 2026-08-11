#!/usr/bin/env python3
"""AI Sidecar Entrypoint - Provides internal AI Runtime API for Rust control plane."""

from __future__ import annotations

import asyncio
import logging
import os
import signal
import sys
from pathlib import Path
from typing import Any

project_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(project_root / "services" / "ai-sidecar"))

from fastapi import FastAPI, Request, Response
from fastapi.responses import JSONResponse
import uvicorn

from src.infrastructure.ai.ontology.schema_mirror import schema_mirror
from src.infrastructure.ai.service_identity import (
    require_service_identity,
)
from src.infrastructure.ai.api_routes import router as api_routes
from src.infrastructure.ai.management_routes import router as management_routes

logger = logging.getLogger(__name__)


_docs_enabled = os.getenv("AI_SIDECAR_ENABLE_DOCS", "").strip().lower() in {"1", "true", "yes", "on"}

app = FastAPI(
    title="FMS AI Runtime",
    version="0.1.0",
    docs_url="/docs" if _docs_enabled else None,
    redoc_url="/redoc" if _docs_enabled else None,
    openapi_url="/openapi.json" if _docs_enabled else None,
)

app.include_router(api_routes)
app.include_router(management_routes)

@app.exception_handler(Exception)
async def unhandled_exception_handler(request: Request, exc: Exception) -> JSONResponse:
    """Never leak raw exception text to clients; log full detail server-side."""
    logger.error(
        "ai_sidecar_unhandled_exception path=%s method=%s",
        request.url.path,
        request.method,
        exc_info=exc,
    )
    return JSONResponse(
        status_code=500,
        content={"detail": "Internal server error"},
    )


@app.middleware("http")
async def add_security_headers(request: Request, call_next) -> Response:
    response = await call_next(request)
    response.headers.setdefault("X-Content-Type-Options", "nosniff")
    response.headers.setdefault("X-Frame-Options", "DENY")
    response.headers.setdefault("Referrer-Policy", "no-referrer")
    response.headers.setdefault("Cache-Control", "no-store")
    return response


@app.get("/internal/ai/v1/health")
async def health() -> JSONResponse:
    return JSONResponse({"status": "healthy", "service": "ai-runtime"})


@app.get("/metrics")
async def prometheus_metrics() -> Response:
    from prometheus_client import REGISTRY, generate_latest

    return Response(generate_latest(REGISTRY), media_type="text/plain; version=0.0.4")


@app.get("/internal/ai/v1/ontology/schema")
async def get_schema(request: Request) -> JSONResponse:
    require_service_identity(request)
    if not schema_mirror._schema_cache:
        schema_mirror.load_schema_snapshot()
    return JSONResponse(schema_mirror._schema_cache)


@app.post("/internal/ai/v1/ontology/snapshot/load")
async def load_snapshot(request: Request) -> JSONResponse:
    require_service_identity(request)
    try:
        schema = schema_mirror.load_schema_snapshot()
        return JSONResponse({"status": "success", "version": schema.get("version")})
    except Exception as exc:
        logger.error("ai_sidecar_internal_handler_failed", exc_info=exc)
        return JSONResponse({"status": "error", "message": "Internal server error"}, status_code=500)


@app.post("/internal/ai/v1/runs/{run_id}/complete")
async def complete_run_with_structured_output(request: Request, run_id: str) -> JSONResponse:
    require_service_identity(request)
    body = await request.json()
    try:
        from src.infrastructure.ai.structured_output import AiStructuredOutput
        _output = AiStructuredOutput(**body)
        return JSONResponse({"success": True, "run_id": run_id, "status": "completed"})
    except Exception as exc:
        logger.error("ai_sidecar_structured_output_validation_failed", exc_info=exc)
        return JSONResponse({"success": False, "error": "validation_failed"}, status_code=422)


@app.api_route("/internal/ai/v1/runs/{run_id}/events", methods=["POST"])
async def ingest_run_event(request: Request, run_id: str) -> JSONResponse:
    require_service_identity(request)
    return JSONResponse({
        "success": False,
        "message": "AI Runtime endpoint not yet implemented",
        "data": None,
        "degraded": True,
    }, status_code=503)


@app.api_route("/internal/ai/v1/runs/{run_id}/fail", methods=["POST"])
async def fail_run(request: Request, run_id: str) -> JSONResponse:
    require_service_identity(request)
    return JSONResponse({
        "success": False,
        "message": "AI Runtime endpoint not yet implemented",
        "data": None,
        "degraded": True,
    }, status_code=503)


@app.api_route("/api/v2/health/ping")
async def legacy_health_ping() -> JSONResponse:
    return JSONResponse({"status": "healthy", "service": "ai-sidecar-legacy"})


@app.api_route("/api/v2/{full_path:path}", methods=["GET", "POST", "PUT", "DELETE", "PATCH"])
async def legacy_proxy_endpoint(request: Request, full_path: str) -> JSONResponse:
    return JSONResponse({
        "success": False,
        "message": "This endpoint is deprecated. Use /internal/ai/v1/* via Rust gateway.",
        "data": None,
        "degraded": True,
    }, status_code=410)


async def startup_event() -> None:
    print("FMS AI Runtime started on port 9000")
    try:
        from src.infrastructure.ai.ai_runtime_bootstrap import bootstrap_ai_runtime_from_env

        registered = await bootstrap_ai_runtime_from_env()
        if registered:
            print("AI runtime DI graph bootstrapped (v2 capability stack registered)")
        else:
            print("AI runtime DI graph degraded (no DB-backed capability stack)")
    except Exception as exc:  # defensive: never block startup on DI wiring
        print(f"AI runtime DI bootstrap error (continuing degraded): {exc}")


async def shutdown_event() -> None:
    print("FMS AI Runtime shutting down")


app.add_event_handler("startup", startup_event)
app.add_event_handler("shutdown", shutdown_event)


def main() -> None:
    host = os.getenv("API_HOST", "127.0.0.1")
    port = int(os.getenv("API_PORT", "9000"))

    print(f"Starting FMS AI Runtime on {host}:{port}")
    print("Health endpoint: http://{host}:{port}/internal/ai/v1/health")
    print("Legacy health endpoint: http://{host}:{port}/api/v2/health/ping")

    shutdown_event_set = asyncio.Event()

    def signal_handler(signum: int, _frame: Any) -> None:
        print(f"Received signal {signum}, initiating shutdown...")
        shutdown_event_set.set()

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    config = uvicorn.Config(
        app,
        host=host,
        port=port,
        log_level="info",
        access_log=True,
    )
    server = uvicorn.Server(config)

    asyncio.run(server.serve())
    print("FMS AI Runtime server stopped")


if __name__ == "__main__":
    main()
