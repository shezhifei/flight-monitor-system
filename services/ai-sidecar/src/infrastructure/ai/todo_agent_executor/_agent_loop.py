"""TODO Agent executor — main class."""

from __future__ import annotations

import asyncio
import json
import logging
import time
from typing import Any

from src.domain.ai.agent_execution import (
    AgentExecution,
    AgentStep,
    AgentStepType,
    TokenUsage,
    ToolCallRecord,
)
from src.infrastructure.ai.ai_entity import AIEntity
from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
from src.infrastructure.ai.monitoring.metrics import (
    record_error,
    record_latency,
    record_tokens,
    record_tool_usage,
)
from src.infrastructure.ai.openai_client import Message, MessageRole
from src.infrastructure.ai.todo_agent_executor.models import AgentLoopContext
from src.infrastructure.ai.tools.base import InvocationMode

logger = logging.getLogger(__name__)


class _AgentLoopMixin:
    """Mixin for TodoAgentExecutor."""

    async def _execute_agent_loop(
        self,
        ai_entity: AIEntity,
        execution: AgentExecution,
        task_description: str,
        tools: list[dict],
        max_iterations: int,
        system_prompt: str | None,
        child_todos: list[str],
        ai_timeout_seconds: float,
        tool_timeout_seconds: float,
        user_id: str | None,
        user_roles: list[str] | None,
        invocation_mode: InvocationMode,
    ) -> tuple:
        """执行 Agent 工具调用循环 (使用自适应策略)"""
        messages = []
        if system_prompt:
            messages.append(Message(role=MessageRole.SYSTEM, content=system_prompt))
        messages.append(Message(role=MessageRole.USER, content=task_description))

        current_response = None
        step_sequence = 1

        context = AgentLoopContext(execution_id=execution.run_id, start_time=time.time())

        retry_count = 0
        api_format = self._normalize_api_format(getattr(ai_entity.config, "api_format", "chat_completions"))

        # 初始化工具调用熔断器
        try:
            from src.infrastructure.ai.execution.circuit_breaker import ToolCallCircuitBreaker

            circuit_breaker = ToolCallCircuitBreaker()
        except Exception as exc:  # noqa: BLE001 - circuit breaker init failure must degrade to no circuit breaker
            logger.warning("todo_agent_circuit_breaker_init_failed", exc_info=exc)
            circuit_breaker = None

        # 上下文压缩：获取模型的 context window 限制
        context_window_limit = getattr(ai_entity.config, "context_window", 128000)

        while self._should_continue(context):
            # 覆盖最大迭代作为硬停止
            if context.iterations >= max_iterations:
                break

            if await self._is_execution_cancelled(execution.run_id):
                raise asyncio.CancelledError(f"Execution {execution.run_id} cancelled during agent loop")

            step_sequence += 1
            iteration_start = time.time()
            context.iterations += 1

            try:
                # 上下文压缩：超长时自动裁剪旧消息
                try:
                    from src.infrastructure.ai.context_manager import _truncate_messages

                    model_name = getattr(ai_entity.config, "default_model", "gpt-4")
                    max_msg_tokens = int(context_window_limit * 0.75)
                    truncated_msgs, removed = _truncate_messages(messages, max_msg_tokens, model_name, "sliding_window")
                    if removed:
                        logger.info(f"Context compressed: removed {len(removed)} old messages")
                        messages = truncated_msgs
                except Exception as compress_exc:  # noqa: BLE001 - best-effort context compression must not break the agent loop
                    logger.debug(f"Context compression skipped: {compress_exc}")

                # 熔断检查：如果熔断器已打开，注入系统提示并跳过工具调用
                if circuit_breaker and circuit_breaker.is_open:
                    messages.append(
                        Message(role=MessageRole.SYSTEM, content=circuit_breaker.get_circuit_break_message())
                    )
                    await self._emit_ai_event(
                        "text",
                        execution.run_id,
                        execution.todo_id,
                        {"type": "text", "content": "检测到工具调用异常，切换为直接回答模式", "isSystem": True},
                    )
                    # 不传工具，强制模型以文字回答
                    tools = []

                # Emit Thinking Event
                await self._emit_ai_event(
                    "text",
                    execution.run_id,
                    execution.todo_id,
                    {"type": "text", "content": "正在分析上下文...", "isSystem": True},
                )

                # 调用 AI
                try:
                    response = await asyncio.wait_for(
                        self._request_ai(
                            ai_entity=ai_entity,
                            messages=messages,
                            tools=tools,
                        ),
                        timeout=ai_timeout_seconds,
                    )
                except TimeoutError as timeout_error:
                    raise TimeoutError(f"AI call timed out after {ai_timeout_seconds:.1f}s") from timeout_error

                current_response = response
                retry_count = 0  # Reset retry on success

                latency_ms = int((time.time() - iteration_start) * 1000)

                # 提取 token 使用
                token_usage = None
                if hasattr(response, "usage") and response.usage:
                    usage = response.usage
                    prompt_tokens = int(usage.get("prompt_tokens", 0) or usage.get("input_tokens", 0) or 0)
                    completion_tokens = int(usage.get("completion_tokens", 0) or usage.get("output_tokens", 0) or 0)
                    total_tokens = int(usage.get("total_tokens", 0) or (prompt_tokens + completion_tokens))
                    token_usage = TokenUsage(
                        prompt_tokens=prompt_tokens,
                        completion_tokens=completion_tokens,
                        total_tokens=total_tokens,
                    )
                    context.total_tokens += token_usage.total_tokens

                record_latency(
                    operation="responses_create" if api_format == "responses" else "chat_completion",
                    latency_ms=latency_ms,
                    entity_id=execution.entity_id,
                )
                if token_usage:
                    record_tokens(
                        model=ai_entity.config.default_model,
                        prompt_tokens=token_usage.prompt_tokens,
                        completion_tokens=token_usage.completion_tokens,
                        entity_id=execution.entity_id,
                    )

                # 提取原始内容和解析推理
                raw_content = self._extract_response_content(response) or ""
                thinking, content, decision_summary = self._parse_thinking_content(raw_content)

                tool_calls = self._extract_tool_calls(response)

                if thinking:
                    # 渐进式 UX：展示 AI 推理过程
                    await self._emit_ai_event(
                        "insight",
                        execution.run_id,
                        execution.todo_id,
                        {
                            "content": decision_summary or "正在进行深度推理...",
                            "reasoning": thinking[:200] if thinking else None,
                        },
                    )
                elif content and tool_calls:
                    # 渐进式 UX：展示 AI 的思路（Thought 阶段）
                    thought_preview = content[:100].strip()
                    if thought_preview:
                        await self._emit_ai_event(
                            "text",
                            execution.run_id,
                            execution.todo_id,
                            {"type": "text", "content": f"💭 {thought_preview}", "isSystem": True},
                        )

                if tool_calls:
                    context.total_tool_calls += len(tool_calls)

                # 记录 AI 响应作业类型
                response_step = AgentStep.create(
                    run_id=execution.run_id,
                    sequence=step_sequence,
                    step_type=AgentStepType.AI_RESPONSE,
                    role="assistant",
                    content=content,
                    thinking=thinking,
                    decision_summary=decision_summary,
                    tool_calls=[
                        ToolCallRecord(
                            tool_call_id=tc.get("id", ""),
                            tool_name=tc.get("function", {}).get("name", ""),
                            arguments=tc.get("function", {}).get("arguments", {}),
                            status="pending",
                        )
                        for tc in tool_calls
                    ]
                    if tool_calls
                    else None,
                    token_usage=token_usage,
                    latency_ms=latency_ms,
                )
                execution.add_step(response_step)
                await self._execution_repo.save_task_type(response_step)

                if not tool_calls and not content.strip():
                    if thinking:
                        logger.warning("AI response contains only thinking, no action or content")
                    else:
                        break

                if not tool_calls and not thinking and not content.strip():
                    break

                # 纯文本响应，循环结束 (Task Completed)
                # 除非需要 multi-turn reasoning without tools, 但通常 TODO agent 需要 action
                if not tool_calls and content.strip():
                    break

                # 添加 AI 消息到历史
                assistant_content = self._extract_response_content(response) or ""

                messages.append(Message(role=MessageRole.ASSISTANT, content=assistant_content, tool_calls=tool_calls))

                if not tool_calls:
                    continue

                # 执行工具调用
                for tool_call in tool_calls:
                    if await self._is_execution_cancelled(execution.run_id):
                        raise asyncio.CancelledError(f"Execution {execution.run_id} cancelled during tool execution")

                    step_sequence += 1
                    tool_name = tool_call.get("function", {}).get("name", "")
                    tool_args = tool_call.get("function", {}).get("arguments", "{}")
                    tool_call_id = tool_call.get("id", "")
                    normalized_tool_args = self._normalize_tool_payload_value(tool_args)
                    tool_arguments_payload, tool_arguments_truncated = self._truncate_tool_payload(
                        normalized_tool_args,
                        max_chars=self.TOOL_EVENT_ARGUMENT_MAX_CHARS,
                    )

                    # Emit Tool Start
                    await self._emit_ai_event(
                        "tool_start",
                        execution.run_id,
                        execution.todo_id,
                        {
                            "event": "tool_start",
                            "phase": "tool_execute",
                            "tool_name": tool_name,
                            "tool_call_id": tool_call_id,
                            "tool": {
                                "name": tool_name,
                                "color": "text-yellow-400",
                                "bgColor": "bg-yellow-400/10",
                                "borderColor": "border-yellow-400/20",
                            },
                            "content": f"参数: {str(tool_args)[:50]}...",
                            "status": "in_progress",
                            "message": f"tool '{tool_name}' started",
                            "tool_arguments": tool_arguments_payload,
                            "tool_arguments_truncated": tool_arguments_truncated,
                            "tool_result": None,
                            "tool_result_truncated": False,
                            "tool_error": None,
                        },
                    )

                    tool_start = time.time()
                    tool_status = "success"
                    parsed_result: dict[str, Any] | None = None

                    try:
                        # 检查是否是创建子 TODO 的工具
                        if tool_name == "spawn_subtodo":
                            tool_result = await asyncio.wait_for(
                                self._handle_spawn_subtodo(
                                    parent_todo_id=execution.todo_id,
                                    parent_entity_id=execution.entity_id,
                                    args=tool_args,
                                    child_todos=child_todos,
                                ),
                                timeout=tool_timeout_seconds,
                            )
                            if str(tool_result).startswith("Error"):
                                tool_status = "error"
                        else:
                            # 执行普通工具
                            tool_result = await self._execute_tool(
                                tool_name,
                                tool_args,
                                tool_call_id,
                                timeout_seconds=tool_timeout_seconds,
                                user_id=user_id,
                                user_roles=user_roles,
                                invocation_mode=invocation_mode,
                            )
                            try:
                                parsed_result = json.loads(tool_result)
                                if isinstance(parsed_result, dict):
                                    parsed_status = str(parsed_result.get("status", "success"))
                                    if parsed_status != "success":
                                        tool_status = parsed_status
                            except (json.JSONDecodeError, TypeError, ValueError):
                                logger.debug("Failed to parse tool result JSON for status extraction")
                    except TimeoutError:
                        tool_status = "timeout"
                        tool_result = json.dumps(
                            {
                                "error": f"tool '{tool_name}' timed out after {tool_timeout_seconds:.1f}s",
                                "status": "timeout",
                            },
                            ensure_ascii=False,
                        )

                    tool_duration = int((time.time() - tool_start) * 1000)
                    record_tool_usage(tool_name, tool_status, tool_duration)
                    normalized_tool_result = self._normalize_tool_payload_value(tool_result)
                    tool_result_payload, tool_result_truncated = self._truncate_tool_payload(
                        normalized_tool_result,
                        max_chars=self.TOOL_EVENT_RESULT_MAX_CHARS,
                    )
                    tool_error_message = None
                    if tool_status not in {"success", "pending_approval"}:
                        if isinstance(parsed_result, dict):
                            tool_error_message = parsed_result.get("error") or parsed_result.get("message")
                        if not tool_error_message:
                            tool_error_message = str(tool_result)

                    # Emit Tool End
                    await self._emit_ai_event(
                        "tool_end",
                        execution.run_id,
                        execution.todo_id,
                        {
                            "event": "tool_end",
                            "phase": "tool_execute",
                            "tool_name": tool_name,
                            "tool_call_id": tool_call_id,
                            "tool": {"name": tool_name},
                            "content": str(tool_result)[:50],
                            "status": tool_status,
                            "message": f"tool '{tool_name}' finished with status '{tool_status}'",
                            "duration_ms": tool_duration,
                            "tool_arguments": tool_arguments_payload,
                            "tool_arguments_truncated": tool_arguments_truncated,
                            "tool_result": tool_result_payload,
                            "tool_result_truncated": tool_result_truncated,
                            "tool_error": tool_error_message,
                        },
                    )

                    if tool_status == "pending_approval" and isinstance(parsed_result, dict):
                        pending_payload = parsed_result.get("result")
                        approval_result_payload, approval_result_truncated = self._truncate_tool_payload(
                            pending_payload,
                            max_chars=self.TOOL_EVENT_RESULT_MAX_CHARS,
                        )
                        await self._emit_ai_event(
                            "approval_required",
                            execution.run_id,
                            execution.todo_id,
                            {
                                "event": "approval_required",
                                "phase": "approval",
                                "tool_name": tool_name,
                                "tool_call_id": tool_call_id,
                                "tool": {"name": tool_name},
                                "status": "pending_approval",
                                "message": parsed_result.get("message"),
                                "pending_action": pending_payload,
                                "duration_ms": tool_duration,
                                "tool_arguments": tool_arguments_payload,
                                "tool_arguments_truncated": tool_arguments_truncated,
                                "tool_result": approval_result_payload,
                                "tool_result_truncated": approval_result_truncated,
                                "tool_error": None,
                            },
                        )

                    # 记录工具结果作业类型
                    tool_step = AgentStep.create(
                        run_id=execution.run_id,
                        sequence=step_sequence,
                        step_type=AgentStepType.TOOL_RESULT,
                        role="tool",
                        content=tool_result,
                        tool_calls=[
                            ToolCallRecord(
                                tool_call_id=tool_call_id,
                                tool_name=tool_name,
                                arguments=tool_args if isinstance(tool_args, dict) else {},
                                result=tool_result,
                                status=tool_status,
                                duration_ms=tool_duration,
                            )
                        ],
                        latency_ms=tool_duration,
                    )
                    execution.add_step(tool_step)
                    await self._execution_repo.save_task_type(tool_step)

                    # 添加工具结果到消息（失败时附加纠偏引导）
                    tool_message_content = tool_result
                    if tool_status != "success" and tool_status != "pending_approval":
                        tool_message_content = self._inject_error_coaching(tool_name, tool_status, tool_result, tools)
                    messages.append(
                        Message(role=MessageRole.TOOL, content=tool_message_content, tool_call_id=tool_call_id)
                    )

                    # 熔断器记录工具调用结果
                    if circuit_breaker:
                        circuit_breaker.record(tool_name, tool_status)

                    # 因果记忆：记录工具错误到恢复机制
                    if tool_status != "success" and tool_status != "pending_approval":
                        try:
                            error_code = ""
                            if parsed_result and isinstance(parsed_result, dict):
                                error_code = parsed_result.get("code", parsed_result.get("status", ""))
                            self.recovery_mechanism.record_tool_error(
                                tool_name, str(error_code), tool_args if isinstance(tool_args, dict) else None
                            )
                        except Exception as exc:  # noqa: BLE001 - best-effort tool error recording must not break the agent loop
                            logger.debug("tool_error_record_failed", exc_info=exc)

                await self._execution_repo.update_execution(execution)

            except Exception as e:
                record_error(type(e).__name__, execution.entity_id)
                # 错误恢复尝试
                if await self.recovery_mechanism.recover_from_error(e, retry_count, context):
                    retry_count += 1
                    context.iterations -= 1  # Don't count failed iteration towards limit if we retry? Or do?
                    continue
                else:
                    logger.error(f"Error in agent loop: {e}")
                    raise e

        # 输出 Guardrail：对最终回复进行安全检查
        if current_response:
            try:
                from src.infrastructure.ai.guardrails.output_guardrail import OutputGuardrail

                final_text = self._extract_response_content(current_response) or ""
                guardrail = OutputGuardrail()
                # 收集本次执行中所有工具返回的结果
                tool_results_for_check = [
                    msg.content for msg in messages if getattr(msg, "role", None) == MessageRole.TOOL and msg.content
                ]
                result = guardrail.validate(final_text, tool_results_for_check)
                if not result.passed:
                    logger.warning(f"Output guardrail warnings for {execution.run_id}: {result.warnings}")
            except Exception as guardrail_exc:  # noqa: BLE001 - best-effort output guardrail must not break the agent loop
                logger.debug(f"Output guardrail skipped: {guardrail_exc}")

        return current_response, step_sequence

    async def _request_ai(
        self,
        *,
        ai_entity: AIEntity,
        messages: list[Message],
        tools: list[dict[str, Any]],
        prompt_cache_key: str | None = None,
        prompt_cache_retention: str | None = None,
    ) -> Any:
        runner = LLMStreamRunner(ai_entity._ai_client)
        api_format = self._normalize_api_format(getattr(ai_entity.config, "api_format", "chat_completions"))
        if api_format == "responses":
            instructions, input_items = self._messages_to_responses_input(
                messages=messages,
                fallback_instructions=getattr(ai_entity.config, "system_prompt", None) or "",
            )
            result = await runner.run_responses(
                model=ai_entity.config.default_model,
                instructions=instructions or None,
                input=input_items,
                tools=self._convert_tools_for_responses(tools),
                tool_choice="auto" if tools else None,
                temperature=ai_entity.config.temperature,
                max_output_tokens=ai_entity.config.max_tokens,
                prompt_cache_key=prompt_cache_key,
                prompt_cache_retention=prompt_cache_retention,
            )
            return result.raw_response or result

        result = await runner.run_chat(
            messages=messages,
            model=ai_entity.config.default_model,
            temperature=ai_entity.config.temperature,
            max_tokens=ai_entity.config.max_tokens,
            tools=tools if tools else None,
            prompt_cache_key=prompt_cache_key,
            prompt_cache_retention=prompt_cache_retention,
        )
        return result.raw_response or result
