"""RuntimeService — executes AI runs from ContextEnvelope to AiStructuredOutput.

Layering:
- HTTP contract lives in api_routes.py (auth, JSON, status codes).
- This module owns validation, intent routing, optional LLM, and structured output.
- Does not write business tables or mutate Rust job/run state.
"""

from __future__ import annotations

import base64
import logging
import os
import time
from typing import Any

from src.infrastructure.ai.audio_client import AudioClient
from src.infrastructure.ai.context_envelope import ContextEnvelope
from src.infrastructure.ai.openai_client import (
    Message,
    MultimodalContent,
    build_audio_content_part,
)
from src.infrastructure.ai.runtime_graph import RuntimeGraphRunner
from src.infrastructure.ai.runtime_llm import (
    LlmClient,
    StreamingLlmClient,
)
from src.infrastructure.ai.tools.read_only_tools import READ_ONLY_TOOL_SCHEMAS
from src.infrastructure.ai.tools.tool_executor import ToolExecutor
from src.infrastructure.common.exceptions import LLM_EXCEPTIONS

from ._context_cache import _ContextCacheMixin
from ._resolve import _ResolveMixin
from ._streaming import _StreamingMixin
from ._streaming_tools import _StreamingToolsMixin
from .helpers import (
    _attachment_field,
    _attachment_size_bytes,
    _audio_format_from_mime,
    _infer_input_modalities,
    _iter_envelope_attachments,
    _mime_to_modality,
    _sse_event,
    structured_output_to_response_dict,
)
from .models import _CapabilityPreparation

logger = logging.getLogger(__name__)


def _is_production_environment() -> bool:
    """Return True when the runtime is in a production-like environment.

    Mirrors the env-key lookup used by PostgresConfigStore so the runtime
    service fails closed consistently with the rest of the AI sidecar.
    """
    for key in ("APP_ENV", "APP_ENVIRONMENT", "ENVIRONMENT", "FLIGHT_ENV"):
        value = str(os.environ.get(key, "")).strip().lower()
        if value and value in {"production", "prod", "staging", "stage"}:
            return True
    return False


class RuntimeService(
    _StreamingToolsMixin,
    _StreamingMixin,
    _ResolveMixin,
    _ContextCacheMixin,
):
    """Executes a single non-streaming AI run from a ContextEnvelope."""

    def __init__(
        self,
        llm_client: LlmClient | None = None,
        streaming_llm_client: StreamingLlmClient | None = None,
        graph_runner: RuntimeGraphRunner | None = None,
        tool_executor: ToolExecutor | None = None,
        capability_resolver: Any | None = None,
        config_store: Any | None = None,
        mcp_client_manager: Any | None = None,
        mcp_repo: Any | None = None,
        skill_instruction_composer: Any | None = None,
        subagent_dispatcher: Any | None = None,
        cache_manager: Any | None = None,
        context_budget_planner: Any | None = None,
        audio_client_factory: Any | None = None,
        mq_gate: Any | None = None,
    ) -> None:
        self._llm_override = llm_client
        self._streaming_override = streaming_llm_client
        # ASR/TTS 旁路客户端工厂。provider 凭据是 per-entity 的，故按 resolved_config
        # 在每次调用时构造（而非 DI 单例）；工厂可注入以便测试覆盖。
        self._audio_client_factory = audio_client_factory or AudioClient
        self._graph_runner = graph_runner or RuntimeGraphRunner()
        self._skill_instruction_composer = skill_instruction_composer
        self._subagent_dispatcher = subagent_dispatcher
        self._cache_manager = cache_manager
        self._context_budget_planner = context_budget_planner
        # Prefer an explicit gate; otherwise resolve from the
        # AI container (registered by the MQ composition root); otherwise
        # fall back to None (no MQ publishes).
        resolved_mq_gate = mq_gate
        if resolved_mq_gate is None and tool_executor is None:
            try:
                from src.infrastructure.ai.ai_container import get_ai_container

                resolved_mq_gate = get_ai_container().resolve("tool_mq_gate", None)
            except Exception:  # noqa: BLE001 - container is best-effort
                resolved_mq_gate = None
        if tool_executor is not None:
            self._tool_executor = tool_executor
        else:
            self._tool_executor = ToolExecutor(
                mcp_client_manager=mcp_client_manager,
                mcp_repo=mcp_repo,
                subagent_dispatcher=subagent_dispatcher,
                cache_manager=cache_manager,
                mq_gate=resolved_mq_gate,
            )
        self._capability_resolver = capability_resolver
        self._config_store = config_store
        self._mcp_client_manager = mcp_client_manager

    async def _prepare_capabilities(
        self,
        envelope: ContextEnvelope,
        run_id: str,
        started: float,
        *,
        read_context_cache: bool,
    ) -> _CapabilityPreparation:
        """Shared run preamble: resolve entity capabilities, enforce the attachment
        security gate, and (optionally) read the context cache.

        This is the single source of truth used by ``execute_run`` (non-streaming),
        ``stream_run`` (streaming, no tools), and ``stream_run_with_tools`` (alpha,
        tool loop) so the three entrypoints cannot drift apart.

        Fail-closed: a resolution error or attachment violation produces a rejection
        (no fallback with defaults, which would bypass the entity's security config).
        When no capability resolver is wired the preparation is a no-op
        (``resolved_config=None``), preserving the legacy env-only behavior.
        """
        prep = _CapabilityPreparation()
        if not self._capability_resolver:
            if _is_production_environment():
                prep.rejection_answer = (
                    "AI_CAPABILITY_RESOLVER_MISSING: production environment requires "
                    "a capability resolver; run rejected."
                )
                output = self._failed_output(
                    run_id=run_id,
                    answer=prep.rejection_answer,
                    duration_ms=self._elapsed_ms(started),
                )
                prep.rejection_event = _sse_event("run.fail", structured_output_to_response_dict(output))
            return prep

        try:
            inferred_modalities = _infer_input_modalities(envelope)
            entity_id = getattr(envelope, "entity_id", None) or "default"
            resolved_config = await self._capability_resolver.resolve(
                entity_id=entity_id,
                model_purpose="chat",
                input_modalities=inferred_modalities,
            )
            prep.resolved_config = resolved_config
            prep.progress_events.append(
                _sse_event(
                    "progress",
                    {
                        "run_id": run_id,
                        "step": "capability.resolved",
                        "summary": (
                            f"Resolved config for entity={entity_id}, "
                            f"model={resolved_config.model_id}, "
                            f"tools={len(resolved_config.tools)}, "
                            f"hash={resolved_config.snapshot_hash}"
                        ),
                    },
                )
            )

            # P0a input gate: enforce security MIME/size and model modality on every
            # attachment BEFORE any LLM call. Raises ValueError → rejection below.
            self._validate_attachments(envelope, resolved_config)

            # Context cache read (records hit/miss for observability). Only the tool
            # path reuses the cached transcript as prior history.
            if read_context_cache and self._context_cache_enabled(resolved_config):
                conversation_id = self._context_conversation_id(envelope, run_id)
                cached_context = await self._read_context_cache(resolved_config, entity_id, conversation_id)
                prep.progress_events.append(
                    _sse_event(
                        "progress",
                        {
                            "run_id": run_id,
                            "step": "context.cache_lookup",
                            "summary": ("context cache hit" if cached_context is not None else "context cache miss"),
                            "hit": cached_context is not None,
                        },
                    )
                )
                # On a hit, reuse the cached transcript as prior history. Drop any
                # stored system turn — the system prompt is rebuilt per run.
                if cached_context and cached_context.get("messages"):
                    prep.cached_prior_messages = [
                        Message.from_dict(m)
                        for m in cached_context["messages"]
                        if isinstance(m, dict) and m.get("role") != "system"
                    ]
        except ValueError as exc:
            # Modality not supported or other attachment validation error.
            logger.error("capability_resolution_value_error", exc_info=exc)
            exc_text = str(exc)
            if "AI_INPUT_MIME_NOT_ALLOWED" in exc_text:
                rejection_answer = "AI_INPUT_MIME_NOT_ALLOWED"
            else:
                rejection_answer = "请求包含不支持的附件或模态"
            prep.progress_events.append(
                _sse_event(
                    "progress",
                    {
                        "run_id": run_id,
                        "step": "capability.rejected",
                        "summary": "attachment validation failed",
                    },
                )
            )
            prep.rejection_answer = rejection_answer
            output = self._failed_output(
                run_id=run_id,
                answer=rejection_answer,
                duration_ms=self._elapsed_ms(started),
            )
            prep.rejection_event = _sse_event("run.fail", structured_output_to_response_dict(output))
        except Exception as exc:
            # Fail closed: do not fall back with defaults, which bypasses entity security.
            logger.error("capability_resolution_failed", exc_info=exc)
            prep.progress_events.append(
                _sse_event(
                    "progress",
                    {
                        "run_id": run_id,
                        "step": "capability.rejected",
                        "summary": "capability resolution failed",
                    },
                )
            )
            prep.rejection_answer = "AI_CAPABILITY_RESOLUTION_FAILED"
            output = self._failed_output(
                run_id=run_id,
                answer="AI_CAPABILITY_RESOLUTION_FAILED",
                duration_ms=self._elapsed_ms(started),
            )
            prep.rejection_event = _sse_event("run.fail", structured_output_to_response_dict(output))

        return prep

    def _validate_attachments(
        self,
        envelope: ContextEnvelope,
        resolved_config: Any,
    ) -> None:
        """Input gate enforced before any LLM call (P0a).

        For every attachment carried by ``envelope`` (context attachments/files/media
        plus top-level ``envelope.attachments``) this enforces the entity's
        ``security`` policy — previously a dead config — and the selected model's
        declared input modalities:

        1. MIME ∈ ``security.allowed_input_mime_types`` (when configured), else
           ``AI_INPUT_MIME_NOT_ALLOWED: <mime>``.
        2. Byte size ≤ ``security.max_input_bytes`` (when configured), else
           ``AI_INPUT_TOO_LARGE``.
        3. Inferred modality ∈ the selected model's ``input_modalities`` — delegated
           to the resolver's :meth:`_validate_modalities` style via the shared
           ``AI_MODALITY_NOT_SUPPORTED`` contract.

        Raises ``ValueError`` (handled by the caller's ``capability.rejected`` /
        ``run.fail`` branch). No-ops when there are no attachments or no resolved
        config so text-only runs are unaffected.
        """
        if resolved_config is None:
            return

        security = getattr(resolved_config, "security", None) or {}
        allowed_mimes = security.get("allowed_input_mime_types")
        max_input_bytes = security.get("max_input_bytes")

        model = getattr(resolved_config, "model", None)
        model_modalities = list(getattr(model, "input_modalities", []) or [])

        for att in _iter_envelope_attachments(envelope):
            mime = _attachment_field(att, "mime_type", "mimeType", "content_type") or ""

            # 1) MIME allowlist (only enforced when the policy declares one).
            if allowed_mimes and mime not in allowed_mimes:
                raise ValueError(f"AI_INPUT_MIME_NOT_ALLOWED: {mime}")

            # 2) Size ceiling (only enforced when the policy declares one).
            if max_input_bytes is not None and _attachment_size_bytes(att) > int(max_input_bytes):
                raise ValueError("AI_INPUT_TOO_LARGE")

            # 3) Inferred modality must be accepted by the selected model. Mirrors
            #    CapabilityResolver._validate_modalities' AI_MODALITY_NOT_SUPPORTED.
            modality = _mime_to_modality(mime)
            if modality and model_modalities and modality not in model_modalities:
                raise ValueError(
                    f"AI_MODALITY_NOT_SUPPORTED: model "
                    f"{getattr(resolved_config, 'model_id', '')} does not support "
                    f"['{modality}']. Configured input modalities: {model_modalities}"
                )

    async def _build_user_content(
        self,
        envelope: ContextEnvelope,
        resolved_config: Any,
        base_text: str,
    ) -> MultimodalContent:
        """Assemble the current user turn, feeding media to a multimodal LLM (P1a).

        Image attachments always become ``image_url`` content blocks. Audio is
        fed directly as ``input_audio`` blocks when the selected model declares the
        ``audio`` input modality; otherwise it falls back to the ASR route — the
        clip is transcribed via :class:`AudioClient` and the recognized text is
        appended to the prompt. Returns a plain string when there is no media so the
        text-only path is byte-for-byte unchanged.
        """
        if resolved_config is None:
            return base_text

        model = getattr(resolved_config, "model", None)
        model_modalities = list(getattr(model, "input_modalities", []) or [])
        audio_direct = "audio" in model_modalities

        content_parts: list[dict[str, Any]] = []
        transcripts: list[str] = []

        for att in _iter_envelope_attachments(envelope):
            mime = _attachment_field(att, "mime_type", "mimeType", "content_type") or ""
            modality = _mime_to_modality(mime)
            data_b64 = _attachment_field(att, "data", "data_b64", "base64")
            if not isinstance(data_b64, str) or not data_b64:
                continue
            payload_b64 = data_b64.split(",", 1)[-1].strip()

            if modality == "image":
                # Reuse the existing image_url data-URI convention.
                url = data_b64 if data_b64.startswith("data:") else f"data:{mime};base64,{payload_b64}"
                content_parts.append({"type": "image_url", "image_url": {"url": url}})
            elif modality == "audio":
                if audio_direct:
                    fmt = _audio_format_from_mime(mime)
                    content_parts.append(build_audio_content_part(payload_b64, fmt))
                else:
                    text = await self._transcribe_attachment(resolved_config, payload_b64, mime)
                    if text:
                        transcripts.append(text)

        # No media converted: keep the simple string contract.
        if not content_parts and not transcripts:
            return base_text

        text_segment = base_text
        if transcripts:
            joined = "\n".join(transcripts)
            text_segment = f"{base_text}\n\n[Audio transcription]\n{joined}".strip()

        parts: list[dict[str, Any]] = []
        if text_segment:
            parts.append({"type": "text", "text": text_segment})
        parts.extend(content_parts)
        return parts

    async def _transcribe_attachment(
        self,
        resolved_config: Any,
        payload_b64: str,
        mime: str,
    ) -> str:
        """Transcribe a single audio attachment via the ASR route (P1a fallback).

        Used when the selected model cannot take audio directly. Best-effort: a
        decode/transport failure logs and yields an empty string rather than
        aborting the run (the input gate has already validated the attachment).
        """
        try:
            audio_bytes = base64.b64decode(payload_b64)
        except (ValueError, TypeError) as exc:
            logger.warning("Audio transcription skipped (decode failed): %s", exc)
            return ""

        # base_url 总有默认值；api_key 在 entity 未携带凭据时回退到与主 LLM
        # (OpenAiLlmClient) 同源的 OPENAI_API_KEY，保证同一部署下 ASR-fallback 可用。
        base_url = (getattr(resolved_config, "base_url", "") or "").strip()
        api_key = (getattr(resolved_config, "api_key", "") or "").strip() or os.getenv("OPENAI_API_KEY", "").strip()
        asr_model = self._resolve_asr_model(resolved_config)
        if not base_url or not asr_model:
            logger.warning("Audio transcription skipped: ASR base_url/model unavailable")
            return ""

        try:
            client = self._audio_client_factory(
                base_url=base_url,
                api_key=api_key,
                timeout=getattr(resolved_config, "timeout", 30.0),
            )
            return await client.transcribe(
                model=asr_model,
                audio_bytes=audio_bytes,
                mime=mime or "audio/wav",
            )
        except LLM_EXCEPTIONS as exc:
            logger.warning("Audio transcription failed: %s", exc)
            return ""

    @staticmethod
    def _resolve_asr_model(resolved_config: Any) -> str:
        """Pick the ASR model id from the resolved config, defaulting to whisper-1."""
        model = getattr(resolved_config, "model", None)
        caps = getattr(model, "capabilities", None) or {}
        return caps.get("asr_model") or "whisper-1"

    @staticmethod
    def _elapsed_ms(started: float) -> int:
        return max(1, int((time.monotonic() - started) * 1000))


def _build_default_capability_resolver() -> Any | None:
    """Build a default CapabilityResolver with available repositories."""
    try:
        from src.infrastructure.ai.ai_container import (
            resolve_mcp_repo,
            resolve_model_catalog_repo,
            resolve_skill_repo,
        )
        from src.infrastructure.ai.capability_resolver import CapabilityResolver

        # Try to get config store from the global container or build one
        config_store = None
        try:
            from src.infrastructure.ai.ai_container import get_ai_container

            container = get_ai_container()
            config_store = container.resolve("config_store", None)
        except Exception as e:  # noqa: BLE001 - bootstrap must catch all init failures
            logger.debug("Failed to resolve config_store from AI container: %s", e)

        mcp_repo = resolve_mcp_repo()
        skill_repo = resolve_skill_repo()
        model_catalog_repo = resolve_model_catalog_repo()

        return CapabilityResolver(
            config_store=config_store,
            model_catalog_repo=model_catalog_repo,
            mcp_repo=mcp_repo,
            skill_repo=skill_repo,
            builtin_tools=list(READ_ONLY_TOOL_SCHEMAS),
        )
    except Exception as exc:  # noqa: BLE001 - bootstrap must catch all init failures
        logger.debug("Failed to build default CapabilityResolver: %s", exc)
        return None


def get_runtime_service() -> RuntimeService:
    # The singleton lives on the package module so external callers (tests and
    # ai_runtime_bootstrap) can reset it via ``runtime_service._default_runtime_service = None``.
    import src.infrastructure.ai.runtime_service as _pkg

    if _pkg._default_runtime_service is None:
        # Prefer a fully-wired resolver registered by the production DI bootstrap;
        # fall back to building one from whatever repos are available in the container.
        try:
            from src.infrastructure.ai.ai_container import resolve_capability_resolver

            resolver = resolve_capability_resolver() or _build_default_capability_resolver()
        except Exception as exc:  # noqa: BLE001 - bootstrap must catch all init failures
            logger.debug("CapabilityResolver resolution failed, using fallback: %s", exc)
            resolver = _build_default_capability_resolver()
        mcp_client_manager = None
        mcp_repo = None
        skill_instruction_composer = None
        subagent_dispatcher = None
        cache_manager = None
        context_budget_planner = None
        try:
            from src.infrastructure.ai.ai_container import (
                resolve_cache_manager,
                resolve_context_budget_planner,
                resolve_mcp_client_manager,
                resolve_mcp_repo,
                resolve_skill_instruction_composer,
            )

            mcp_client_manager = resolve_mcp_client_manager()
            mcp_repo = resolve_mcp_repo()
            skill_instruction_composer = resolve_skill_instruction_composer()
            cache_manager = resolve_cache_manager()
            context_budget_planner = resolve_context_budget_planner()
        except Exception as e:  # noqa: BLE001 - bootstrap must catch all init failures
            logger.warning("Failed to resolve runtime service dependencies: %s", e)

        # Build subagent dispatcher (lazy, factory-based)
        try:
            from src.infrastructure.ai.subagents.dispatcher import SubagentDispatcher

            # Use a list to allow the closure to reference the dispatcher after creation
            _dispatcher_ref: list = [None]

            def _runtime_service_factory(target_entity_id: str) -> RuntimeService | None:
                """Create a RuntimeService for a sub-entity, with dispatcher for recursion."""
                sub_resolver = _build_default_capability_resolver()
                return RuntimeService(
                    capability_resolver=sub_resolver,
                    mcp_client_manager=mcp_client_manager,
                    mcp_repo=mcp_repo,
                    skill_instruction_composer=skill_instruction_composer,
                    subagent_dispatcher=_dispatcher_ref[0],
                    cache_manager=cache_manager,
                    context_budget_planner=context_budget_planner,
                )

            subagent_dispatcher = SubagentDispatcher(
                runtime_service_factory=_runtime_service_factory,
                capability_resolver=resolver,
            )
            _dispatcher_ref[0] = subagent_dispatcher
        except Exception as e:  # noqa: BLE001 - bootstrap must catch all init failures
            logger.warning("Failed to build SubagentDispatcher: %s", e)

        _pkg._default_runtime_service = RuntimeService(
            capability_resolver=resolver,
            mcp_client_manager=mcp_client_manager,
            mcp_repo=mcp_repo,
            skill_instruction_composer=skill_instruction_composer,
            subagent_dispatcher=subagent_dispatcher,
            cache_manager=cache_manager,
            context_budget_planner=context_budget_planner,
        )
    return _pkg._default_runtime_service
