"""AI Runtime API - Internal endpoints for Rust control plane.

HTTP layer only: service identity, request parsing, response serialization.
Business orchestration lives in runtime_service.py.
"""

from __future__ import annotations

import os
from typing import Any

import orjson
from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse, StreamingResponse

from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.runtime_service import (
    STATUS_FAILED,
    RuntimeService,
    get_runtime_service,
    structured_output_to_response_dict,
)
from src.infrastructure.ai.service_identity import require_service_identity
from src.infrastructure.common.exceptions import JSON_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

router = APIRouter(prefix="/internal/ai/v1", tags=["AI Runtime Internal"])
logger = get_logger(__name__)


def _is_tool_streaming_enabled() -> bool:
    """P2.4-alpha feature gate: tool streaming is disabled by default."""
    val = os.environ.get("AI_RUNTIME_ENABLE_TOOL_STREAMING", "")
    return val == "1" or val.lower() == "true"


@router.post("/runs")
async def create_run(request: Request) -> JSONResponse:
    """Create and execute a non-streaming AI run (ContextEnvelope → AiStructuredOutput)."""
    require_service_identity(request)

    try:
        body = await request.json()
    except JSON_EXCEPTIONS as exc:
        logger.warning("Failed to parse AI run request body", error_type=type(exc).__name__)
        return JSONResponse(
            {"success": False, "error": "Malformed JSON request body", "code": "MALFORMED_JSON"},
            status_code=400,
        )

    try:
        envelope = ContextEnvelope(**body)
    except Exception as exc:  # noqa: BLE001 - top-level route handler must catch all validation errors
        logger.warning("Invalid ContextEnvelope for AI run", error_type=type(exc).__name__)
        return JSONResponse(
            {
                "success": False,
                "error": "Invalid ContextEnvelope: required fields missing or malformed",
                "code": "INVALID_CONTEXT_ENVELOPE",
            },
            status_code=422,
        )

    service: RuntimeService = get_runtime_service()
    try:
        output = await service.execute_run(envelope)
    except Exception as exc:  # noqa: BLE001 - top-level route handler must catch all service failures
        logger.error("AI processing failed", run_id=envelope.run_id, error_type=type(exc).__name__)
        return JSONResponse(
            {
                "success": False,
                "error": "AI processing failed",
                "code": "AI_PROCESSING_FAILED",
                "run_id": envelope.run_id,
                "degraded": True,
            },
            status_code=500,
        )

    payload: dict[str, Any] = structured_output_to_response_dict(output)
    payload["success"] = output.status != STATUS_FAILED

    degraded = bool(
        output.limitations or (output.metrics and output.metrics.model and output.metrics.model.startswith("heuristic"))
    )
    if degraded:
        payload["degraded"] = True
    if output.status == STATUS_FAILED:
        payload["success"] = False
        payload["error"] = output.answer

    return JSONResponse(payload, status_code=200)


def _sse_format(event: str, data: dict[str, Any]) -> str:
    return f"event: {event}\ndata: {orjson.dumps(data).decode()}\n\n"


async def _stream_run_generator(service: RuntimeService, envelope: ContextEnvelope):
    async for evt in service.stream_run(envelope):
        yield _sse_format(evt["event"], evt["data"])


@router.post("/runs/stream")
async def stream_run(request: Request) -> StreamingResponse:
    """Streaming AI run: SSE event stream with progress/token/run.complete/run.fail."""
    require_service_identity(request)

    try:
        body = await request.json()
    except JSON_EXCEPTIONS as exc:
        logger.warning("Failed to parse streaming AI run request body", error_type=type(exc).__name__)
        error_data: dict[str, Any] = {
            "contract_version": "ai-structured-output.v1",
            "run_id": "",
            "status": "failed",
            "answer": "Malformed JSON request body",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": [],
            "metrics": None,
            "token_usage": None,
        }
        return StreamingResponse(
            iter([_sse_format("run.fail", error_data)]),
            media_type="text/event-stream",
            headers={"Cache-Control": "no-cache"},
        )

    try:
        envelope = ContextEnvelope(**body)
    except Exception as exc:  # noqa: BLE001 - top-level route handler must catch all validation errors
        logger.warning("Invalid ContextEnvelope for streaming AI run", error_type=type(exc).__name__)
        error_data = {
            "contract_version": "ai-structured-output.v1",
            "run_id": body.get("run_id", ""),
            "status": "failed",
            "answer": "Invalid ContextEnvelope: required fields missing or malformed",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": [],
            "metrics": None,
            "token_usage": None,
        }
        return StreamingResponse(
            iter([_sse_format("run.fail", error_data)]),
            media_type="text/event-stream",
            headers={"Cache-Control": "no-cache"},
        )

    service: RuntimeService = get_runtime_service()
    return StreamingResponse(
        _stream_run_generator(service, envelope),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache"},
    )


async def _stream_run_with_tools_generator(service: RuntimeService, envelope: ContextEnvelope):
    async for evt in service.stream_run_with_tools(envelope):
        yield _sse_format(evt["event"], evt["data"])


@router.post("/runs/stream-with-tools")
async def stream_run_with_tools(request: Request) -> StreamingResponse:
    """Streaming AI run with full tool execution loop.

    Supports multi-turn tool calls: LLM → tool.call → tool.result → LLM continues.
    Read-only tools execute locally; write actions generate OutputProposals.

    P2.4-alpha: disabled by default. Set AI_RUNTIME_ENABLE_TOOL_STREAMING=1 to enable.
    """
    require_service_identity(request)

    if not _is_tool_streaming_enabled():
        error_data: dict[str, Any] = {
            "contract_version": "ai-structured-output.v1",
            "run_id": "",
            "status": "failed",
            "answer": "Tool streaming is an alpha feature (P2.4-alpha) and is disabled by default. Set AI_RUNTIME_ENABLE_TOOL_STREAMING=1 to enable.",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": ["feature_disabled:AI_RUNTIME_ENABLE_TOOL_STREAMING"],
            "metrics": None,
            "token_usage": None,
        }
        return StreamingResponse(
            iter([_sse_format("run.fail", error_data)]),
            media_type="text/event-stream",
            headers={"Cache-Control": "no-cache"},
        )

    try:
        body = await request.json()
    except JSON_EXCEPTIONS as exc:
        logger.warning("Failed to parse tool-streaming AI run request body", error_type=type(exc).__name__)
        error_data: dict[str, Any] = {
            "contract_version": "ai-structured-output.v1",
            "run_id": "",
            "status": "failed",
            "answer": "Malformed JSON request body",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": [],
            "metrics": None,
            "token_usage": None,
        }
        return StreamingResponse(
            iter([_sse_format("run.fail", error_data)]),
            media_type="text/event-stream",
            headers={"Cache-Control": "no-cache"},
        )

    try:
        envelope = ContextEnvelope(**body)
    except Exception as exc:  # noqa: BLE001 - top-level route handler must catch all validation errors
        logger.warning("Invalid ContextEnvelope for tool-streaming AI run", error_type=type(exc).__name__)
        error_data = {
            "contract_version": "ai-structured-output.v1",
            "run_id": body.get("run_id", ""),
            "status": "failed",
            "answer": "Invalid ContextEnvelope: required fields missing or malformed",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": [],
            "limitations": [],
            "metrics": None,
            "token_usage": None,
        }
        return StreamingResponse(
            iter([_sse_format("run.fail", error_data)]),
            media_type="text/event-stream",
            headers={"Cache-Control": "no-cache"},
        )

    service: RuntimeService = get_runtime_service()
    return StreamingResponse(
        _stream_run_with_tools_generator(service, envelope),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache"},
    )
