"""AI Runtime service — orchestrates ContextEnvelope → AiStructuredOutput.

Facade package: re-exports the public API split across submodules so that
``from src.infrastructure.ai.runtime_service import X`` continues to work
unchanged.

Submodules:
- ``models``: dataclasses (``_CapabilityPreparation``, ``_RunContext``).
- ``helpers``: validation, prompt/evidence/reasoning builders, modality
  inference, attachment handling, and SSE event sanitization.
- ``service``: the ``RuntimeService`` class plus the ``get_runtime_service``
  singleton accessor and ``_build_default_capability_resolver`` factory.

Layering:
- HTTP contract lives in api_routes.py (auth, JSON, status codes).
- This package owns validation, intent routing, optional LLM, and structured output.
- Does not write business tables or mutate Rust job/run state.
"""

from __future__ import annotations

from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
from src.infrastructure.ai.runtime_llm import (
    FakeLlmClient,
    LlmClient,
    LlmCompletion,
    LlmStreamError,
    LlmUnavailableError,
    OpenAiLlmClient,
    OpenAiStreamingLlmClient,
    StreamingLlmClient,
    sanitize_provider_error,
)

from ._constants import CONTRACT_VERSION, STATUS_FAILED, STATUS_SUCCEEDED

# Re-export helpers (private + public).
from .helpers import (
    _AUDIO_MIME_TO_FORMAT,
    _INTENT_LABELS,
    _MIME_TO_MODALITY,
    _MODALITY_ORDER,
    _attachment_field,
    _attachment_size_bytes,
    _audio_format_from_mime,
    _extract_note_content,
    _infer_input_modalities,
    _iter_answer_chunks,
    _iter_envelope_attachments,
    _mime_to_modality,
    _sanitize_tool_call_event,
    _sanitize_tool_result_event,
    _sse_event,
    build_evidence,
    build_proposals_from_envelope,
    build_reasoning_steps,
    build_system_prompt,
    heuristic_answer,
    structured_output_to_response_dict,
    validate_envelope,
)

# Re-export dataclasses.
from .models import _CapabilityPreparation, _RunContext

# Re-export service-level symbols.
from .service import (
    RuntimeService,
    _build_default_capability_resolver,
    get_runtime_service,
)

# ---------------------------------------------------------------------------
# Package-level singleton. Lives here (not in ``service``) so external callers
# (tests and ai_runtime_bootstrap) can reset it via
# ``runtime_service._default_runtime_service = None`` and ``get_runtime_service``
# observes the reset. ``get_runtime_service`` reads/writes this attribute via a
# lazy import of the package module.
# ---------------------------------------------------------------------------
_default_runtime_service: RuntimeService | None = None

__all__ = [
    "CONTRACT_VERSION",
    "STATUS_FAILED",
    "STATUS_SUCCEEDED",
    "FakeLlmClient",
    "LLMStreamRunner",
    "LlmClient",
    "LlmCompletion",
    "LlmStreamError",
    "LlmUnavailableError",
    "OpenAiLlmClient",
    "OpenAiStreamingLlmClient",
    "RuntimeService",
    "StreamingLlmClient",
    "build_evidence",
    "build_proposals_from_envelope",
    "build_reasoning_steps",
    "build_system_prompt",
    "get_runtime_service",
    "heuristic_answer",
    "sanitize_provider_error",
    "structured_output_to_response_dict",
    "validate_envelope",
]
