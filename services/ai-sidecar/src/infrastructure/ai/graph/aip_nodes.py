"""
AIP 专用 LangGraph 节点 (AIP Nodes)

提供基于 Ontology 的 AIP 模式专用节点：
- object_action_node: 执行 Ontology Action
- ontology_query_node: 本体查询
- aip_approval_node: 增强的审批节点

这些节点与现有的 tool_exec_node/approval_node 并行运行，
通过 aip_enabled 标志切换。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from src.infrastructure.logging.core import get_logger

if TYPE_CHECKING:
    from langchain_core.runnables import RunnableConfig

    from .state import AIPAgentState

logger = get_logger(__name__)


def _get_configurable(config: RunnableConfig) -> dict[str, Any]:
    """从 config 中获取 configurable"""
    if isinstance(config, dict):
        return dict(config.get("configurable") or {})
    getter = getattr(config, "get", None)
    if callable(getter):
        return dict(getter("configurable", {}) or {})
    return {}


def _get_aip_app(config: RunnableConfig) -> Any | None:
    """从 config 中获取 AIPApplication 实例"""
    configurable = _get_configurable(config)
    return configurable.get("aip_app")


def _get_user_context(config: RunnableConfig) -> dict[str, Any]:
    """从 config 中获取用户上下文"""
    configurable = _get_configurable(config)
    return configurable.get("user_context", {})


def _extract_tool_calls(message: Any) -> list[dict[str, Any]]:
    """从消息中提取工具调用"""
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


def _coerce_message(message: Any) -> Any:
    """强制转换消息格式"""
    try:
        from langchain_core.messages import AIMessage, HumanMessage, SystemMessage, ToolMessage

        if isinstance(message, (AIMessage, HumanMessage, SystemMessage, ToolMessage)):
            return message
    except ImportError:
        pass

    if hasattr(message, "content"):
        return message

    if not isinstance(message, dict):
        from .nodes import create_human_message

        return create_human_message(str(message or ""))

    role = str(message.get("role") or "assistant").strip().lower()
    content = str(message.get("content") or "")
    tool_calls = message.get("tool_calls") or []

    try:
        from langchain_core.messages import AIMessage, ToolMessage

        if role == "tool":
            return ToolMessage(content=content, tool_call_id=str(message.get("tool_call_id") or ""))
        return AIMessage(content=content, tool_calls=list(tool_calls or []))
    except ImportError:
        return message


@dataclass
class AIPActionResult:
    """AIP Action 执行结果"""

    success: bool
    object_type: str
    object_id: str
    action: str
    result: dict[str, Any] | None = None
    error: str | None = None
    requires_approval: bool = False
    pending_action_id: str | None = None
    change_preview: dict[str, Any] | None = None
    permission_denied: bool = False
    constraint_violations: list[str] = field(default_factory=list)


async def object_action_node(state: AIPAgentState, config: RunnableConfig) -> dict[str, Any]:
    """
    AIP 专用节点：执行 Ontology Action

    工作流程：
    1. 从 LLM 消息中提取工具调用
    2. 调用 AIPFunctionRegistry.resolve_action() 解析为 Ontology Action
    3. 调用 ObjectACL.check_permission() 检查权限
    4. 调用 AIPActionExecutor.execute() 执行 Action
    5. 处理需要审批的情况，设置 requires_approval 标志

    Args:
        state: AIPAgentState，包含 object_context, action_queue 等
        config: RunnableConfig，包含 aip_app, user_context

    Returns:
        更新后的状态字典
    """
    try:
        from langchain_core.messages import ToolMessage

        tool_message_cls = ToolMessage
    except ImportError:
        tool_message_cls = dict

    messages = state.get("messages", [])
    if not messages:
        return {}

    last_message = messages[-1]
    tool_calls = _extract_tool_calls(last_message)

    if not tool_calls:
        return {}

    aip_app = _get_aip_app(config)
    if aip_app is None:
        logger.warning("AIP application not found in config, falling back to legacy mode")
        return {}

    user_context = _get_user_context(config)
    principal = f"user:{user_context.get('user_id', 'anonymous')}"
    user_roles = user_context.get("roles", [])

    outputs: list[Any] = []
    action_results: list[AIPActionResult] = []
    requires_approval = False
    pending_approvals: list[dict[str, Any]] = []
    object_change_previews: list[dict[str, Any]] = []

    for tool_call in tool_calls:
        tool_name = str(tool_call.get("name") or "").strip()
        args = tool_call.get("args") or {}
        call_id = str(tool_call.get("id") or "")

        try:
            action_result = await _execute_aip_action(
                aip_app=aip_app, tool_name=tool_name, args=args, principal=principal, user_roles=user_roles
            )
            action_results.append(action_result)

            if action_result.requires_approval:
                requires_approval = True
                pending_approvals.append(
                    {
                        "action_id": action_result.pending_action_id,
                        "tool_call_id": call_id,
                        "tool_name": tool_name,
                        "object_type": action_result.object_type,
                        "object_id": action_result.object_id,
                        "action": action_result.action,
                        "change_preview": action_result.change_preview,
                    }
                )
                if action_result.change_preview:
                    object_change_previews.append(action_result.change_preview)

                try:
                    outputs.append(
                        tool_message_cls(
                            content=f"【待审批】Action '{action_result.action}' on {action_result.object_type}:{action_result.object_id} 需要人工审批。",
                            tool_call_id=call_id,
                        )
                    )
                except Exception as exc:  # noqa: BLE001 - ToolMessage construction fallback must catch any failure
                    logger.warning("aip ToolMessage construction failed (approval): %s", exc)
                    outputs.append(
                        {
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": f"【待审批】Action '{action_result.action}' on {action_result.object_type}:{action_result.object_id} 需要人工审批。",
                        }
                    )

            elif action_result.permission_denied:
                try:
                    outputs.append(
                        tool_message_cls(
                            content=f"【拒绝访问】权限不足: {action_result.error}",
                            tool_call_id=call_id,
                        )
                    )
                except Exception as exc:  # noqa: BLE001 - ToolMessage construction fallback must catch any failure
                    logger.warning("aip ToolMessage construction failed (denied): %s", exc)
                    outputs.append(
                        {
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": f"【拒绝访问】权限不足: {action_result.error}",
                        }
                    )

            elif action_result.error:
                try:
                    outputs.append(
                        tool_message_cls(
                            content=f"【执行错误】{action_result.error}",
                            tool_call_id=call_id,
                        )
                    )
                except Exception as exc:  # noqa: BLE001 - ToolMessage construction fallback must catch any failure
                    logger.warning("aip ToolMessage construction failed (error): %s", exc)
                    outputs.append(
                        {"role": "tool", "tool_call_id": call_id, "content": f"【执行错误】{action_result.error}"}
                    )

            else:
                result_content = _format_action_result(action_result)
                try:
                    outputs.append(
                        tool_message_cls(
                            content=result_content,
                            tool_call_id=call_id,
                        )
                    )
                except Exception as exc:  # noqa: BLE001 - ToolMessage construction fallback must catch any failure
                    logger.warning("aip ToolMessage construction failed (success): %s", exc)
                    outputs.append({"role": "tool", "tool_call_id": call_id, "content": result_content})

        except Exception as exc:  # noqa: BLE001 - top-level action execution handler must catch all failures
            logger.error(f"AIP Action execution error for '{tool_name}': {exc}")
            try:
                outputs.append(
                    tool_message_cls(
                        content=f"【系统错误】Action 执行异常: {exc}",
                        tool_call_id=call_id,
                    )
                )
            except Exception as exc_inner:  # noqa: BLE001 - ToolMessage construction fallback must catch any failure
                logger.warning("aip ToolMessage construction failed (execution error): %s", exc_inner)
                outputs.append(
                    {"role": "tool", "tool_call_id": call_id, "content": f"【系统错误】Action 执行异常: {exc}"}
                )

    return {
        "messages": outputs,
        "requires_approval": requires_approval,
        "pending_approvals": pending_approvals,
        "object_change_previews": object_change_previews,
        "last_action_result": action_results[-1].__dict__ if action_results else None,
    }


async def _execute_aip_action(
    aip_app: Any, tool_name: str, args: dict[str, Any], principal: str, user_roles: list[str]
) -> AIPActionResult:
    """执行单个 AIP Action"""
    try:
        resolved = aip_app.function_registry.resolve_action(tool_name, args)
    except ValueError:
        return AIPActionResult(
            success=False,
            object_type="Unknown",
            object_id=args.get("object_id", ""),
            action=tool_name,
            error=f"Unknown function: {tool_name}",
        )

    object_type = resolved.get("object_type", "Unknown")
    object_id = resolved.get("object_id", args.get("object_id", ""))
    action = resolved.get("action_name", tool_name)

    permission_result = aip_app.check_permission(
        principal=principal, object_type=object_type, object_id=object_id, permission="execute"
    )

    if not permission_result.get("allowed", False):
        return AIPActionResult(
            success=False,
            object_type=object_type,
            object_id=object_id,
            action=action,
            permission_denied=True,
            error=permission_result.get("reason", "Permission denied"),
        )

    exec_result = await aip_app.execute_action(
        principal=principal,
        object_type=object_type,
        object_id=object_id,
        action=action,
        parameters=args,
        invocation_mode="user_requested",
    )

    return AIPActionResult(
        success=exec_result.get("status") == "completed",
        object_type=object_type,
        object_id=object_id,
        action=action,
        result=exec_result.get("result"),
        error=exec_result.get("error"),
        requires_approval=exec_result.get("status") == "pending_approval",
        pending_action_id=exec_result.get("pending_action_id"),
        change_preview=exec_result.get("change_preview"),
    )


def _format_action_result(result: AIPActionResult) -> str:
    """格式化 Action 结果为文本"""
    if result.result:
        import json

        return f"【成功】{result.action} 执行完成\n{json.dumps(result.result, ensure_ascii=False, indent=2)}"
    return f"【成功】{result.action} on {result.object_type}:{result.object_id} 执行完成"


async def ontology_query_node(state: AIPAgentState, config: RunnableConfig) -> dict[str, Any]:
    """
    AIP 专用节点：本体查询

    利用 Ontology 的对象模型进行语义查询和关系遍历。

    支持：
    - 跨对象关联查询 (Flight → Team → Equipment)
    - 基于约束的推理 (找所有可用的大机位)
    - 路径查询 (查询某对象的完整上下文)

    Args:
        state: AIPAgentState
        config: RunnableConfig

    Returns:
        查询结果注入 object_context
    """
    aip_app = _get_aip_app(config)
    if aip_app is None:
        return {}

    query_type = state.get("object_context", {}).get("query_type")

    if not query_type:
        return {}

    relevant_objects = state.get("object_context", {}).get("relevant_objects", [])

    context_bridge = aip_app._context_bridge
    query_context = context_bridge.build_query_context(query_type, relevant_objects)

    return {
        "object_context": {
            "query_type": query_type,
            "relevant_objects": relevant_objects,
            "context_text": query_context,
        }
    }


async def aip_approval_node(state: AIPAgentState, config: RunnableConfig) -> dict[str, Any]:
    """
    AIP 增强审批节点

    扩展现有 approval_node，支持 Ontology-aware 的变更展示。

    与原有 approval_node 的区别：
    - 使用 object_change_previews 提供 Schema 感知的 Diff
    - 增强 UI 提示信息

    Args:
        state: AIPAgentState
        config: RunnableConfig

    Returns:
        更新后的状态
    """
    try:
        from langgraph.types import interrupt
    except ImportError:
        from .nodes import interrupt

        if not callable(interrupt) or not callable(interrupt):

            def interrupt(*args, **kwargs):
                return {"status": "approved"}

    configurable = _get_configurable(config)
    resume_data = configurable.get("resume")

    if resume_data is None:
        pending_approvals = state.get("pending_approvals", [])
        change_previews = state.get("object_change_previews", [])

        interrupt_data = {
            "action_id": state.get("pending_action_id"),
            "interrupt_reason": "needs_approval",
            "pending_approvals": pending_approvals,
            "change_previews": change_previews,
        }

        if change_previews:
            latest_preview = change_previews[-1]
            interrupt_data["change_summary"] = _summarize_change(latest_preview)

        maybe_resume = interrupt(interrupt_data)
        resume_data = await maybe_resume if hasattr(maybe_resume, "__await__") else maybe_resume

    status = "rejected"
    if isinstance(resume_data, dict):
        status = str(resume_data.get("status") or "rejected").strip().lower() or "rejected"
    elif resume_data is not None:
        status = str(resume_data).strip().lower() or "rejected"

    messages_content = f"【系统回执】审批结果：{status}。"

    if status == "approved":
        messages_content += "\n\n✅ 变更已批准，系统将执行操作。"

        pending_approvals = state.get("pending_approvals", [])
        if pending_approvals:
            messages_content += f"\n\n审批对象：{len(pending_approvals)} 个待审批操作"
            for pa in pending_approvals:
                obj_type = pa.get("object_type", "")
                obj_id = pa.get("object_id", "")
                action = pa.get("action", "")
                messages_content += f"\n• {obj_type}:{obj_id} - {action}"
    else:
        messages_content += "\n\n❌ 变更已被拒绝，操作不会执行。"

    try:
        from langchain_core.messages import SystemMessage

        messages = [
            SystemMessage(content=messages_content),
        ]

        if status == "approved":
            for pa in state.get("pending_approvals", []):
                pending_id = pa.get("action_id")
                if pending_id:
                    await _execute_approved_action(pending_id, config)

    except ImportError:
        messages = [{"role": "system", "content": messages_content}]

    return {
        "requires_approval": False,
        "pending_action_id": None,
        "pending_tool_call": None,
        "pending_approvals": [],
        "object_change_previews": [],
        "messages": messages,
    }


async def _execute_approved_action(action_id: str, config: RunnableConfig) -> None:
    """执行已批准的 Action"""
    aip_app = _get_aip_app(config)
    if aip_app is None:
        return

    try:
        pending_store = None
        try:
            from src.infrastructure.ai.tools.pending_actions import get_pending_action_store

            pending_store = get_pending_action_store()
        except ImportError:
            pass

        if pending_store:
            pending_action = await pending_store.get_action(action_id)
            if pending_action:
                user_context = _get_user_context(config)
                await aip_app.execute_action(
                    principal=f"user:{user_context.get('user_id', 'system')}",
                    object_type=pending_action.entity_type,
                    object_id=pending_action.entity_id,
                    action=pending_action.tool_name.split(".")[-1]
                    if "." in pending_action.tool_name
                    else pending_action.tool_name,
                    parameters=pending_action.arguments,
                    invocation_mode="approval_approved",
                )

    except Exception as exc:  # noqa: BLE001 - top-level approved action execution handler must catch all failures
        logger.error(f"Failed to execute approved action {action_id}: {exc}")


def _summarize_change(change_preview: dict[str, Any]) -> str:
    """生成变更摘要文本"""
    object_type = change_preview.get("object_type", "Unknown")
    object_id = change_preview.get("object_id", "")
    action = change_preview.get("action", "")

    property_changes = change_preview.get("property_changes", [])

    summary_parts = [f"将对 {object_type} '{object_id}' 执行 '{action}' 操作"]

    if property_changes:
        summary_parts.append(f"涉及 {len(property_changes)} 项变更：")
        for change in property_changes[:5]:
            prop = change.get("property", "")
            before = change.get("before", "N/A")
            after = change.get("after", "N/A")
            summary_parts.append(f"  • {prop}: {before} → {after}")

    risk_level = change_preview.get("risk_level", "NORMAL")
    if risk_level in ("HIGH", "CRITICAL"):
        summary_parts.append(f"\n⚠️ 高风险操作 (风险等级: {risk_level})")

    return "\n".join(summary_parts)


async def object_context_node(state: AIPAgentState, config: RunnableConfig) -> dict[str, Any]:
    """
    AIP 节点：构建对象上下文

    在 LLM 调用前，注入当前操作的对象上下文，
    帮助 LLM 理解业务实体。

    Args:
        state: AIPAgentState
        config: RunnableConfig

    Returns:
        注入到 current_plan
    """
    aip_app = _get_aip_app(config)
    if aip_app is None:
        return {}

    entity_id = state.get("entity_id", "")

    if not entity_id:
        return {}

    resolved_objects = state.get("resolved_objects", {})

    if entity_id in resolved_objects:
        object_data = resolved_objects[entity_id]
    else:
        object_data = await _resolve_object_context(aip_app, entity_id)
        resolved_objects[entity_id] = object_data

    if not object_data:
        return {}

    context_bridge = aip_app._context_bridge
    object_type = object_data.get("object_type", "Unknown")
    object_schema = context_bridge.inject_object_schema(object_type)

    context_text = f"当前操作对象: {object_schema}\n\n对象数据: {object_data}"

    return {
        "current_plan": context_text,
        "object_context": object_data,
        "resolved_objects": resolved_objects,
    }


async def _resolve_object_context(aip_app: Any, entity_id: str) -> dict[str, Any]:
    """解析对象上下文"""
    try:
        parts = entity_id.split("_", 1)
        object_type = parts[0] if len(parts) > 0 else "Unknown"
        object_id = parts[1] if len(parts) > 1 else entity_id

        if hasattr(aip_app, "_action_executor") and aip_app._action_executor:
            state = await aip_app._action_executor._get_object_state(object_type, object_id)
            if state:
                return {"object_type": object_type, "object_id": object_id, "data": state}

    except Exception as exc:  # noqa: BLE001 - top-level object context resolver must catch all failures
        logger.warning(f"Failed to resolve object context for '{entity_id}': {exc}")

    return {"object_type": "Unknown", "object_id": entity_id, "data": {}}
