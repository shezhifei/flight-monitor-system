"""Natural-language query orchestration service — NLQueryService implementation."""

from __future__ import annotations

import asyncio
import json
import re
import time
from collections.abc import Mapping
from typing import Any, Protocol

from src.application.services.ai.feature_flags import is_ai_feature_enabled
from src.domain.ports.notification_port import NotificationPort
from src.domain.utils.time_utils import utc_now
from src.infrastructure.ai.ai_entity import AIEntity, AIEntityConfig
from src.infrastructure.ai.config_store import AIConfigStoreInterface
from src.infrastructure.ai.conversation_manager import Conversation, ConversationManager, ConversationNotFoundError
from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
from src.infrastructure.ai.openai_client import (
    Message,
    MessageRole,
)
from src.infrastructure.ai.prompt_cache import generate_prompt_cache_key
from src.infrastructure.ai.prompts import NL_QUERY_SYSTEM_PROMPT
from src.infrastructure.ai.responses_adapter import (
    convert_tools_for_responses as _convert_tools_fn,
)
from src.infrastructure.ai.responses_adapter import (
    extract_message_content as _extract_message_content_fn,
)
from src.infrastructure.ai.responses_adapter import (
    extract_tool_calls as _extract_tool_calls_fn,
)
from src.infrastructure.ai.responses_adapter import (
    message_content_to_text as _message_content_to_text_fn,
)
from src.infrastructure.ai.responses_adapter import (
    messages_to_responses_input as _messages_to_responses_input_fn,
)
from src.infrastructure.ai.responses_adapter import (
    normalize_api_format as _normalize_api_format_fn,
)
from src.infrastructure.ai.tools.base import (
    InvocationMode,
    ToolCategory,
    ToolExecutionResult,
    ToolExecutionStatus,
)
from src.infrastructure.ai.tools.registry import ToolRegistry
from src.infrastructure.common.exceptions import POSTGRES_EXCEPTIONS
from src.infrastructure.logging.core import get_logger
from src.shared.id_generator import generate_id

from .models import (
    INSIGHT_TOOL_NAMES,
    LEGACY_QUERY_TOOL_NAMES,
    SQL_READ_TOOL_NAME,
    NLQueryResult,
)

logger = get_logger("src.application.services.ai.nl_query_service")


class NLQueryService:
    """Conversation + AI + tool execution orchestrator."""

    RUNTIME_EXECUTION_CACHE_LIMIT = 500
    TEXT_DELTA_CHUNK_SIZE = 64
    DEFAULT_CONVERSATION_MAX_TOKENS = 1200
    TOOL_EVENT_ARGUMENT_MAX_CHARS = 4000
    TOOL_EVENT_RESULT_MAX_CHARS = 6000

    CONTEXT_BUDGET_TOTAL = 12000
    CONTEXT_BUDGET_SYSTEM = 2200
    CONTEXT_BUDGET_RECENT = 7200
    CONTEXT_BUDGET_MEMORY = 1600
    CONTEXT_BUDGET_RUNTIME = 1000
    CONTEXT_BUDGET_PAGE = 1200
    CONTEXT_BUDGET_EVIDENCE = 1000

    class FlightQueryService(Protocol):
        async def get_all_flights(self, limit: int = 300, offset: int = 0) -> list[object]: ...

    def __init__(
        self,
        conversation_manager: ConversationManager,
        tool_registry: ToolRegistry,
        ai_config_store: AIConfigStoreInterface | None,
        flight_service: FlightQueryService,
        notification_port: NotificationPort | None = None,
        feature_overrides: Mapping[str, Any] | None = None,
        db_pool: Any | None = None,
    ):
        self._conversation_manager = conversation_manager
        self._tool_registry = tool_registry
        self._ai_config_store = ai_config_store
        self._flight_service = flight_service
        self._notification_port = notification_port
        self._feature_overrides = dict(feature_overrides or {})
        self._conversation_memory_cache: dict[str, list[dict[str, Any]]] = {}
        self._user_profile_cache: dict[str, dict[str, Any]] = {}
        self._runtime_execution_cache: dict[str, dict[str, Any]] = {}
        self._runtime_execution_started_at: dict[str, str] = {}
        self._db_pool: Any | None = db_pool
        self._db_pool_checked = False

    def _is_feature_enabled(self, flag_name: str, default: bool = True) -> bool:
        return is_ai_feature_enabled(
            flag_name,
            default=default,
            overrides=self._feature_overrides,
        )

    def _store_runtime_execution(self, execution_id: str | None, payload: dict[str, Any]) -> None:
        normalized_id = str(execution_id or "").strip()
        if not normalized_id:
            return
        try:
            progress_pct = int(payload.get("progress_pct") or 0)
        except (TypeError, ValueError):
            progress_pct = 0
        snapshot = {
            "execution_id": normalized_id,
            "conversation_id": payload.get("conversation_id"),
            "status": payload.get("status"),
            "phase": payload.get("phase"),
            "event": payload.get("event"),
            "code": payload.get("code"),
            "message": payload.get("message"),
            "progress_pct": progress_pct,
            "timestamps": payload.get("timestamps"),
            "error": payload.get("error"),
            "meta": payload.get("meta"),
            "execution_started_at": payload.get("execution_started_at"),
            "updated_at": utc_now().isoformat(),
            "source": "nl_query_runtime",
        }
        self._runtime_execution_cache[normalized_id] = snapshot
        while len(self._runtime_execution_cache) > self.RUNTIME_EXECUTION_CACHE_LIMIT:
            oldest_key = next(iter(self._runtime_execution_cache))
            self._runtime_execution_cache.pop(oldest_key, None)

    def get_runtime_execution(self, execution_id: str) -> dict[str, Any] | None:
        normalized_id = str(execution_id or "").strip()
        if not normalized_id:
            return None
        cached = self._runtime_execution_cache.get(normalized_id)
        if not cached:
            return None
        return dict(cached)

    async def query(
        self,
        question: str,
        user_id: str,
        user_roles: list[str],
        conversation_id: str | None = None,
        request_id: str | None = None,
        context: dict[str, object] | None = None,
        user_permissions: list[str] | None = None,
    ) -> NLQueryResult:
        started_at = time.perf_counter()
        normalized_question = (question or "").strip()
        if not normalized_question:
            raise ValueError("question cannot be empty")
        normalized_context = self._normalize_context(context)
        scene = self._resolve_scene(normalized_context)
        effective_request_id = self._normalize_request_id(request_id)
        self._runtime_execution_started_at[effective_request_id] = utc_now().isoformat()

        conversation = await self._get_or_create_conversation(
            conversation_id=conversation_id,
            user_id=user_id,
        )
        memory_profile_enabled = self._is_feature_enabled("AI_MEMORY_PROFILE_V1", default=True)
        user_profile = (
            await self._get_user_profile(user_id=user_id, user_roles=user_roles, context=normalized_context)
            if memory_profile_enabled
            else {}
        )
        memory_summary = await self._get_memory_summary(conversation.id) if memory_profile_enabled else ""
        prompt_message = self._build_user_message(
            normalized_question,
            normalized_context,
            memory_summary=memory_summary,
            user_profile=user_profile,
        )

        await self._emit_runtime_event(
            event_type="progress",
            scene=scene,
            request_id=effective_request_id,
            user_id=user_id,
            conversation_id=conversation.id,
            payload={
                "stage": "start",
                "message": "开始处理查询请求",
            },
        )

        try:
            await self._conversation_manager.add_message(
                conversation_id=conversation.id,
                role=MessageRole.USER,
                content=prompt_message,
            )

            tools = self._tool_registry.get_tools(
                categories=[ToolCategory.QUERY, ToolCategory.ANOMALY],
                user_id=user_id,
                user_roles=user_roles,
            )
            tools = self._prefer_sql_read_tools(tools)
            supports_sql_readonly = SQL_READ_TOOL_NAME in self._tool_name_set(tools)
            query_unified_enabled = self._is_feature_enabled("AI_QUERY_UNIFIED_V1", default=True)
            if supports_sql_readonly:
                supports_unified_query = False
            else:
                if query_unified_enabled:
                    tools = self._prefer_unified_query_tools(tools)
                else:
                    tools = self._remove_query_tools_by_name(tools, names={"QUERY"})
                supports_unified_query = query_unified_enabled and ("QUERY" in self._tool_name_set(tools))
            tools = self._filter_tools_by_permissions(tools, user_permissions=user_permissions)
            allow_insight_tools = self._can_use_insight_tools(user_permissions)

            ai_entity = await self._build_query_entity()
            if ai_entity is None:
                await self._emit_runtime_event(
                    event_type="progress",
                    scene=scene,
                    request_id=effective_request_id,
                    user_id=user_id,
                    conversation_id=conversation.id,
                    payload={
                        "stage": "ai_request",
                        "message": "AI 不可用，切换工具回退路径",
                    },
                )
                result = await self._fallback_query(
                    question=normalized_question,
                    user_id=user_id,
                    user_roles=user_roles,
                    conversation_id=conversation.id,
                    started_at=started_at,
                    context=normalized_context,
                    allow_insight_tools=allow_insight_tools,
                    supports_unified_query=supports_unified_query,
                    supports_sql_readonly=supports_sql_readonly,
                    scene=scene,
                    request_id=effective_request_id,
                )
                await self._emit_runtime_event(
                    event_type="done",
                    scene=scene,
                    request_id=effective_request_id,
                    user_id=user_id,
                    conversation_id=conversation.id,
                    payload={
                        "duration_ms": result.duration_ms,
                        "summary": result.summary,
                    },
                )
                return result

            model = ai_entity.config.default_model
            temperature = ai_entity.config.temperature
            max_tokens = ai_entity.config.max_tokens

            structured_data: list[dict[str, object]] = []
            await self._emit_runtime_event(
                event_type="progress",
                scene=scene,
                request_id=effective_request_id,
                user_id=user_id,
                conversation_id=conversation.id,
                payload={
                    "stage": "ai_request",
                    "message": "正在请求 AI 模型",
                },
            )
            # ---- Prompt Cache key ----
            cache_key: str | None = None
            cache_retention: str | None = None
            if self._is_feature_enabled("AI_PROMPT_CACHE_V1", default=False):
                api_format = self._normalize_api_format(getattr(ai_entity.config, "api_format", "chat_completions"))
                cache_key = generate_prompt_cache_key(
                    namespace="nl_query",
                    surface="query",
                    entity_id=str(ai_entity.config.default_model),
                    api_format=api_format,
                    model=model,
                    system_prompt=NL_QUERY_SYSTEM_PROMPT,
                    tools_schemas=tools,
                )
                cache_retention = getattr(ai_entity.config, "prompt_cache_retention", None)

            response = await self._request_ai(
                conversation_id=conversation.id,
                ai_entity=ai_entity,
                model=model,
                temperature=temperature,
                max_tokens=max_tokens,
                tools=tools,
                prompt_cache_key=cache_key,
                prompt_cache_retention=cache_retention,
            )

            for _ in range(3):
                tool_calls = self._extract_tool_calls(response)
                if not tool_calls:
                    break
                tool_calls = self._inject_context_into_tool_calls(tool_calls, normalized_context)

                await self._conversation_manager.add_message(
                    conversation_id=conversation.id,
                    role=MessageRole.ASSISTANT,
                    content="",
                    tool_calls=tool_calls,
                )

                tool_results = await self._execute_tool_calls_with_events(
                    tool_calls=tool_calls,
                    user_id=user_id,
                    user_roles=user_roles,
                    invocation_mode=InvocationMode.USER_REQUESTED,
                    max_concurrent=4,
                    scene=scene,
                    request_id=effective_request_id,
                    conversation_id=conversation.id,
                )

                for result in tool_results:
                    content = result.to_message()["content"]
                    await self._conversation_manager.add_message(
                        conversation_id=conversation.id,
                        role=MessageRole.TOOL,
                        content=content,
                        tool_call_id=result.tool_call_id,
                    )
                    structured_data.append(
                        {
                            "tool_name": result.tool_name,
                            "status": result.status.value,
                            "result": result.result,
                            "error": result.error_message,
                        }
                    )

                await self._emit_runtime_event(
                    event_type="progress",
                    scene=scene,
                    request_id=effective_request_id,
                    user_id=user_id,
                    conversation_id=conversation.id,
                    payload={
                        "stage": "ai_request",
                        "message": "工具执行完成，继续请求 AI 汇总",
                    },
                )
                response = await self._request_ai(
                    conversation_id=conversation.id,
                    ai_entity=ai_entity,
                    model=model,
                    temperature=temperature,
                    max_tokens=max_tokens,
                    tools=tools,
                    prompt_cache_key=cache_key,
                    prompt_cache_retention=cache_retention,
                )

            await self._emit_runtime_event(
                event_type="progress",
                scene=scene,
                request_id=effective_request_id,
                user_id=user_id,
                conversation_id=conversation.id,
                payload={
                    "stage": "finalizing",
                    "message": "正在整理最终回答",
                },
            )

            # 从 sync 调用的 response 中提取最终文本，通过流式 delta 推送给前端
            summary_text = self._extract_message_content(response)
            await self._emit_summary_text_deltas(
                scene=scene,
                request_id=effective_request_id,
                user_id=user_id,
                conversation_id=conversation.id,
                summary=summary_text,
            )

            visualization_hint, cleaned_summary = self._parse_visualization_hint(summary_text)
            msg_metadata = {}
            if visualization_hint:
                msg_metadata["visualization_hint"] = visualization_hint
            if structured_data:
                msg_metadata["structured_data"] = structured_data
            await self._conversation_manager.add_message(
                conversation_id=conversation.id,
                role=MessageRole.ASSISTANT,
                content=cleaned_summary,
                metadata=msg_metadata or None,
            )
            if memory_profile_enabled:
                await self._append_conversation_memory(
                    conversation_id=conversation.id,
                    turn_summary=cleaned_summary,
                    tool_outcomes=structured_data,
                    constraints={
                        "question": normalized_question,
                        "context": normalized_context,
                    },
                )

            duration_ms = int((time.perf_counter() - started_at) * 1000)
            result = NLQueryResult(
                query=normalized_question,
                interpretation=self._build_interpretation(normalized_question),
                structured_data=self._flatten_structured_data(structured_data),
                visualization_hint=visualization_hint,
                summary=cleaned_summary,
                conversation_id=conversation.id,
                duration_ms=duration_ms,
            )
            await self._emit_runtime_event(
                event_type="done",
                scene=scene,
                request_id=effective_request_id,
                user_id=user_id,
                conversation_id=conversation.id,
                payload={
                    "duration_ms": duration_ms,
                    "summary": cleaned_summary,
                },
            )
            return result
        except Exception as exc:
            await self._emit_runtime_event(
                event_type="error",
                scene=scene,
                request_id=effective_request_id,
                user_id=user_id,
                conversation_id=conversation.id,
                payload={
                    "message": str(exc),
                },
            )
            raise
        finally:
            self._runtime_execution_started_at.pop(effective_request_id, None)

    async def get_suggestions(self, user_id: str) -> list[str]:
        flights = await self._flight_service.get_all_flights(limit=300, offset=0)
        flight_entities = [item.get_flight() if hasattr(item, "get_flight") else item for item in flights]

        delayed_exists = any(self._delay_over_threshold(flight, threshold=15.0) for flight in flight_entities)
        abnormal_exists = any(
            bool((getattr(flight, "anomaly_summary", None) or {}).get("has_open_anomaly", False))
            for flight in flight_entities
        )
        in_progress_exists = any(
            str(self._safe_value(getattr(flight, "status", None)) or "") in {"BOARDING", "登机"}
            for flight in flight_entities
        )

        suggestions = []
        if delayed_exists:
            suggestions.append("今天有哪些延误超过30分钟的航班？")
        if abnormal_exists:
            suggestions.append("今天异常航班有哪些？")
        if in_progress_exists:
            suggestions.append("当前正在登机的航班有多少？")

        if not suggestions:
            suggestions = [
                "今天航班状态分布如何？",
                "今天有哪些延误航班？",
                "今天过站时间统计如何？",
            ]
        return suggestions

    async def _request_ai(
        self,
        *,
        conversation_id: str,
        ai_entity: AIEntity,
        model: str,
        temperature: float,
        max_tokens: int,
        tools: list[dict[str, object]],
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
    ) -> object:
        context_budget_total = self._resolve_request_context_budget(ai_entity.config)
        context = await self._conversation_manager.get_conversation_context(
            conversation_id,
            max_tokens=context_budget_total,
        )
        if not context:
            context = [Message(role=MessageRole.SYSTEM, content=NL_QUERY_SYSTEM_PROMPT)]
        elif self._is_feature_enabled("AI_CONTEXT_BUDGET_V1", default=True):
            context = self._apply_context_budget(context, total_budget=context_budget_total)

        api_format = self._normalize_api_format(getattr(ai_entity.config, "api_format", "chat_completions"))
        runner = LLMStreamRunner(ai_entity._ai_client)
        if api_format == "responses":
            instructions, input_items = self._context_to_responses_input(
                context=context,
                fallback_instructions=getattr(ai_entity.config, "system_prompt", None) or NL_QUERY_SYSTEM_PROMPT,
            )
            result = await runner.run_responses(
                model=model,
                input=input_items,
                instructions=instructions,
                tools=self._convert_tools_for_responses(tools),
                tool_choice="auto" if tools else None,
                max_output_tokens=max_tokens,
                temperature=temperature,
                prompt_cache_key=prompt_cache_key,
                prompt_cache_retention=prompt_cache_retention,
            )
            return result.raw_response or result

        result = await runner.run_chat(
            messages=context,
            model=model,
            temperature=temperature,
            max_tokens=max_tokens,
            tools=tools,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
        )
        return result.raw_response or result

    async def _build_query_entity(self) -> AIEntity | None:
        config = await self._load_default_ai_config()

        if not config or not config.get("api_key"):
            return None

        entity_config = AIEntityConfig(
            api_key=config.get("api_key"),
            base_url=config.get("base_url", "https://api.openai.com/v1"),
            default_model=config.get("default_model", "gpt-4o-mini"),
            api_format=str(config.get("api_format", "chat_completions")),
            temperature=float(config.get("temperature", 0.2)),
            max_tokens=int(config.get("max_tokens", self.DEFAULT_CONVERSATION_MAX_TOKENS)),
            context_window=int(config.get("context_window", 128000)),
            timeout=float(config.get("timeout", 30.0)),
            max_retries=int(config.get("max_retries", 2)),
            retry_delay=float(config.get("retry_delay", 0.5)),
            system_prompt=NL_QUERY_SYSTEM_PROMPT,
            allowed_tool_categories=[ToolCategory.QUERY.value, ToolCategory.ANOMALY.value],
        )
        entity = AIEntity(config=entity_config, entity_id="nl_query")
        await entity._ensure_initialized()
        return entity

    async def _execute_tool_calls_with_events(
        self,
        *,
        tool_calls: list[dict[str, object]],
        user_id: str,
        user_roles: list[str],
        invocation_mode: InvocationMode,
        max_concurrent: int,
        scene: str,
        request_id: str,
        conversation_id: str,
    ) -> list[ToolExecutionResult]:
        if not tool_calls:
            return []

        if not hasattr(self._tool_registry, "execute_tool_call"):
            tool_args_by_id: dict[str, tuple[Any, bool]] = {}
            for tool_call in tool_calls:
                function_block = tool_call.get("function") if isinstance(tool_call, dict) else {}
                tool_name = str((function_block or {}).get("name") or "unknown_tool")
                tool_call_id = str(tool_call.get("id") or generate_id("toolcall"))
                raw_arguments = (function_block or {}).get("arguments", "{}")
                normalized_arguments = self._normalize_tool_payload_value(raw_arguments)
                tool_arguments, tool_arguments_truncated = self._truncate_tool_payload(
                    normalized_arguments,
                    max_chars=self.TOOL_EVENT_ARGUMENT_MAX_CHARS,
                )
                tool_args_by_id[tool_call_id] = (tool_arguments, tool_arguments_truncated)
                await self._emit_runtime_event(
                    event_type="tool_start",
                    scene=scene,
                    request_id=request_id,
                    user_id=user_id,
                    conversation_id=conversation_id,
                    payload={
                        "event": "tool_start",
                        "phase": "tool_execute",
                        "tool_name": tool_name,
                        "tool_call_id": tool_call_id,
                        "status": "in_progress",
                        "message": f"tool '{tool_name}' started",
                        "content": f"tool '{tool_name}' started",
                        "tool_arguments": tool_arguments,
                        "tool_arguments_truncated": tool_arguments_truncated,
                    },
                )

            results = await self._tool_registry.execute_tool_calls(
                tool_calls,
                user_id=user_id,
                user_roles=user_roles,
                invocation_mode=invocation_mode,
                max_concurrent=max_concurrent,
            )

            for result in results:
                tool_arguments, tool_arguments_truncated = tool_args_by_id.get(
                    result.tool_call_id,
                    (None, False),
                )
                tool_result, tool_result_truncated = self._truncate_tool_payload(
                    result.result,
                    max_chars=self.TOOL_EVENT_RESULT_MAX_CHARS,
                )
                if result.status == ToolExecutionStatus.PENDING_APPROVAL:
                    await self._emit_runtime_event(
                        event_type="approval_required",
                        scene=scene,
                        request_id=request_id,
                        user_id=user_id,
                        conversation_id=conversation_id,
                        payload={
                            "event": "approval_required",
                            "phase": "approval",
                            "tool_name": result.tool_name,
                            "tool_call_id": result.tool_call_id,
                            "status": result.status.value,
                            "code": result.code,
                            "message": result.message or result.error_message,
                            "content": result.message or result.error_message or "approval required",
                            "pending_action": result.result,
                            "duration_ms": result.execution_time_ms,
                            "tool_error": result.error_message,
                            "tool_result": tool_result,
                            "tool_result_truncated": tool_result_truncated,
                            "tool_arguments": tool_arguments,
                            "tool_arguments_truncated": tool_arguments_truncated,
                        },
                    )
                await self._emit_runtime_event(
                    event_type="tool_end",
                    scene=scene,
                    request_id=request_id,
                    user_id=user_id,
                    conversation_id=conversation_id,
                    payload={
                        "event": "tool_end",
                        "phase": "tool_execute",
                        "tool_name": result.tool_name,
                        "tool_call_id": result.tool_call_id,
                        "status": result.status.value,
                        "code": result.code,
                        "message": result.message,
                        "content": result.message or result.error_message or "",
                        "duration_ms": result.execution_time_ms,
                        "error": result.error_message,
                        "tool_error": result.error_message,
                        "tool_result": tool_result,
                        "tool_result_truncated": tool_result_truncated,
                        "tool_arguments": tool_arguments,
                        "tool_arguments_truncated": tool_arguments_truncated,
                    },
                )
            return results

        concurrency = max(1, min(int(max_concurrent), len(tool_calls), 16))
        semaphore = asyncio.Semaphore(concurrency)

        async def _run(index: int, tool_call: dict[str, object]) -> tuple[int, ToolExecutionResult]:
            function_block = tool_call.get("function") if isinstance(tool_call, dict) else {}
            function_payload = function_block if isinstance(function_block, dict) else {}
            tool_name = str(function_payload.get("name") or "unknown_tool")
            tool_call_id = str(tool_call.get("id") or generate_id("toolcall"))
            arguments = function_payload.get("arguments", "{}")
            normalized_arguments = self._normalize_tool_payload_value(arguments)
            tool_arguments, tool_arguments_truncated = self._truncate_tool_payload(
                normalized_arguments,
                max_chars=self.TOOL_EVENT_ARGUMENT_MAX_CHARS,
            )

            async with semaphore:
                started_at = time.perf_counter()
                await self._emit_runtime_event(
                    event_type="tool_start",
                    scene=scene,
                    request_id=request_id,
                    user_id=user_id,
                    conversation_id=conversation_id,
                    payload={
                        "event": "tool_start",
                        "phase": "tool_execute",
                        "tool_name": tool_name,
                        "tool_call_id": tool_call_id,
                        "status": "in_progress",
                        "message": f"tool '{tool_name}' started",
                        "content": f"tool '{tool_name}' started",
                        "tool_arguments": tool_arguments,
                        "tool_arguments_truncated": tool_arguments_truncated,
                    },
                )
                try:
                    result = await self._tool_registry.execute_tool_call(
                        tool_call_id,
                        tool_name,
                        arguments,
                        user_id=user_id,
                        user_roles=user_roles,
                        invocation_mode=invocation_mode,
                    )
                except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                    result = ToolExecutionResult(
                        tool_call_id=tool_call_id,
                        tool_name=tool_name,
                        status=ToolExecutionStatus.ERROR,
                        error_message=str(exc),
                    )
                if result.status == ToolExecutionStatus.PENDING_APPROVAL:
                    approval_duration = int((time.perf_counter() - started_at) * 1000)
                    tool_result, tool_result_truncated = self._truncate_tool_payload(
                        result.result,
                        max_chars=self.TOOL_EVENT_RESULT_MAX_CHARS,
                    )
                    await self._emit_runtime_event(
                        event_type="approval_required",
                        scene=scene,
                        request_id=request_id,
                        user_id=user_id,
                        conversation_id=conversation_id,
                        payload={
                            "event": "approval_required",
                            "phase": "approval",
                            "tool_name": result.tool_name,
                            "tool_call_id": result.tool_call_id,
                            "status": result.status.value,
                            "code": result.code,
                            "message": result.message or result.error_message,
                            "content": result.message or result.error_message or "approval required",
                            "pending_action": result.result,
                            "tool_arguments": tool_arguments,
                            "tool_arguments_truncated": tool_arguments_truncated,
                            "tool_result": tool_result,
                            "tool_result_truncated": tool_result_truncated,
                            "tool_error": result.error_message,
                            "duration_ms": approval_duration,
                        },
                    )
            duration_ms = int((time.perf_counter() - started_at) * 1000)
            result.execution_time_ms = duration_ms
            tool_result, tool_result_truncated = self._truncate_tool_payload(
                result.result,
                max_chars=self.TOOL_EVENT_RESULT_MAX_CHARS,
            )

            await self._emit_runtime_event(
                event_type="tool_end",
                scene=scene,
                request_id=request_id,
                user_id=user_id,
                conversation_id=conversation_id,
                payload={
                    "event": "tool_end",
                    "phase": "tool_execute",
                    "tool_name": result.tool_name,
                    "tool_call_id": result.tool_call_id,
                    "status": result.status.value,
                    "code": result.code,
                    "message": result.message,
                    "content": result.message or result.error_message or "",
                    "duration_ms": duration_ms,
                    "error": result.error_message,
                    "tool_error": result.error_message,
                    "tool_result": tool_result,
                    "tool_result_truncated": tool_result_truncated,
                    "tool_arguments": tool_arguments,
                    "tool_arguments_truncated": tool_arguments_truncated,
                },
            )
            return index, result

        indexed_results = await asyncio.gather(*(_run(index, tool_call) for index, tool_call in enumerate(tool_calls)))
        indexed_results.sort(key=lambda item: item[0])
        return [item[1] for item in indexed_results]

    @staticmethod
    def _normalize_request_id(request_id: str | None) -> str:
        normalized = str(request_id or "").strip()
        return normalized or generate_id("req")

    @staticmethod
    def _resolve_scene(context: dict[str, str]) -> str:
        source_page = str(context.get("source_page") or "").strip().lower()
        if source_page == "flight_monitor":
            return "flight_monitor"
        return "nl_query"

    @staticmethod
    def _normalize_tool_payload_value(value: Any) -> Any:
        if isinstance(value, str):
            text = value.strip()
            if not text:
                return ""
            try:
                return json.loads(text)
            except (json.JSONDecodeError, TypeError, ValueError):
                return value
        return value

    @classmethod
    def _truncate_tool_payload(
        cls,
        value: Any,
        *,
        max_chars: int,
    ) -> tuple[Any, bool]:
        if value is None:
            return None, False

        if isinstance(value, str):
            if len(value) <= max_chars:
                return value, False
            return f"{value[:max_chars]}...(truncated)", True

        try:
            serialized = json.dumps(value, ensure_ascii=False, default=str)
        except (json.JSONDecodeError, TypeError, ValueError):
            fallback = str(value)
            if len(fallback) <= max_chars:
                return fallback, False
            return f"{fallback[:max_chars]}...(truncated)", True

        if len(serialized) <= max_chars:
            return value, False
        return f"{serialized[:max_chars]}...(truncated)", True

    async def _emit_runtime_event(
        self,
        *,
        event_type: str,
        scene: str,
        request_id: str,
        user_id: str,
        conversation_id: str | None,
        payload: dict[str, object] | None = None,
    ) -> None:
        if self._notification_port is None:
            return
        if not self._is_feature_enabled("AI_GLOBAL_PUSH_V1", default=True):
            return

        now_iso = utc_now().isoformat()
        execution_started_at = self._runtime_execution_started_at.get(request_id) or now_iso
        event_payload: dict[str, object] = {
            "event": str(payload.get("event") if isinstance(payload, dict) else "" or event_type).strip().lower(),
            "scene": scene,
            "request_id": request_id,
            "user_id": user_id,
            "phase": "tool_execute",
            "status": "in_progress",
            "meta": {"contract_version": "2.0"},
            "timestamp": now_iso,
            "execution_started_at": execution_started_at,
        }
        if conversation_id:
            event_payload["conversation_id"] = conversation_id
        if payload:
            event_payload.update(payload)

        event_name = str(event_payload.get("event") or event_type or "progress").strip().lower()
        if not event_payload.get("status"):
            status_map = {
                "tool_start": "in_progress",
                "tool_progress": "in_progress",
                "tool_end": "success",
                "approval_required": "pending_approval",
                "approval_result": "success",
                "done": "success",
                "error": "error",
                "progress": "in_progress",
            }
            event_payload["status"] = status_map.get(event_name, "in_progress")
        if not event_payload.get("phase"):
            phase_map = {
                "tool_start": "tool_execute",
                "tool_progress": "tool_execute",
                "tool_end": "tool_execute",
                "approval_required": "approval",
                "approval_result": "approval",
                "done": "report",
                "error": "report",
                "progress": "planning",
            }
            event_payload["phase"] = phase_map.get(event_name, "planning")

        status_value = str(event_payload.get("status") or "in_progress").strip().lower()
        execution_id = str(event_payload.get("execution_id") or request_id or "").strip()
        conversation_value = str(event_payload.get("conversation_id") or conversation_id or execution_id or "").strip()
        message_value = str(event_payload.get("message") or event_payload.get("stage") or "").strip()
        try:
            progress_pct = int(event_payload.get("progress_pct") or 0)
        except (TypeError, ValueError):
            progress_pct = 0
        if progress_pct < 0:
            progress_pct = 0
        if progress_pct > 100:
            progress_pct = 100

        raw_timestamps = event_payload.get("timestamps") if isinstance(event_payload.get("timestamps"), dict) else {}
        started_at = (
            raw_timestamps.get("started_at")
            or event_payload.get("execution_started_at")
            or event_payload.get("started_at")
            or event_payload.get("timestamp")
            or now_iso
        )
        ended_at = raw_timestamps.get("ended_at") or event_payload.get("ended_at")
        if not ended_at and (
            event_name in {"tool_end", "approval_result", "execution_end", "done", "error"}
            or status_value in {"success", "error", "timeout", "not_found", "permission_denied", "validation_error"}
        ):
            ended_at = now_iso

        raw_error = event_payload.get("error")
        normalized_error: dict[str, Any] | None = None
        if isinstance(raw_error, dict):
            details = raw_error.get("details")
            if not isinstance(details, dict):
                details = {k: v for k, v in raw_error.items() if k not in {"type", "details"}}
            normalized_error = {
                "type": str(raw_error.get("type") or status_value or "error").strip().lower(),
                "details": details or {},
            }
        elif raw_error is not None and raw_error != "":
            normalized_error = {
                "type": status_value
                if status_value in {"validation_error", "permission_denied", "not_found", "timeout"}
                else "error",
                "details": {"message": str(raw_error)},
            }
        elif status_value in {"validation_error", "permission_denied", "not_found", "timeout", "error"}:
            details: dict[str, Any] = {}
            if message_value:
                details["message"] = message_value
            if event_payload.get("code"):
                details["code"] = event_payload.get("code")
            normalized_error = {
                "type": status_value if status_value != "error" else "error",
                "details": details,
            }

        if "meta" in event_payload and isinstance(event_payload["meta"], dict):
            event_payload["meta"] = {**event_payload["meta"], "contract_version": "2.0"}
        else:
            event_payload["meta"] = {"contract_version": "2.0"}

        event_payload.update(
            {
                "execution_id": execution_id or None,
                "conversation_id": conversation_value or None,
                "status": status_value,
                "message": message_value,
                "progress_pct": progress_pct,
                "recoverable": bool(
                    event_payload.get(
                        "recoverable",
                        status_value
                        in {
                            "pending_approval",
                            "validation_error",
                            "not_found",
                            "permission_denied",
                            "timeout",
                            "error",
                        },
                    )
                ),
                "retryable": bool(event_payload.get("retryable", status_value in {"timeout", "error"})),
                "timestamps": {
                    "started_at": str(started_at),
                    "ended_at": str(ended_at) if ended_at else None,
                },
                "error": normalized_error,
                "execution_started_at": str(event_payload.get("execution_started_at") or execution_started_at),
            }
        )
        self._store_runtime_execution(execution_id, event_payload)

        try:
            await self._notification_port.notify_ai_event(event_type, event_payload)
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.warning(f"failed to emit nl_query runtime event '{event_type}': {exc}")

    @classmethod
    def _chunk_text_for_streaming(cls, text: str) -> list[str]:
        raw = str(text or "")
        if not raw:
            return []
        chunk_size = max(16, int(cls.TEXT_DELTA_CHUNK_SIZE))
        return [raw[idx : idx + chunk_size] for idx in range(0, len(raw), chunk_size)]

    async def _emit_summary_text_deltas(
        self,
        *,
        scene: str,
        request_id: str,
        user_id: str,
        conversation_id: str,
        summary: str,
    ) -> None:
        if not summary:
            return
        accumulated_chars = 0
        for chunk in self._chunk_text_for_streaming(summary):
            if not chunk:
                continue
            accumulated_chars += len(chunk)
            await self._emit_runtime_event(
                event_type="text_delta",
                scene=scene,
                request_id=request_id,
                user_id=user_id,
                conversation_id=conversation_id,
                payload={
                    "event": "text_delta",
                    "phase": "report",
                    "status": "in_progress",
                    "delta": chunk,
                    "accumulated_chars": accumulated_chars,
                },
            )
            await asyncio.sleep(0)

    async def _load_default_ai_config(self) -> dict[str, Any]:
        if self._ai_config_store is None:
            return {}
        config = await self._ai_config_store.get("default")
        if config:
            return config
        all_configs = await self._ai_config_store.get_all()
        if not all_configs:
            return {}
        return next(iter(all_configs.values()))

    @staticmethod
    def _coerce_positive_int(value: Any, default: int = 0) -> int:
        try:
            parsed = int(value)
        except (TypeError, ValueError):
            return default
        return parsed if parsed > 0 else default

    def _resolve_conversation_token_limit(self, config: Mapping[str, Any]) -> int:
        context_window_limit = self._coerce_positive_int(config.get("context_window"), 0)
        if context_window_limit > 0:
            return context_window_limit

        configured_limit = self._coerce_positive_int(config.get("max_tokens"), 0)
        if configured_limit > 0:
            return configured_limit

        return self.DEFAULT_CONVERSATION_MAX_TOKENS

    def _resolve_request_context_budget(self, ai_config: Any) -> int:
        configured_window = self._coerce_positive_int(
            getattr(ai_config, "context_window", None),
            0,
        )
        if configured_window > 0:
            return configured_window
        return self.CONTEXT_BUDGET_TOTAL

    @staticmethod
    def _scaled_bucket_limits(total_budget: int) -> dict[str, int]:
        base_limits = {
            "page": NLQueryService.CONTEXT_BUDGET_PAGE,
            "memory": NLQueryService.CONTEXT_BUDGET_MEMORY,
            "evidence": NLQueryService.CONTEXT_BUDGET_EVIDENCE,
            "runtime": NLQueryService.CONTEXT_BUDGET_RUNTIME,
            "recent": NLQueryService.CONTEXT_BUDGET_RECENT,
        }
        base_total = sum(base_limits.values())
        if total_budget <= 0 or base_total <= 0:
            return dict(base_limits)

        remaining = total_budget
        items = list(base_limits.items())
        scaled: dict[str, int] = {}
        for index, (name, value) in enumerate(items):
            if index == len(items) - 1:
                scaled[name] = max(1, remaining)
                break
            share = int((total_budget * value) / base_total)
            share = max(1, share)
            scaled[name] = share
            remaining = max(1, remaining - share)
        return scaled

    async def _sync_conversation_context_limit(self, conversation: Conversation, max_tokens: int) -> None:
        if max_tokens <= 0:
            return

        if getattr(conversation, "max_tokens", 0) != max_tokens:
            conversation.max_tokens = max_tokens

        context_id = str(getattr(conversation, "context_id", "") or "").strip()
        if not context_id:
            return

        context_manager = getattr(self._conversation_manager, "_context_manager", None)
        if context_manager is None:
            return

        try:
            context = await context_manager.get_context(context_id)
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.debug("context_get_failed context_id=%s", context_id, exc_info=exc)
            return

        if getattr(context, "max_tokens", 0) == max_tokens:
            return

        context.max_tokens = max_tokens
        if isinstance(getattr(context, "metadata", None), dict):
            context.metadata["max_tokens"] = max_tokens

        try:
            await context_manager.save_context(context_id, context)
        except Exception as exc:  # noqa: BLE001 - best-effort side effect must not abort main flow
            logger.warning(
                "failed to sync context token limit for conversation %s: %s",
                conversation.id,
                exc,
            )

    async def _get_or_create_conversation(self, conversation_id: str | None, user_id: str) -> Conversation:
        config = await self._load_default_ai_config()
        context_token_limit = self._resolve_conversation_token_limit(config)

        if conversation_id:
            try:
                conversation = await self._conversation_manager.get_conversation(conversation_id)
                await self._sync_conversation_context_limit(conversation, context_token_limit)
                return conversation
            except ConversationNotFoundError:
                pass

        return await self._conversation_manager.create_conversation(
            title="自然语言查询",
            user_id=user_id,
            model="gpt-4o-mini",
            temperature=0.2,
            max_tokens=context_token_limit,
            system_prompt=NL_QUERY_SYSTEM_PROMPT,
            custom_data={"scene": "nl_query", "created_at": utc_now().isoformat()},
        )

    @staticmethod
    def _extract_tool_calls(response: object) -> list[dict[str, object]]:
        return _extract_tool_calls_fn(response)

    @staticmethod
    def _extract_message_content(response: object) -> str:
        return _extract_message_content_fn(response, fallback="未生成回答。")

    @staticmethod
    def _normalize_api_format(api_format: object) -> str:
        return _normalize_api_format_fn(api_format)

    @staticmethod
    def _message_content_to_text(content: object) -> str:
        return _message_content_to_text_fn(content)

    @classmethod
    def _context_to_responses_input(
        cls,
        *,
        context: list[Message],
        fallback_instructions: str,
    ) -> tuple[str, list[dict[str, object]]]:
        return _messages_to_responses_input_fn(
            messages=context,
            fallback_instructions=fallback_instructions,
        )

    @staticmethod
    def _convert_tools_for_responses(tools: list[dict[str, object]]) -> list[dict[str, object]]:
        return _convert_tools_fn(tools)

    def _apply_context_budget(self, context: list[Message], total_budget: int | None = None) -> list[Message]:
        if not context:
            return [Message(role=MessageRole.SYSTEM, content=NL_QUERY_SYSTEM_PROMPT)]

        effective_total_budget = self._coerce_positive_int(total_budget, self.CONTEXT_BUDGET_TOTAL)
        system_messages = [msg for msg in context if msg.role == MessageRole.SYSTEM]
        non_system = [msg for msg in context if msg.role != MessageRole.SYSTEM]

        chosen_system = (
            system_messages[:1]
            if system_messages
            else [Message(role=MessageRole.SYSTEM, content=NL_QUERY_SYSTEM_PROMPT)]
        )
        indexed_messages: list[tuple[int, Message, str, int]] = []
        for index, msg in enumerate(non_system):
            content = self._message_content_to_text(msg.content)
            char_count = max(1, len(content))
            bucket = self._classify_context_bucket(msg, content)
            indexed_messages.append((index, msg, bucket, char_count))

        bucket_limits = self._scaled_bucket_limits(effective_total_budget)

        selected_indexes: set[int] = set()
        for bucket_name, budget in bucket_limits.items():
            used_chars = 0
            for index, _msg, bucket, char_count in reversed(indexed_messages):
                if bucket != bucket_name:
                    continue
                if index in selected_indexes:
                    continue
                if used_chars + char_count > budget:
                    continue
                selected_indexes.add(index)
                used_chars += char_count

        if not selected_indexes:
            used_chars = 0
            for index, _msg, _bucket, char_count in reversed(indexed_messages):
                if used_chars + char_count > bucket_limits["recent"]:
                    continue
                selected_indexes.add(index)
                used_chars += char_count

        selected = [msg for index, msg in enumerate(non_system) if index in selected_indexes]
        total_chars = sum(max(1, len(self._message_content_to_text(msg.content))) for msg in selected)
        if total_chars > effective_total_budget:
            trimmed: list[Message] = []
            consumed = 0
            for msg in reversed(selected):
                size = max(1, len(self._message_content_to_text(msg.content)))
                if consumed + size > effective_total_budget:
                    continue
                trimmed.append(msg)
                consumed += size
            selected = list(reversed(trimmed))

        return chosen_system + selected

    @staticmethod
    def _classify_context_bucket(message: Message, content_text: str) -> str:
        lowered = str(content_text or "").lower()
        if message.role == MessageRole.TOOL:
            return "runtime"
        if "系统补充上下文" in lowered or "来源页面" in lowered or "scope_mode" in lowered:
            return "page"
        if "会话记忆摘要" in lowered or "用户画像" in lowered:
            return "memory"
        if "sources" in lowered or "evidence" in lowered or "chunk_id" in lowered or "retrieval_mode" in lowered:
            return "evidence"
        return "recent"

    @staticmethod
    def _parse_visualization_hint(text: str) -> tuple[str | None, str]:
        pattern = re.compile(r"\[VIS:(table|bar_chart|timeline)\]", re.IGNORECASE)
        match = pattern.search(text or "")
        if not match:
            return None, (text or "").strip()
        hint = match.group(1).lower()
        cleaned = pattern.sub("", text).strip()
        return hint, cleaned

    @staticmethod
    def _build_interpretation(question: str) -> str:
        return f"用户希望查询：{question.strip()}"

    @staticmethod
    def _flatten_structured_data(items: list[dict[str, object]]) -> object:
        success_payloads = [
            item.get("result") for item in items if item.get("status") == ToolExecutionStatus.SUCCESS.value
        ]
        if not success_payloads:
            return {"tool_calls": items}
        if len(success_payloads) == 1:
            return success_payloads[0]
        return success_payloads

    async def _fallback_query(
        self,
        *,
        question: str,
        user_id: str,
        user_roles: list[str],
        conversation_id: str,
        started_at: float,
        context: dict[str, str] | None,
        allow_insight_tools: bool,
        supports_unified_query: bool,
        supports_sql_readonly: bool,
        scene: str,
        request_id: str,
    ) -> NLQueryResult:
        tool_name, args, visualization = self._pick_fallback_tool(
            question,
            context=context,
            allow_insight_tools=allow_insight_tools,
            supports_unified_query=supports_unified_query,
            supports_sql_readonly=supports_sql_readonly,
        )
        tool_call = {
            "id": f"fallback_{int(time.time() * 1000)}",
            "function": {
                "name": tool_name,
                "arguments": json.dumps(args, ensure_ascii=False),
            },
        }
        results = await self._execute_tool_calls_with_events(
            tool_calls=[tool_call],
            user_id=user_id,
            user_roles=user_roles,
            invocation_mode=InvocationMode.USER_REQUESTED,
            max_concurrent=1,
            scene=scene,
            request_id=request_id,
            conversation_id=conversation_id,
        )
        first = results[0] if results else None

        if first and first.status == ToolExecutionStatus.SUCCESS:
            summary = self._build_fallback_summary(question, first.result)
            structured_data = first.result
        else:
            first_error = first.error_message if first else "tool execution failed"
            summary = first_error or "当前无法完成查询，请稍后重试。"
            structured_data = {"error": first_error}

        await self._emit_summary_text_deltas(
            scene=scene,
            request_id=request_id,
            user_id=user_id,
            conversation_id=conversation_id,
            summary=summary,
        )
        await self._conversation_manager.add_message(
            conversation_id=conversation_id,
            role=MessageRole.ASSISTANT,
            content=summary,
        )

        return NLQueryResult(
            query=question,
            interpretation=self._build_interpretation(question),
            structured_data=structured_data,
            visualization_hint=visualization,
            summary=summary,
            conversation_id=conversation_id,
            duration_ms=int((time.perf_counter() - started_at) * 1000),
        )

    def _pick_fallback_tool(
        self,
        question: str,
        *,
        context: dict[str, str] | None,
        allow_insight_tools: bool,
        supports_unified_query: bool,
        supports_sql_readonly: bool,
    ) -> tuple[str, dict[str, object], str | None]:
        text = question.strip()
        normalized_text = text.lower()
        normalized_context = self._normalize_context(context)
        selected_flight_id = normalized_context.get("selected_flight_id")
        selected_flight_no = normalized_context.get("selected_flight_no")
        extracted_flight_no = self._extract_flight_number_from_text(text)

        def _contains_any(*keywords: str) -> bool:
            for keyword in keywords:
                normalized_keyword = str(keyword or "").lower()
                if keyword in text or (normalized_keyword and normalized_keyword in normalized_text):
                    return True
            return False

        def _default_time_range() -> dict[str, str]:
            now = utc_now()
            start = now.replace(hour=0, minute=0, second=0, microsecond=0)
            end = now.replace(hour=23, minute=59, second=59, microsecond=0)
            return {
                "from": start.isoformat(),
                "to": end.isoformat(),
            }

        def _with_flight_args() -> dict[str, object]:
            payload: dict[str, object] = {"hours": 24}
            if selected_flight_id:
                payload["flight_id"] = selected_flight_id
            elif selected_flight_no:
                payload["flight_number"] = selected_flight_no
            elif extracted_flight_no:
                payload["flight_number"] = extracted_flight_no
            return payload

        if allow_insight_tools and ("事件经过" in text or ("经过" in text and "航班" in text)):
            return "generate_flight_event_journey", _with_flight_args(), "table"

        if allow_insight_tools and ("动态报表" in text or "历史报表" in text or ("报表" in text and "航班" in text)):
            args = _with_flight_args()
            incident_match = re.search(r"(延误|返航|备降|机械故障|旅客事件|其他)", text)
            if incident_match:
                args["incident_type"] = incident_match.group(1)
            return "generate_flight_history_report", args, "table"

        has_stats_signal = _contains_any(
            "统计",
            "概览",
            "总数",
            "趋势",
            "分布",
            "多少",
            "count",
            "summary",
            "aggregate",
            "对比",
        )
        has_timeseries_signal = _contains_any(
            "趋势",
            "时间趋势",
            "时间线",
            "时序",
            "按天",
            "按小时",
            "走势",
            "timeline",
            "timeseries",
        )
        timeseries_granularity: str | None = None
        if _contains_any("按小时", "每小时", "逐小时", "小时级", "hourly", "per hour"):
            timeseries_granularity = "hour"
        elif _contains_any("按天", "每天", "每日", "逐日", "按日", "daily", "per day"):
            timeseries_granularity = "day"

        if supports_sql_readonly:

            def _sql_literal(value: str) -> str:
                return "'" + str(value or "").replace("'", "''") + "'"

            def _sql_args(sql_text: str, *, max_rows: int = 200) -> tuple[str, dict[str, object]]:
                return SQL_READ_TOOL_NAME, {"sql": sql_text, "max_rows": max_rows}

            def _where_sql(clauses: list[str]) -> str:
                filtered = [clause for clause in clauses if str(clause or "").strip()]
                if not filtered:
                    return ""
                return "WHERE " + " AND ".join(filtered)

            default_time_range = _default_time_range()
            from_sql = _sql_literal(default_time_range["from"])
            to_sql = _sql_literal(default_time_range["to"])
            bucket = "hour" if timeseries_granularity == "hour" else "day"

            if _contains_any("运营概览", "运行概览", "运营态势", "运行态势", "全局态势", "ops", "overview"):
                if has_timeseries_signal:
                    sql = (
                        "WITH raw AS ("
                        f"SELECT date_trunc('{bucket}', created_at) AS bucket, COUNT(*)::int AS flights, 0::int AS alerts, 0::int AS tasks "
                        "FROM ai_query.v_flights "
                        f"WHERE created_at BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz "
                        "GROUP BY 1 "
                        "UNION ALL "
                        f"SELECT date_trunc('{bucket}', detected_at) AS bucket, 0::int AS flights, COUNT(*)::int AS alerts, 0::int AS tasks "
                        "FROM ai_query.v_anomalies "
                        f"WHERE detected_at BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz "
                        "GROUP BY 1 "
                        "UNION ALL "
                        f"SELECT date_trunc('{bucket}', created_at) AS bucket, 0::int AS flights, 0::int AS alerts, COUNT(*)::int AS tasks "
                        "FROM ai_query.v_todos "
                        f"WHERE created_at BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz "
                        "GROUP BY 1"
                        ") "
                        "SELECT bucket, "
                        "SUM(flights)::int AS flights, "
                        "SUM(alerts)::int AS alerts, "
                        "SUM(tasks)::int AS tasks, "
                        "(SUM(flights) + SUM(alerts) + SUM(tasks))::int AS total "
                        "FROM raw "
                        "GROUP BY bucket "
                        "ORDER BY bucket ASC"
                    )
                    tool_name, payload = _sql_args(sql)
                    return tool_name, payload, "timeline"

                tool_name, payload = _sql_args("SELECT * FROM ai_query.v_ops_overview", max_rows=20)
                return tool_name, payload, "bar_chart"

            if _contains_any("告警", "报警", "anomaly", "alert"):
                alert_filters: list[str] = []
                if selected_flight_id:
                    alert_filters.append(f"flight_id = {_sql_literal(selected_flight_id)}")
                if _contains_any("未处理", "open"):
                    alert_filters.append("status = 'open'")
                elif _contains_any("已确认", "acknowledged"):
                    alert_filters.append("status = 'acknowledged'")
                elif _contains_any("已解决", "resolved"):
                    alert_filters.append("status = 'resolved'")

                if _contains_any("机位冲突", "gate conflict"):
                    alert_filters.append("anomaly_type = 'gate_stand_conflict'")
                elif _contains_any("kpi", "指标劣化", "kpi劣化", "kpi 恶化"):
                    alert_filters.append("anomaly_type = 'kpi_degradation'")
                elif _contains_any("服务节点", "超时", "timeout"):
                    alert_filters.append("anomaly_type = 'service_node_timeout'")
                elif _contains_any("派工", "dispatch"):
                    alert_filters.append("anomaly_type = 'dispatch_issue'")
                elif _contains_any("ai风险", "ai 风险", "风险"):
                    alert_filters.append("anomaly_type = 'ai_risk'")

                if has_timeseries_signal:
                    where_clause = _where_sql(
                        [
                            f"detected_at BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz",
                            *alert_filters,
                        ]
                    )
                    sql = (
                        f"SELECT date_trunc('{bucket}', detected_at) AS bucket, COUNT(*)::int AS alert_count "
                        f"FROM ai_query.v_anomalies "
                        f"{where_clause} "
                        "GROUP BY bucket "
                        "ORDER BY bucket ASC"
                    )
                    tool_name, payload = _sql_args(sql)
                    return tool_name, payload, "timeline"

                if has_stats_signal:
                    where_clause = _where_sql(alert_filters)
                    sql = (
                        "SELECT status, anomaly_type, COUNT(*)::int AS alert_count "
                        "FROM ai_query.v_anomalies "
                        f"{where_clause} "
                        "GROUP BY status, anomaly_type "
                        "ORDER BY alert_count DESC, status ASC"
                    )
                    tool_name, payload = _sql_args(sql, max_rows=50)
                    return tool_name, payload, "bar_chart"

                where_clause = _where_sql(alert_filters)
                sql = (
                    "SELECT anomaly_id, flight_id, anomaly_type, severity, status, title, detected_at, resolved_at "
                    "FROM ai_query.v_anomalies "
                    f"{where_clause} "
                    "ORDER BY detected_at DESC"
                )
                tool_name, payload = _sql_args(sql, max_rows=100)
                return tool_name, payload, "table"

            if _contains_any("待办", "todo", "task", "任务"):
                task_filters = ["is_deleted = FALSE"]
                if _contains_any("进行中", "in progress"):
                    task_filters.append("status = '进行中'")
                elif _contains_any("已完成", "completed"):
                    task_filters.append("status = '已完成'")
                elif _contains_any("已取消", "cancelled"):
                    task_filters.append("status = '已取消'")
                elif _contains_any("阻塞", "blocked"):
                    task_filters.append("status = '阻塞中'")

                if _contains_any("逾期", "overdue"):
                    task_filters.append("due_date IS NOT NULL AND due_date < CURRENT_TIMESTAMP")
                if _contains_any("今天到期", "今日到期", "due today"):
                    task_filters.append(
                        "DATE(due_date AT TIME ZONE 'Asia/Shanghai') = DATE(CURRENT_TIMESTAMP AT TIME ZONE 'Asia/Shanghai')"
                    )
                if _contains_any("高优先级", "high priority"):
                    task_filters.append("priority IN ('关键', '高', '紧急')")

                if has_timeseries_signal:
                    where_clause = _where_sql(
                        [
                            f"COALESCE(due_date, created_at) BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz",
                            *task_filters,
                        ]
                    )
                    sql = (
                        f"SELECT date_trunc('{bucket}', COALESCE(due_date, created_at)) AS bucket, COUNT(*)::int AS todo_count "
                        f"FROM ai_query.v_todos "
                        f"{where_clause} "
                        "GROUP BY bucket "
                        "ORDER BY bucket ASC"
                    )
                    tool_name, payload = _sql_args(sql)
                    return tool_name, payload, "timeline"

                if has_stats_signal:
                    where_clause = _where_sql(task_filters)
                    sql = (
                        "SELECT status, COUNT(*)::int AS todo_count "
                        "FROM ai_query.v_todos "
                        f"{where_clause} "
                        "GROUP BY status "
                        "ORDER BY todo_count DESC"
                    )
                    tool_name, payload = _sql_args(sql, max_rows=50)
                    return tool_name, payload, "bar_chart"

                where_clause = _where_sql(task_filters)
                sql = (
                    "SELECT todo_id, title, status, priority, assigned_to, due_date, progress, created_at, updated_at "
                    "FROM ai_query.v_todos "
                    f"{where_clause} "
                    "ORDER BY COALESCE(due_date, created_at) ASC NULLS LAST"
                )
                tool_name, payload = _sql_args(sql, max_rows=100)
                return tool_name, payload, "table"

            delay_match = re.search(r"(\d+)\s*分钟", text)
            if "延误" in text:
                min_delay = int(delay_match.group(1)) if delay_match else 15
                sql = (
                    "SELECT flight_id, flight_number, airline_code, status, scheduled_departure, estimated_departure, "
                    "delay_minutes, stand, gate "
                    "FROM ai_query.v_flights "
                    f"WHERE COALESCE(delay_minutes, 0) >= {max(min_delay, 0)} "
                    "ORDER BY delay_minutes DESC NULLS LAST, scheduled_departure DESC"
                )
                tool_name, payload = _sql_args(sql, max_rows=100)
                return tool_name, payload, "table"

            if "异常" in text:
                sql = (
                    "SELECT flight_id, flight_number, airline_code, status, scheduled_departure, "
                    "has_open_anomaly, open_anomaly_count, delay_minutes, stand, gate "
                    "FROM ai_query.v_flights "
                    "WHERE has_open_anomaly = TRUE "
                    "ORDER BY scheduled_departure DESC"
                )
                tool_name, payload = _sql_args(sql, max_rows=100)
                return tool_name, payload, "table"

            if "趋势" in text or "分布" in text:
                if has_timeseries_signal:
                    sql = (
                        f"SELECT date_trunc('{bucket}', scheduled_departure) AS bucket, COUNT(*)::int AS flight_count "
                        "FROM ai_query.v_flights "
                        f"WHERE scheduled_departure BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz "
                        "GROUP BY bucket "
                        "ORDER BY bucket ASC"
                    )
                    tool_name, payload = _sql_args(sql)
                    return tool_name, payload, "timeline"

                sql = (
                    "SELECT status, COUNT(*)::int AS flight_count "
                    "FROM ai_query.v_flights "
                    "GROUP BY status "
                    "ORDER BY flight_count DESC"
                )
                tool_name, payload = _sql_args(sql, max_rows=50)
                return tool_name, payload, "bar_chart"

            if "时间" in text and "范围" in text:
                sql = (
                    "SELECT flight_id, flight_number, airline_code, status, scheduled_departure, estimated_departure, "
                    "stand, gate, delay_minutes "
                    "FROM ai_query.v_flights "
                    f"WHERE scheduled_departure BETWEEN {from_sql}::timestamptz AND {to_sql}::timestamptz "
                    "ORDER BY scheduled_departure ASC"
                )
                tool_name, payload = _sql_args(sql, max_rows=100)
                return tool_name, payload, "timeline"

            sql = (
                "SELECT status, COUNT(*)::int AS flight_count "
                "FROM ai_query.v_flights "
                "GROUP BY status "
                "ORDER BY flight_count DESC"
            )
            tool_name, payload = _sql_args(sql, max_rows=50)
            return tool_name, payload, "table"

        if supports_unified_query and _contains_any(
            "运营概览", "运行概览", "运营态势", "运行态势", "全局态势", "ops", "overview"
        ):
            intent = "timeseries" if has_timeseries_signal else "aggregate"
            payload: dict[str, object] = {
                "intent": intent,
                "dataset": "ops",
            }
            if intent == "timeseries":
                payload["time_range"] = _default_time_range()
                if timeseries_granularity:
                    payload["filters"] = {"granularity": timeseries_granularity}
            return "QUERY", payload, ("timeline" if intent == "timeseries" else "bar_chart")

        if supports_unified_query and _contains_any("告警", "报警", "anomaly", "alert"):
            alert_filters: dict[str, object] = {}
            if selected_flight_id:
                alert_filters["flight_id"] = selected_flight_id
            if _contains_any("未处理", "open"):
                alert_filters["status"] = "open"
            elif _contains_any("已确认", "acknowledged"):
                alert_filters["status"] = "acknowledged"
            elif _contains_any("已解决", "resolved"):
                alert_filters["status"] = "resolved"

            if _contains_any("机位冲突", "gate conflict"):
                alert_filters["anomaly_type"] = "gate_stand_conflict"
            elif _contains_any("kpi", "指标劣化", "kpi劣化", "kpi 恶化"):
                alert_filters["anomaly_type"] = "kpi_degradation"
            elif _contains_any("服务节点", "超时", "timeout"):
                alert_filters["anomaly_type"] = "service_node_timeout"
            elif _contains_any("派工", "dispatch"):
                alert_filters["anomaly_type"] = "dispatch_issue"
            elif _contains_any("ai风险", "ai 风险", "风险"):
                alert_filters["anomaly_type"] = "ai_risk"

            intent = "timeseries" if has_timeseries_signal else ("aggregate" if has_stats_signal else "search")
            payload: dict[str, object] = {
                "intent": intent,
                "dataset": "alerts",
            }
            if alert_filters and intent != "timeseries":
                payload["filters"] = alert_filters
            if intent in {"search", "timeseries"}:
                payload["limit"] = 100
            if intent == "timeseries":
                payload["time_range"] = _default_time_range()
                timeseries_filters = dict(alert_filters)
                if timeseries_granularity:
                    timeseries_filters["granularity"] = timeseries_granularity
                if timeseries_filters:
                    payload["filters"] = timeseries_filters
            visualization = (
                "timeline" if intent == "timeseries" else ("bar_chart" if intent == "aggregate" else "table")
            )
            return "QUERY", payload, visualization

        if supports_unified_query and _contains_any("待办", "todo", "task", "任务"):
            task_filters: dict[str, object] = {}
            if _contains_any("进行中", "in progress"):
                task_filters["status"] = "in_progress"
            elif _contains_any("已完成", "completed"):
                task_filters["status"] = "completed"
            elif _contains_any("已取消", "cancelled"):
                task_filters["status"] = "cancelled"
            elif _contains_any("阻塞", "blocked"):
                task_filters["status"] = "blocked"

            if _contains_any("逾期", "overdue"):
                task_filters["overdue_only"] = True
            if _contains_any("今天到期", "今日到期", "due today"):
                task_filters["due_today"] = True
            if _contains_any("高优先级", "high priority"):
                task_filters["high_priority_only"] = True

            intent = "timeseries" if has_timeseries_signal else ("aggregate" if has_stats_signal else "search")
            payload = {
                "intent": intent,
                "dataset": "tasks",
            }
            if task_filters and intent != "timeseries":
                payload["filters"] = task_filters
            if intent in {"search", "timeseries"}:
                payload["limit"] = 100
            if intent == "timeseries":
                payload["time_range"] = _default_time_range()
                timeseries_filters = dict(task_filters)
                if timeseries_granularity:
                    timeseries_filters["granularity"] = timeseries_granularity
                if timeseries_filters:
                    payload["filters"] = timeseries_filters
            visualization = (
                "timeline" if intent == "timeseries" else ("bar_chart" if intent == "aggregate" else "table")
            )
            return "QUERY", payload, visualization

        delay_match = re.search(r"(\d+)\s*分钟", text)
        if "延误" in text:
            min_delay = int(delay_match.group(1)) if delay_match else 15
            if supports_unified_query:
                return (
                    "QUERY",
                    {
                        "intent": "search",
                        "dataset": "flights",
                        "filters": {"min_delay_minutes": min_delay},
                        "limit": 100,
                    },
                    "table",
                )
            return (
                "get_delayed_flights",
                {
                    "min_delay_minutes": min_delay,
                    "limit": 100,
                },
                "table",
            )

        if "异常" in text:
            if supports_unified_query:
                return (
                    "QUERY",
                    {
                        "intent": "search",
                        "dataset": "flights",
                        "filters": {"has_open_anomaly": True},
                        "limit": 100,
                    },
                    "table",
                )
            return (
                "get_abnormal_flights",
                {
                    "limit": 100,
                },
                "table",
            )

        if "趋势" in text or "分布" in text:
            if supports_unified_query:
                return (
                    "QUERY",
                    {
                        "intent": "aggregate",
                        "dataset": "flights",
                        "group_by": ["status"],
                    },
                    "bar_chart",
                )
            return "count_flights_by_status", {}, "bar_chart"

        if "时间" in text and "范围" in text:
            now = utc_now()
            start = now.replace(hour=0, minute=0, second=0, microsecond=0)
            end = now.replace(hour=23, minute=59, second=59, microsecond=0)
            if supports_unified_query:
                return (
                    "QUERY",
                    {
                        "intent": "timeseries",
                        "dataset": "flights",
                        "time_range": {"from": start.isoformat(), "to": end.isoformat()},
                        "limit": 100,
                    },
                    "timeline",
                )
            return (
                "get_flights_by_time_range",
                {
                    "start_time": start.isoformat(),
                    "end_time": end.isoformat(),
                    "limit": 100,
                },
                "timeline",
            )

        if supports_unified_query:
            return (
                "QUERY",
                {
                    "intent": "aggregate",
                    "dataset": "flights",
                    "group_by": ["status"],
                },
                "table",
            )
        return "count_flights_by_status", {}, "table"

    @staticmethod
    def _build_fallback_summary(question: str, payload: object) -> str:
        if isinstance(payload, dict) and "total" in payload:
            total = payload.get("total")
            return f"已完成查询：{question}。共匹配 {total} 条记录。"
        if isinstance(payload, dict) and "row_count" in payload:
            row_count = payload.get("row_count")
            truncated = bool(payload.get("truncated"))
            if truncated:
                return f"已完成查询：{question}。当前返回 {row_count} 条记录（结果已截断）。"
            return f"已完成查询：{question}。共返回 {row_count} 条记录。"
        if isinstance(payload, dict) and payload.get("report_markdown"):
            return "已生成航班动态报表。"
        if isinstance(payload, dict) and payload.get("journey_markdown"):
            return "已生成航班事件经过。"
        return f"已完成查询：{question}。"

    @staticmethod
    def _delay_minutes(flight: object) -> float | None:
        estimated_departure = getattr(flight, "estimated_departure", None)
        scheduled_departure = getattr(flight, "scheduled_departure", None)
        if not estimated_departure or not scheduled_departure:
            return None
        return (estimated_departure - scheduled_departure).total_seconds() / 60

    @classmethod
    def _delay_over_threshold(cls, flight: object, threshold: float) -> bool:
        delay_minutes = cls._delay_minutes(flight)
        return delay_minutes is not None and delay_minutes >= threshold

    @staticmethod
    def _safe_value(value: object) -> object:
        return value.value if hasattr(value, "value") else value

    @staticmethod
    def _normalize_context(context: dict[str, object] | None) -> dict[str, str]:
        if not isinstance(context, dict):
            return {}

        normalized: dict[str, str] = {}
        for key in ("source_page", "selected_flight_id", "selected_flight_no", "scope_mode"):
            value = context.get(key)
            if value is None:
                continue
            text = str(value).strip()
            if text:
                normalized[key] = text

        scope_mode = normalized.get("scope_mode")
        if scope_mode not in {"selected_or_global", "global"}:
            normalized.pop("scope_mode", None)

        return normalized

    @staticmethod
    def _extract_flight_number_from_text(text: str) -> str | None:
        upper_text = str(text or "").upper()
        match = re.search(r"\b[A-Z0-9]{2,3}\d{2,4}\b", upper_text)
        return match.group(0) if match else None

    async def _get_user_profile(
        self,
        *,
        user_id: str,
        user_roles: list[str],
        context: dict[str, str],
    ) -> dict[str, Any]:
        cached = self._user_profile_cache.get(user_id)
        if cached:
            return cached

        fallback_profile = {
            "user_id": user_id,
            "role": user_roles[0] if user_roles else "unknown",
            "timezone": context.get("timezone") or "Asia/Shanghai",
            "preferences": {},
            "pinned_metrics": {},
        }
        profile = await self._load_user_profile_from_db(user_id)
        if profile is None:
            profile = dict(fallback_profile)
            await self._upsert_user_profile_to_db(profile)
        else:
            role_value = profile.get("role") or fallback_profile["role"]
            timezone_value = profile.get("timezone") or fallback_profile["timezone"]
            profile = {
                "user_id": user_id,
                "role": role_value,
                "timezone": timezone_value,
                "preferences": profile.get("preferences") or {},
                "pinned_metrics": profile.get("pinned_metrics") or {},
            }

        self._user_profile_cache[user_id] = profile
        return profile

    def _resolve_db_pool(self) -> Any | None:
        if self._db_pool_checked:
            return self._db_pool

        self._db_pool_checked = True
        return self._db_pool

    async def _load_user_profile_from_db(self, user_id: str) -> dict[str, Any] | None:
        db_pool = self._resolve_db_pool()
        if db_pool is None:
            return None

        query = """
            SELECT user_id, role, timezone, preferences, pinned_metrics
            FROM ai_user_profile
            WHERE user_id = %s
            LIMIT 1
        """
        try:
            async with db_pool.connection_context() as conn, conn.cursor() as cursor:
                await cursor.execute(query, (user_id,))
                row = await cursor.fetchone()
            if not row:
                return None
            row_data = dict(row)
            return {
                "user_id": str(row_data.get("user_id") or user_id),
                "role": row_data.get("role"),
                "timezone": row_data.get("timezone"),
                "preferences": self._safe_json_loads(row_data.get("preferences"), default={}),
                "pinned_metrics": self._safe_json_loads(row_data.get("pinned_metrics"), default={}),
            }
        except POSTGRES_EXCEPTIONS as exc:
            logger.debug(f"nl_query load user profile from db failed: {exc}")
            return None

    async def _upsert_user_profile_to_db(self, profile: dict[str, Any]) -> None:
        db_pool = self._resolve_db_pool()
        if db_pool is None:
            return

        query = """
            INSERT INTO ai_user_profile (user_id, role, timezone, preferences, pinned_metrics, updated_at)
            VALUES (%s, %s, %s, %s::jsonb, %s::jsonb, CURRENT_TIMESTAMP)
            ON CONFLICT (user_id)
            DO UPDATE SET
                role = EXCLUDED.role,
                timezone = EXCLUDED.timezone,
                updated_at = CURRENT_TIMESTAMP
        """
        try:
            async with db_pool.connection_context() as conn:
                await conn.execute(
                    query,
                    (
                        str(profile.get("user_id") or ""),
                        profile.get("role"),
                        profile.get("timezone"),
                        self._safe_json_dumps(profile.get("preferences") or {}),
                        self._safe_json_dumps(profile.get("pinned_metrics") or {}),
                    ),
                )
        except POSTGRES_EXCEPTIONS as exc:
            logger.debug(f"nl_query upsert user profile skipped: {exc}")

    async def _get_memory_summary(self, conversation_id: str) -> str:
        memories = self._conversation_memory_cache.get(conversation_id) or []
        if not memories:
            memories = await self._load_memory_records_from_db(conversation_id, limit=6)
            if memories:
                self._conversation_memory_cache[conversation_id] = memories[-50:]
        return self._build_memory_summary_from_records(memories)

    def _build_memory_summary(self, conversation_id: str) -> str:
        memories = self._conversation_memory_cache.get(conversation_id) or []
        return self._build_memory_summary_from_records(memories)

    @staticmethod
    def _build_memory_summary_from_records(memories: list[dict[str, Any]]) -> str:
        if not memories:
            return ""
        latest = memories[-3:]
        lines = []
        for item in latest:
            summary = str(item.get("summary") or "").strip()
            if summary:
                lines.append(f"- {summary[:180]}")
        return "\n".join(lines)

    async def _append_conversation_memory(
        self,
        *,
        conversation_id: str,
        turn_summary: str,
        tool_outcomes: list[dict[str, object]],
        constraints: dict[str, object],
    ) -> None:
        records = self._conversation_memory_cache.setdefault(conversation_id, [])
        memory_record = {
            "turn_no": len(records) + 1,
            "summary": str(turn_summary or "").strip()[:280],
            "entities": [],
            "tool_outcomes": tool_outcomes[:3] if isinstance(tool_outcomes, list) else [],
            "constraints": constraints if isinstance(constraints, dict) else {},
            "created_at": utc_now().isoformat(),
        }
        records.append(memory_record)
        if len(records) > 50:
            del records[0 : len(records) - 50]
        await self._save_memory_record_to_db(conversation_id, memory_record)

    async def _load_memory_records_from_db(self, conversation_id: str, limit: int = 6) -> list[dict[str, Any]]:
        db_pool = self._resolve_db_pool()
        if db_pool is None:
            return []

        query = """
            SELECT turn_no, summary, entities, constraints, tool_outcomes, created_at
            FROM ai_conversation_memory
            WHERE conversation_id = %s
            ORDER BY turn_no DESC
            LIMIT %s
        """
        safe_limit = max(1, min(int(limit or 1), 50))
        try:
            async with db_pool.connection_context() as conn, conn.cursor() as cursor:
                await cursor.execute(query, (conversation_id, safe_limit))
                rows = await cursor.fetchall()
            parsed: list[dict[str, Any]] = []
            for raw_row in reversed(rows or []):
                row = dict(raw_row)
                parsed.append(
                    {
                        "turn_no": int(row.get("turn_no") or 0),
                        "summary": str(row.get("summary") or "").strip(),
                        "entities": self._safe_json_loads(row.get("entities"), default=[]),
                        "constraints": self._safe_json_loads(row.get("constraints"), default={}),
                        "tool_outcomes": self._safe_json_loads(row.get("tool_outcomes"), default=[]),
                        "created_at": row.get("created_at").isoformat()
                        if hasattr(row.get("created_at"), "isoformat")
                        else str(row.get("created_at") or ""),
                    }
                )
            return parsed
        except POSTGRES_EXCEPTIONS as exc:
            logger.debug(f"nl_query load memory from db skipped: {exc}")
            return []

    async def _save_memory_record_to_db(self, conversation_id: str, record: dict[str, Any]) -> None:
        db_pool = self._resolve_db_pool()
        if db_pool is None:
            return

        next_turn_query = """
            SELECT COALESCE(MAX(turn_no), 0) + 1 AS next_turn
            FROM ai_conversation_memory
            WHERE conversation_id = %s
        """
        insert_query = """
            INSERT INTO ai_conversation_memory (
                conversation_id,
                turn_no,
                summary,
                entities,
                constraints,
                tool_outcomes,
                created_at
            )
            VALUES (%s, %s, %s, %s::jsonb, %s::jsonb, %s::jsonb, CURRENT_TIMESTAMP)
        """
        try:
            async with db_pool.connection_context() as conn:
                async with conn.cursor() as cursor:
                    await cursor.execute(next_turn_query, (conversation_id,))
                    row = await cursor.fetchone()
                next_turn = int((dict(row) if row else {}).get("next_turn") or record.get("turn_no") or 1)
                await conn.execute(
                    insert_query,
                    (
                        conversation_id,
                        next_turn,
                        str(record.get("summary") or "")[:280],
                        self._safe_json_dumps(record.get("entities") or []),
                        self._safe_json_dumps(record.get("constraints") or {}),
                        self._safe_json_dumps(record.get("tool_outcomes") or []),
                    ),
                )
        except POSTGRES_EXCEPTIONS as exc:
            logger.debug(f"nl_query save memory to db skipped: {exc}")

    @staticmethod
    def _safe_json_dumps(value: Any) -> str:
        try:
            return json.dumps(value, ensure_ascii=False, default=str)
        except (json.JSONDecodeError, TypeError, ValueError):
            return json.dumps({}, ensure_ascii=False)

    @staticmethod
    def _safe_json_loads(value: Any, *, default: Any) -> Any:
        if value is None:
            return default
        if isinstance(value, (dict, list)):
            return value
        try:
            return json.loads(value)
        except (json.JSONDecodeError, TypeError, ValueError):
            return default

    def _build_user_message(
        self,
        question: str,
        context: dict[str, str],
        *,
        memory_summary: str = "",
        user_profile: dict[str, Any] | None = None,
    ) -> str:
        if not context:
            context = {}

        lines: list[str] = []
        source_page = context.get("source_page")
        selected_flight_id = context.get("selected_flight_id")
        selected_flight_no = context.get("selected_flight_no")
        scope_mode = context.get("scope_mode", "selected_or_global")

        if source_page:
            lines.append(f"- 来源页面: {source_page}")
        if selected_flight_id:
            lines.append(f"- 当前选中航班ID: {selected_flight_id}")
        if selected_flight_no:
            lines.append(f"- 当前选中航班号: {selected_flight_no}")
        if scope_mode == "selected_or_global":
            lines.append("- 查询范围规则: 优先当前选中航班，无选中时再使用全局查询")
        elif scope_mode == "global":
            lines.append("- 查询范围规则: 仅全局查询")

        if user_profile:
            lines.append(f"- 用户画像: role={user_profile.get('role')}, timezone={user_profile.get('timezone')}")
        if memory_summary:
            lines.append("- 会话记忆摘要:")
            lines.append(memory_summary)

        if not lines:
            return question

        return f"{question}\n\n系统补充上下文（用于工具调用与范围判定，不要逐字复述给用户）：\n" + "\n".join(lines)

    @staticmethod
    def _can_use_insight_tools(user_permissions: list[str] | None) -> bool:
        permission_set = {str(item).strip() for item in (user_permissions or []) if str(item).strip()}
        if "*" in permission_set:
            return True
        return "ai:execute" in permission_set and "flight:read" in permission_set

    def _filter_tools_by_permissions(
        self,
        tools: list[dict[str, Any]],
        *,
        user_permissions: list[str] | None,
    ) -> list[dict[str, Any]]:
        if self._can_use_insight_tools(user_permissions):
            return tools

        filtered_tools: list[dict[str, Any]] = []
        for tool in tools:
            function_block = tool.get("function", {}) if isinstance(tool, dict) else {}
            name = str(function_block.get("name") or "").strip()
            if not name or name in INSIGHT_TOOL_NAMES:
                continue
            filtered_tools.append(tool)
        return filtered_tools

    @staticmethod
    def _prefer_sql_read_tools(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
        if not isinstance(tools, list):
            return []

        has_sql_read_tool = False
        for tool in tools:
            function_block = tool.get("function", {}) if isinstance(tool, dict) else {}
            name = str(function_block.get("name") or "").strip()
            if name == SQL_READ_TOOL_NAME:
                has_sql_read_tool = True
                break

        if not has_sql_read_tool:
            return tools

        filtered: list[dict[str, Any]] = []
        for tool in tools:
            function_block = tool.get("function", {}) if isinstance(tool, dict) else {}
            name = str(function_block.get("name") or "").strip()
            if not name:
                continue
            if name in LEGACY_QUERY_TOOL_NAMES:
                continue
            filtered.append(tool)
        return filtered

    @staticmethod
    def _prefer_unified_query_tools(tools: list[dict[str, Any]]) -> list[dict[str, Any]]:
        if not isinstance(tools, list):
            return []

        has_unified_query = False
        filtered: list[dict[str, Any]] = []
        for tool in tools:
            function_block = tool.get("function", {}) if isinstance(tool, dict) else {}
            name = str(function_block.get("name") or "").strip()
            if not name:
                continue
            if name == "QUERY":
                has_unified_query = True
                filtered.append(tool)
                continue
            if name in LEGACY_QUERY_TOOL_NAMES:
                continue
            filtered.append(tool)

        if has_unified_query:
            return filtered
        return tools

    @staticmethod
    def _remove_query_tools_by_name(tools: list[dict[str, Any]], names: set[str]) -> list[dict[str, Any]]:
        if not isinstance(tools, list):
            return []
        blocked = {str(name or "").strip() for name in names if str(name or "").strip()}
        if not blocked:
            return list(tools)

        filtered: list[dict[str, Any]] = []
        for tool in tools:
            function_block = tool.get("function", {}) if isinstance(tool, dict) else {}
            tool_name = str(function_block.get("name") or "").strip()
            if tool_name and tool_name in blocked:
                continue
            filtered.append(tool)
        return filtered

    @staticmethod
    def _tool_name_set(tools: list[dict[str, Any]]) -> set[str]:
        if not isinstance(tools, list):
            return set()

        names: set[str] = set()
        for tool in tools:
            if not isinstance(tool, dict):
                continue
            function_block = tool.get("function", {})
            if not isinstance(function_block, dict):
                continue
            name = str(function_block.get("name") or "").strip()
            if name:
                names.add(name)
        return names

    @staticmethod
    def _inject_context_into_tool_calls(
        tool_calls: list[dict[str, object]],
        context: dict[str, str],
    ) -> list[dict[str, object]]:
        selected_flight_id = context.get("selected_flight_id")
        selected_flight_no = context.get("selected_flight_no")

        if not selected_flight_id and not selected_flight_no:
            return tool_calls

        patched_calls: list[dict[str, object]] = []
        for call in tool_calls:
            function_payload = call.get("function") if isinstance(call, dict) else None
            if not isinstance(function_payload, dict):
                patched_calls.append(call)
                continue

            tool_name = str(function_payload.get("name") or "").strip()
            raw_arguments = function_payload.get("arguments", "{}")
            if tool_name not in INSIGHT_TOOL_NAMES:
                patched_calls.append(call)
                continue

            try:
                parsed_arguments = (
                    json.loads(raw_arguments) if isinstance(raw_arguments, str) else dict(raw_arguments or {})
                )
            except (json.JSONDecodeError, TypeError, ValueError):
                parsed_arguments = {}

            if not parsed_arguments.get("flight_id") and selected_flight_id:
                parsed_arguments["flight_id"] = selected_flight_id
            if (
                not parsed_arguments.get("flight_id")
                and not parsed_arguments.get("flight_number")
                and selected_flight_no
            ):
                parsed_arguments["flight_number"] = selected_flight_no

            patched_function = dict(function_payload)
            patched_function["arguments"] = json.dumps(parsed_arguments, ensure_ascii=False)
            patched_call = dict(call)
            patched_call["function"] = patched_function
            patched_calls.append(patched_call)

        return patched_calls
