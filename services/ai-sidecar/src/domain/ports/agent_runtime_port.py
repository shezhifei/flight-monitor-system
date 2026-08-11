"""
Agent 运行时端口

定义应用层调用 Agent 引擎的统一边界。
隔离了具体引擎（如原生 Executor 或 LangGraph 实现）对业务的侵入。
"""

from abc import ABC, abstractmethod
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any


@dataclass
class AgentTodoInfoDTO:
    """引擎中转用的 Todo 基本信息 DTO"""

    todo_id: str
    title: str
    description: str | None
    entity_id: str
    parent_todo_id: str | None
    depends_on: list[str]
    execution_order: int


@dataclass
class AgentExecutionResultDTO:
    """引擎执行结果 DTO"""

    run_id: str
    todo_id: str
    status: Any
    response: str | None
    total_steps: int
    total_tokens: int
    total_tool_calls: int
    duration_ms: int
    error_message: str | None = None
    child_todos: list[str] = None

    def __post_init__(self):
        if self.child_todos is None:
            self.child_todos = []


class AgentRuntimePort(ABC):
    """
    Agent 运行时统一访问端口
    """

    @abstractmethod
    def set_todo_callbacks(self, creator: Callable, loader: Callable, updater: Callable) -> None:
        """设置环境上下文回调, 用于创建子任务或报告状态"""
        pass

    @abstractmethod
    async def execute_todo(
        self,
        todo: AgentTodoInfoDTO,
        max_iterations: int = 10,
        system_prompt_override: str | None = None,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: Any = None,
        shared_pool: Any | None = None,
        root_todo_id: str | None = None,
    ) -> AgentExecutionResultDTO:
        """执行单一任务"""
        pass

    @abstractmethod
    async def execute_todo_tree(
        self,
        root_todo_id: str,
        max_iterations_per_todo: int = 10,
        fail_fast: bool = True,
        user_id: str | None = None,
        user_roles: list[str] | None = None,
        invocation_mode: Any = None,
        shared_pool: Any | None = None,
    ) -> dict[str, AgentExecutionResultDTO]:
        """按依赖树执行任务"""
        pass

    @abstractmethod
    async def cancel_execution(self, run_id: str) -> bool:
        """取消执行"""
        pass

    @abstractmethod
    async def get_execution(self, run_id: str) -> Any | None:
        """获取执行单对象/字典"""
        pass
