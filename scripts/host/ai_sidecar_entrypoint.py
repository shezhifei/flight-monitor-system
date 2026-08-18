#!/usr/bin/env python3
# ruff: noqa: E402 - executable entrypoint must add the sidecar source root before local imports
"""AI Sidecar Entrypoint - Provides internal AI Runtime API for Rust control plane."""

from __future__ import annotations

import logging
import os
import sys
from pathlib import Path

project_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(project_root / "services" / "ai-sidecar"))

import uvicorn
from fastapi import FastAPI, Request, Response
from fastapi.responses import JSONResponse

from src.infrastructure.ai.api_routes import router as api_routes
from src.infrastructure.ai.ai_runtime_bootstrap import ai_runtime_lifespan
from src.infrastructure.ai.eval_routes import router as eval_routes
from src.infrastructure.ai.management_routes import router as management_routes
from src.infrastructure.ai.ontology.schema_mirror import schema_mirror
from src.infrastructure.ai.service_identity import (
    require_service_identity,
)

logger = logging.getLogger(__name__)


_docs_enabled = os.getenv("AI_SIDECAR_ENABLE_DOCS", "").strip().lower() in {"1", "true", "yes", "on"}

app = FastAPI(
    title="FMS AI Runtime",
    version="0.1.0",
    docs_url="/docs" if _docs_enabled else None,
    redoc_url="/redoc" if _docs_enabled else None,
    openapi_url="/openapi.json" if _docs_enabled else None,
    lifespan=ai_runtime_lifespan,
)

app.include_router(api_routes)
app.include_router(management_routes)
app.include_router(eval_routes)


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
async def prometheus_metrics(request: Request) -> Response:
    # 仅允许环回来源匿名抓取（本机 Prometheus 直连）；
    # 其它来源必须携带服务身份令牌，避免内部运行指标泄露。
    client_host = request.client.host if request.client else None
    if client_host not in {"127.0.0.1", "::1"}:
        require_service_identity(request)

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


def main() -> None:
    host = os.getenv("API_HOST", "127.0.0.1")
    port = int(os.getenv("API_PORT", "9000"))

    print(f"Starting FMS AI Runtime on {host}:{port}")
    print(f"Health endpoint: http://{host}:{port}/internal/ai/v1/health")

    config = uvicorn.Config(
        app,
        host=host,
        port=port,
        log_level="info",
        access_log=True,
    )
    server = uvicorn.Server(config)

    server.run()
    print("FMS AI Runtime server stopped")


if __name__ == "__main__":
    main()
