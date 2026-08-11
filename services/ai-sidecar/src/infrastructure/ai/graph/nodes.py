"""
LangGraph 状态机核心分块节点 (Nodes)。

节点实现需要同时兼容：
1. 真正安装了 langgraph/langchain-core 的运行环境；
2. 当前仓库尚未补齐依赖时的最小本地测试环境。
"""

from __future__ import annotations

import inspect
from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Any

try:
    from langchain_core.messages import AIMessage, HumanMessage, SystemMessage, ToolMessage
    from langchain_core.runnables import RunnableConfig
except ImportError:  # pragma: no cover - fallback exercised in tests
    RunnableConfig = dict[str, Any]  # type: ignore[assignment]  # fallback when langchain-core is absent

    @dataclass
    class _FallbackMessage:
        content: str = ""
        role: str = "assistant"

    @dataclass
    class HumanMessage(_FallbackMessage):
        role: str = "user"

    @dataclass
    class SystemMessage(_FallbackMessage):
        role: str = "system"

    @dataclass
    class AIMessage(_FallbackMessage):
        role: str = "assistant"
        tool_calls: list[dict[str, Any]] = field(default_factory=list)

    @dataclass
    class ToolMessage(_FallbackMessage):
        tool_call_id: str = ""
        role: str = "tool"


from src.infrastructure.ai.graph.state import AgentCoreState
from src.infrastructure.common.exceptions import LLM_EXCEPTIONS
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

try:
    from src.infrastructure.ai.langchain_adapter.tools import (
        PermissionDeniedException,
        RequiresApprovalException,
        ToolResourceNotFoundFailure,
        ToolValidationFailure,
    )
except Exception as exc:  # pragma: no cover - keeps graph importable before adapter lands  # noqa: BLE001 - import guard for optional adapter
    logger.warning("langchain_adapter.tools import unavailable; using fallback stubs: %s", exc)

    class RequiresApprovalException(Exception):
        def __init__(self, action_id: str, tool_name: str, message: str):
            super().__init__(message)
            self.action_id = action_id
            self.tool_name = tool_name
            self.message = message

    class PermissionDeniedException(Exception):
        pass

    class ToolValidationFailure(Exception):
        def __init__(self, code, message, retryable=True):
            super().__init__(message)
            self.message = message
            self.code = code
            self.retryable = retryable

    class ToolResourceNotFoundFailure(Exception):
        def __init__(self, code, message, retryable=True):
            super().__init__(message)
            self.message = message
            self.code = code
            self.retryable = retryable


try:
    from langgraph.types import interrupt
except ImportError:  # pragma: no cover - fallback exercised in tests

    def interrupt(*args: Any, **kwargs: Any) -> dict[str, Any]:
        return {"status": "approved"}


def create_human_message(content: str) -> HumanMessage:
    return HumanMessage(content=str(content or ""))


def message_content_to_text(message: Any) -> str:
    if message is None:
        return ""
    content = getattr(message, "content", None)
    if content is None and isinstance(message, dict):
        content = message.get("content")
    if isinstance(content, list):
        return "\n".join(str(item) for item in content if item is not None).strip()
    return str(content or "").strip()


def _coerce_message(message: Any) -> Any:
    if isinstance(message, (AIMessage, HumanMessage, SystemMessage, ToolMessage)):
        return message

    if hasattr(message, "content"):
        return message

    if not isinstance(message, dict):
        return HumanMessage(content=str(message or ""))

    role = str(message.get("role") or "assistant").strip().lower()
    content = str(message.get("content") or "")
    tool_calls = message.get("tool_calls") or []

    if role == "system":
        return SystemMessage(content=content)
    if role == "user":
        return HumanMessage(content=content)
    if role == "tool":
        return ToolMessage(content=content, tool_call_id=str(message.get("tool_call_id") or ""))
    return AIMessage(content=content, tool_calls=list(tool_calls or []))


def coerce_messages(messages: Iterable[Any]) -> list[Any]:
    return [_coerce_message(message) for message in messages or []]


def _get_configurable(config: RunnableConfig) -> dict[str, Any]:
    if isinstance(config, dict):
        return dict(config.get("configurable") or {})
    getter = getattr(config, "get", None)
    if callable(getter):
        return dict(getter("configurable", {}) or {})
    return {}


def _get_llm_model(config: RunnableConfig) -> Any:
    return _get_configurable(config).get("llm")


def _get_tool_map(config: RunnableConfig) -> dict[str, Any]:
    configurable = _get_configurable(config)
    tools = configurable.get("tools") or []
    mapped: dict[str, Any] = {}
    for tool in tools:
        name = getattr(tool, "name", None)
        if name:
            mapped[str(name)] = tool
    return mapped


def _extract_tool_calls(message: Any) -> list[dict[str, Any]]:
    raw_tool_calls = getattr(message, "tool_calls", None)
    if raw_tool_calls is None and isinstance(message, dict):
        raw_tool_calls = message.get("tool_calls")
    normalized: list[dict[str, Any]] = []
    for tool_call in raw_tool_calls or []:
        if isinstance(tool_call, dict):
            args = tool_call.get("args")
            if args is None and isinstance(tool_call.get("function"), dict):
                args = tool_call["function"].get("arguments")
            normalized.append(
                {
                    "id": str(tool_call.get("id") or ""),
                    "name": str(tool_call.get("name") or ((tool_call.get("function") or {}).get("name")) or ""),
                    "args": args if args is not None else {},
                }
            )
    return normalized


async def observe_node(state: AgentCoreState, config: RunnableConfig) -> dict[str, Any]:
    blackboard_facts = state.get("blackboard_facts", [])
    facts_text = "\n".join(blackboard_facts) if blackboard_facts else "目前尚无前置事实积累。"
    return {"current_plan": f"根据业务环境，已有事实：{facts_text}"}


async def act_node(state: AgentCoreState, config: RunnableConfig) -> dict[str, Any]:
    llm = _get_llm_model(config)
    if llm is None:
        return {"error_message": "graph runtime missing llm"}

    messages = coerce_messages(state.get("messages", []))
    sys_prompt = SystemMessage(
        content=(
            f"当前待办 ID: {state.get('todo_id', 'unknown')}\n"
            f"实体 ID: {state.get('entity_id', 'unknown')}\n"
            f"前情回顾/黑板事实: {state.get('current_plan', '无')}\n"
            "务必严格使用你的工具，不要做无用假设。"
        )
    )
    if not any(getattr(message, "role", "") == "system" for message in messages):
        messages = [sys_prompt, *messages]

    try:
        response = await llm.ainvoke(messages, config=config)
        return {"messages": [_coerce_message(response)]}
    except LLM_EXCEPTIONS as exc:
        logger.error("Agent Act Node Error: %s", exc)
        return {"error_message": str(exc)}


async def tool_exec_node(state: AgentCoreState, config: RunnableConfig) -> dict[str, Any]:
    messages = coerce_messages(state.get("messages", []))
    if not messages:
        return {}

    tool_calls = _extract_tool_calls(messages[-1])
    if not tool_calls:
        return {}

    tool_map = _get_tool_map(config)
    outputs: list[Any] = []
    requires_approval = False
    pending_action_id: str | None = None
    pending_tool_call: dict[str, Any] | None = None

    consecutive_failures = state.get("consecutive_tool_failures", 0)
    max_failures = state.get("max_consecutive_tool_failures", 0)
    last_error_code = state.get("last_tool_error_code", "")
    last_error_message = state.get("last_tool_error_message", "")

    for tool_call in tool_calls:
        name = str(tool_call.get("name") or "").strip()
        args = tool_call.get("args") or {}
        call_id = str(tool_call.get("id") or "")

        tool = tool_map.get(name)
        if tool is None:
            outputs.append(
                ToolMessage(
                    content=f"Error: 未注册或无权使用的工具 '{name}'",
                    tool_call_id=call_id,
                )
            )
            consecutive_failures += 1
            max_failures = max(max_failures, consecutive_failures)
            last_error_code = "TOOL_NOT_FOUND"
            last_error_message = f"Tool {name} not found"
            continue

        try:
            result = await tool.ainvoke(args, config=config)
            outputs.append(ToolMessage(content=str(result), tool_call_id=call_id))
            consecutive_failures = 0
            last_error_code = ""
            last_error_message = ""
        except RequiresApprovalException as exc:
            logger.warning("工具执行受阻，触发审批路由: %s", exc.message)
            requires_approval = True
            pending_action_id = exc.action_id
            pending_tool_call = {
                "id": call_id,
                "name": name,
                "args": args,
            }
            break
        except (ToolValidationFailure, ToolResourceNotFoundFailure) as exc:
            logger.warning("工具验证/资源获取失败: %s", exc)
            consecutive_failures += 1
            max_failures = max(max_failures, consecutive_failures)
            last_error_code = getattr(exc, "code", "VALIDATION_FAILED")
            last_error_message = str(exc)

            outputs.append(
                ToolMessage(
                    content=str(exc),
                    tool_call_id=call_id,
                )
            )
        except PermissionDeniedException as exc:
            logger.warning("工具执行越权: %s", exc)
            outputs.append(
                ToolMessage(
                    content=f"【拒绝访问】执行 '{name}' 权限不足: {exc}",
                    tool_call_id=call_id,
                )
            )
        except Exception as exc:  # noqa: BLE001 - tool execution must not break graph flow
            logger.error("工具执行报错: %s", exc)
            outputs.append(
                ToolMessage(
                    content=f"运行时异常: {exc}",
                    tool_call_id=call_id,
                )
            )

    return {
        "messages": outputs,
        "requires_approval": requires_approval,
        "pending_action_id": pending_action_id,
        "pending_tool_call": pending_tool_call,
        "consecutive_tool_failures": consecutive_failures,
        "max_consecutive_tool_failures": max_failures,
        "last_tool_error_code": last_error_code,
        "last_tool_error_message": last_error_message,
    }


async def approval_node(state: AgentCoreState, config: RunnableConfig) -> dict[str, Any]:
    configurable = _get_configurable(config)
    resume_data = configurable.get("resume")
    if resume_data is None:
        maybe_resume = interrupt(
            {
                "action_id": state.get("pending_action_id"),
                "interrupt_reason": "needs_approval",
            }
        )
        resume_data = await maybe_resume if inspect.isawaitable(maybe_resume) else maybe_resume

    status = "rejected"
    if isinstance(resume_data, dict):
        status = str(resume_data.get("status") or "rejected").strip().lower() or "rejected"
    elif resume_data is not None:
        status = str(resume_data).strip().lower() or "rejected"

    messages: list[Any] = [SystemMessage(content=f"【系统回执】审批结果：{status}。")]
    pending_tool_call = state.get("pending_tool_call") or {}

    if status == "approved" and pending_tool_call:
        tool_name = str(pending_tool_call.get("name") or "").strip()
        tool_args = pending_tool_call.get("args") or {}
        tool_call_id = str(pending_tool_call.get("id") or "")
        tool = _get_tool_map(config).get(tool_name)

        if tool is None:
            messages.append(
                ToolMessage(
                    content=f"Error: 审批后未找到工具 '{tool_name}'",
                    tool_call_id=tool_call_id,
                )
            )
        else:
            approval_config = dict(config) if isinstance(config, dict) else {"configurable": _get_configurable(config)}
            approval_config["configurable"] = {
                **_get_configurable(config),
                "approved_pending_action_id": state.get("pending_action_id"),
                "approval_status": status,
            }
            try:
                result = await tool.ainvoke(tool_args, config=approval_config)
                messages.append(ToolMessage(content=str(result), tool_call_id=tool_call_id))
            except RequiresApprovalException as exc:
                logger.warning("审批后工具再次进入待审批: %s", exc.message)
                return {
                    "requires_approval": True,
                    "pending_action_id": exc.action_id,
                    "pending_tool_call": {
                        "id": tool_call_id,
                        "name": tool_name,
                        "args": tool_args,
                    },
                }
            except Exception as exc:  # noqa: BLE001 - post-approval tool execution must not break flow
                logger.error("审批后执行挂起工具失败: %s", exc)
                messages.append(
                    ToolMessage(
                        content=f"审批后工具执行失败: {exc}",
                        tool_call_id=tool_call_id,
                    )
                )

    return {
        "requires_approval": False,
        "pending_action_id": None,
        "pending_tool_call": None,
        "messages": messages,
    }


async def summarize_node(state: AgentCoreState, config: RunnableConfig) -> dict[str, Any]:
    messages = coerce_messages(state.get("messages", []))
    if not messages:
        return {}

    last_text = message_content_to_text(messages[-1])
    if not last_text:
        return {}

    return {"blackboard_facts": [f"执行记录提要: {last_text[:100]}..."]}


async def graceful_abort_node(state: AgentCoreState, config: RunnableConfig) -> dict[str, Any]:
    error_msg = f"已连续失败 {state.get('consecutive_tool_failures', 0)} 次，自动终止以免死循环。"
    return {
        "messages": [SystemMessage(content=error_msg)],
        "error_message": error_msg,
    }
