"""
LangGraph 状态机的数据底座 (State Definition)

定义在图中流转的整体数据结构。
使用 TypedDict 配合 Annotated 实现特定字段的追加/覆盖逻辑。
"""

import operator
from collections.abc import Sequence
from typing import Annotated, Any, TypedDict

try:
    from typing import NotRequired
except ImportError:
    from typing import NotRequired

try:
    from langchain_core.messages import BaseMessage
    from langgraph.graph.message import add_messages
except ImportError:

    def add_messages(left, right):
        return left + right

    BaseMessage = Any


class AgentMetrics(TypedDict, total=False):
    """用于审计和计费的数据打点"""

    total_steps: int
    total_tokens: int
    total_tool_calls: int


class AgentCoreState(TypedDict):
    """
    Agent 核心流转状态

    - messages: LangChain 格式对话流水，使用 add_messages 聚合。
    - todo_info: 当前正在执行的任务的基础信息。
    - blackboard_facts: 总结出的事实（不再依赖死长 Message list）。
    - requires_approval: 是否由于执行高危动作进入"等待审批"分叉口。
    - pending_action_id: 如果挂起，对应的动作单 ID。
    - metrics: 性能追踪指标。
    """

    messages: Annotated[Sequence[BaseMessage], add_messages]

    todo_id: str
    entity_id: str
    current_plan: str

    requires_approval: bool
    pending_action_id: str | None
    pending_tool_call: dict[str, Any] | None

    metrics: AgentMetrics
    blackboard_facts: Annotated[list[str], operator.add]
    error_message: str | None

    consecutive_tool_failures: NotRequired[int]
    max_consecutive_tool_failures: NotRequired[int]
    last_tool_error_code: NotRequired[str]
    last_tool_error_message: NotRequired[str]
