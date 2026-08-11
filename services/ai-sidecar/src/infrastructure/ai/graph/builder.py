"""
LangGraph 图构建器 (StateGraph Builder)

定义流转规则与有向无环图/带环图 的物理形态。

AIP 扩展：
- 支持 AIP 模式和 Legacy 模式切换
- AIP 模式下使用 object_action_node 替代 tool_exec_node
"""

from collections.abc import Callable
from typing import Any

try:
    from langgraph.graph import END, StateGraph

    _LANGGRAPH_AVAILABLE = True
except ImportError:
    StateGraph, END = Any, Any
    _LANGGRAPH_AVAILABLE = False

from src.infrastructure.ai.graph.aip_nodes import (
    aip_approval_node,
    object_action_node,
    object_context_node,
)
from src.infrastructure.ai.graph.constants import MAX_GRAPH_TOOL_RETRIES
from src.infrastructure.ai.graph.nodes import (
    act_node,
    approval_node,
    graceful_abort_node,
    observe_node,
    summarize_node,
    tool_exec_node,
)
from src.infrastructure.ai.graph.state import AgentCoreState, AIPAgentState, create_initial_aip_state


def route_after_act(state: AgentCoreState) -> str:
    """推理后路由：是否有新调用的工具？"""
    messages = state.get("messages", [])
    if not messages:
        return "summarize"

    last_message = messages[-1]
    if hasattr(last_message, "tool_calls") and last_message.tool_calls:
        return "tools"

    return "summarize"


def route_after_tools(state: AgentCoreState) -> str:
    """工具后路由：如果需要审批，去审批节点；否则返回继续思考"""
    if state.get("requires_approval", False):
        return "wait_for_approval"
    if state.get("consecutive_tool_failures", 0) >= MAX_GRAPH_TOOL_RETRIES:
        return "graceful_abort"
    return "act"


def route_after_act_aip(state: AIPAgentState) -> str:
    """AIP 模式下推理后路由"""
    messages = state.get("messages", [])
    if not messages:
        return "summarize"

    last_message = messages[-1]
    if hasattr(last_message, "tool_calls") and last_message.tool_calls:
        aip_enabled = state.get("aip_enabled", True)
        if aip_enabled:
            return "aip_tools"
        return "tools"

    return "summarize"


def route_after_aip_tools(state: AIPAgentState) -> str:
    """AIP 模式下工具执行后路由"""
    if state.get("requires_approval", False):
        return "aip_approval"

    consecutive_failures = state.get("consecutive_tool_failures", 0)
    if consecutive_failures >= MAX_GRAPH_TOOL_RETRIES:
        return "graceful_abort"

    return "act"


def build_workflow_graph() -> StateGraph:
    """
    构建标准 Workflow Agent 的执行图

    Returns:
        StateGraph: 标准 LangGraph 工作流
    """
    if not _LANGGRAPH_AVAILABLE:
        raise RuntimeError("langgraph is not installed")

    workflow = StateGraph(AgentCoreState)

    workflow.add_node("observe", observe_node)
    workflow.add_node("act", act_node)
    workflow.add_node("tools", tool_exec_node)
    workflow.add_node("summarize", summarize_node)
    workflow.add_node("wait_for_approval", approval_node)
    workflow.add_node("graceful_abort", graceful_abort_node)

    workflow.set_entry_point("observe")
    workflow.add_edge("observe", "act")

    workflow.add_conditional_edges("act", route_after_act, {"tools": "tools", "summarize": "summarize"})

    workflow.add_conditional_edges(
        "tools",
        route_after_tools,
        {"act": "act", "wait_for_approval": "wait_for_approval", "graceful_abort": "graceful_abort"},
    )

    workflow.add_edge("wait_for_approval", "act")
    workflow.add_edge("graceful_abort", END)
    workflow.add_edge("summarize", END)

    return workflow


def build_aip_workflow_graph() -> StateGraph:
    """
    构建 AIP 模式的工作流图

    与标准工作流的区别：
    1. 使用 object_action_node 替代 tool_exec_node
    2. 使用 aip_approval_node 替代 approval_node
    3. 添加 object_context_node 提供对象上下文
    4. 使用 AIPAgentState 替代 AgentCoreState

    Returns:
        StateGraph: AIP 模式的 LangGraph 工作流
    """
    if not _LANGGRAPH_AVAILABLE:
        raise RuntimeError("langgraph is not installed")

    workflow = StateGraph(AIPAgentState)

    workflow.add_node("observe", observe_node)
    workflow.add_node("object_context", object_context_node)
    workflow.add_node("act", act_node)
    workflow.add_node("aip_tools", object_action_node)
    workflow.add_node("summarize", summarize_node)
    workflow.add_node("aip_approval", aip_approval_node)
    workflow.add_node("graceful_abort", graceful_abort_node)

    workflow.set_entry_point("observe")
    workflow.add_edge("observe", "object_context")
    workflow.add_edge("object_context", "act")

    workflow.add_conditional_edges("act", route_after_act_aip, {"aip_tools": "aip_tools", "summarize": "summarize"})

    workflow.add_conditional_edges(
        "aip_tools",
        route_after_aip_tools,
        {"act": "act", "aip_approval": "aip_approval", "graceful_abort": "graceful_abort"},
    )

    workflow.add_edge("aip_approval", "act")
    workflow.add_edge("graceful_abort", END)
    workflow.add_edge("summarize", END)

    return workflow


class AIAgentBuilder:
    """
    AI Agent 构建器

    支持标准模式和 AIP 模式的灵活切换。

    使用示例:
        builder = AIAgentBuilder(config)
        graph = builder.with_aip(True).build()

    Args:
        config: AI 配置对象
    """

    def __init__(self, config: Any | None = None):
        self._config = config
        self._aip_enabled = False
        self._aip_app = None
        self._custom_nodes: dict[str, Callable] = {}
        self._graph = None

    def with_aip(self, enabled: bool = True) -> "AIAgentBuilder":
        """
        启用/禁用 AIP 模式

        Args:
            enabled: 是否启用 AIP 模式

        Returns:
            self 支持链式调用
        """
        self._aip_enabled = enabled
        return self

    def set_aip_app(self, aip_app: Any) -> "AIAgentBuilder":
        """
        设置 AIPApplication 实例

        Args:
            aip_app: AIPApplication 实例

        Returns:
            self 支持链式调用
        """
        self._aip_app = aip_app
        return self

    def add_custom_node(self, name: str, node_fn: Callable) -> "AIAgentBuilder":
        """
        添加自定义节点

        Args:
            name: 节点名称
            node_fn: 节点函数

        Returns:
            self 支持链式调用
        """
        self._custom_nodes[name] = node_fn
        return self

    def build(self) -> StateGraph:
        """
        构建工作流图

        Returns:
            StateGraph: 根据配置构建的工作流图
        """
        if self._aip_enabled:
            self._graph = build_aip_workflow_graph()
        else:
            self._graph = build_workflow_graph()

        for name, node_fn in self._custom_nodes.items():
            self._graph.add_node(name, node_fn)

        return self._graph

    def compile(
        self,
        checkpointer: Any | None = None,
        interrupt_before: list | None = None,
        interrupt_after: list | None = None,
        debug: bool = False,
    ) -> Any:
        """
        编译工作流为可执行的应用

        Args:
            checkpointer: 状态持久化检查点
            interrupt_before: 执行前中断的节点列表
            interrupt_after: 执行后中断的节点列表
            debug: 是否启用调试模式

        Returns:
            可执行的 Runnable
        """
        if self._graph is None:
            self.build()

        compile_kwargs = {}

        if checkpointer is not None:
            compile_kwargs["checkpointer"] = checkpointer

        if interrupt_before is not None:
            compile_kwargs["interrupt_before"] = interrupt_before

        if interrupt_after is not None:
            compile_kwargs["interrupt_after"] = interrupt_after

        if debug:
            compile_kwargs["debug"] = debug

        return self._graph.compile(**compile_kwargs)

    def get_initial_state(self, **kwargs) -> dict[str, Any]:
        """
        获取初始状态

        Args:
            **kwargs: 初始状态参数

        Returns:
            初始状态字典
        """
        if self._aip_enabled:
            return create_initial_aip_state(**kwargs)
        else:
            return {
                "messages": [],
                "todo_id": kwargs.get("todo_id", ""),
                "entity_id": kwargs.get("entity_id", ""),
                "current_plan": "",
                "requires_approval": False,
                "pending_action_id": None,
                "pending_tool_call": None,
                "metrics": {"total_steps": 0, "total_tokens": 0, "total_tool_calls": 0},
                "blackboard_facts": [],
                "error_message": None,
            }

    def get_config(self, **kwargs) -> dict[str, Any]:
        """
        获取运行时配置

        Args:
            **kwargs: 配置参数

        Returns:
            配置字典，用于传递给图的每个节点
        """
        config = {
            "configurable": {
                "aip_enabled": self._aip_enabled,
            }
        }

        if self._aip_app is not None:
            config["configurable"]["aip_app"] = self._aip_app

        if "user_context" in kwargs:
            config["configurable"]["user_context"] = kwargs["user_context"]

        if "llm" in kwargs:
            config["configurable"]["llm"] = kwargs["llm"]

        if "tools" in kwargs:
            config["configurable"]["tools"] = kwargs["tools"]

        return config


__all__ = [
    "AIAgentBuilder",
    "build_aip_workflow_graph",
    "build_workflow_graph",
]
