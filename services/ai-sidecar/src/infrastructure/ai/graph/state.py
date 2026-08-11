"""
LangGraph 状态机的数据底座 (State Definition)

定义在图中流转的整体数据结构。
使用 TypedDict 配合 Annotated 实现特定字段的追加/覆盖逻辑。

AIP 扩展：
- AIPAgentState 继承 AgentCoreState，添加 Ontology 相关字段
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


class AIPAgentState(AgentCoreState, total=False):
    """
    AIP 模式扩展状态

    继承 AgentCoreState，添加 Ontology 相关的上下文信息。

    新增字段说明：
    - object_context: 当前操作的对象上下文，用于 LLM 理解业务实体
    - action_queue: 待执行的 Ontology Action 队列
    - resolved_objects: 已解析的对象缓存，避免重复查询
    - pending_approvals: 待审批的变更列表（增强审批体验）
    - aip_enabled: 是否启用 AIP 模式
    - last_action_result: 最近一次 Action 执行结果
    """

    object_context: NotRequired[dict[str, Any] | None]
    action_queue: NotRequired[list[dict[str, Any]]]
    resolved_objects: NotRequired[dict[str, Any]]
    pending_approvals: NotRequired[list[dict[str, Any]]]
    aip_enabled: NotRequired[bool]
    last_action_result: NotRequired[dict[str, Any] | None]
    object_change_previews: NotRequired[list[dict[str, Any]]]


def create_initial_aip_state(
    todo_id: str = "", entity_id: str = "", aip_enabled: bool = True, **kwargs
) -> AIPAgentState:
    """
    创建 AIP 初始状态

    Args:
        todo_id: 待办ID
        entity_id: 实体ID
        aip_enabled: 是否启用AIP模式
        **kwargs: 其他初始字段

    Returns:
        AIPAgentState: 初始状态字典
    """
    return AIPAgentState(
        messages=[],
        todo_id=todo_id,
        entity_id=entity_id,
        current_plan="",
        requires_approval=False,
        pending_action_id=None,
        pending_tool_call=None,
        metrics=AgentMetrics(total_steps=0, total_tokens=0, total_tool_calls=0),
        blackboard_facts=[],
        error_message=None,
        object_context=None,
        action_queue=[],
        resolved_objects={},
        pending_approvals=[],
        aip_enabled=aip_enabled,
        last_action_result=None,
        object_change_previews=[],
        **kwargs,
    )
