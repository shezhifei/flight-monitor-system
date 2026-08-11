"""FastAPI host entrypoint for the Python AI sidecar.

Exposes the internal AI runtime routes under ``/internal/ai/v1``.
"""

from __future__ import annotations

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from src.infrastructure.ai.api_routes import router as ai_runtime_router
from src.infrastructure.ai.service_identity import require_service_identity
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

app = FastAPI(title="FMS AI Sidecar", version="1.0.0")
app.include_router(ai_runtime_router)


@app.get("/internal/ai/v1/health")
async def _health() -> JSONResponse:
    return JSONResponse({"status": "healthy", "service": "ai-runtime"})


@app.get("/internal/ai/v1/ontology/schema")
async def _ontology_schema(request: Request) -> JSONResponse:
    require_service_identity(request)
    from src.infrastructure.ai.ontology.schema_mirror import schema_mirror

    schema = schema_mirror._schema_cache or {"version": "1.0.0", "objects": {}}
    return JSONResponse(schema)


@app.api_route("/api/v2/ai/nl-query", methods=["GET", "POST", "PUT", "PATCH", "DELETE"])
async def _legacy_api_v2_nl_query() -> JSONResponse:
    return JSONResponse(
        {"success": False, "error": "Legacy v2 endpoint is retired", "code": "LEGACY_ENDPOINT_RETIRED"},
        status_code=410,
    )


@app.on_event("startup")
async def _log_startup() -> None:
    logger.info("ai_sidecar_entrypoint_started")
