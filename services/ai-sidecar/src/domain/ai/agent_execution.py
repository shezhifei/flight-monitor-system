"""
AI Agent 执行领域模型

定义 Agent 执行相关的领域实体和值对象。
"""

from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.shared.id_generator import generate_id


class AgentExecutionStatus(StrEnum):
    """Agent 执行状态"""

    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


class AgentStepType(StrEnum):
    """Agent 作业类型类型"""

    USER_INPUT = "user_input"
    AI_RESPONSE = "ai_response"
    TOOL_CALL = "tool_call"
    TOOL_RESULT = "tool_result"
    SYSTEM = "system"


class TodoSourceType(StrEnum):
    """TODO 来源类型"""

    MANUAL = "manual"  # 手动创建
    AI = "ai"  # AI Agent 创建
    EVENT = "event"  # 事件/流程触发
    SYSTEM = "system"  # 系统规则创建


@dataclass
class ToolCallRecord:
    """
    工具调用记录 - 值对象

    记录单次工具调用的详细信息。
    """

    tool_call_id: str
    tool_name: str
    arguments: dict[str, Any]
    result: str | None = None
    status: str = "pending"  # pending/success/error
    started_at: datetime | None = None
    finished_at: datetime | None = None
    duration_ms: int | None = None
    error_message: str | None = None

    def to_dict(self) -> dict[str, Any]:
        """转换为字典"""
        return {
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
            "arguments": self.arguments,
            "result": self.result,
            "status": self.status,
            "started_at": self.started_at.isoformat() if self.started_at else None,
            "finished_at": self.finished_at.isoformat() if self.finished_at else None,
            "duration_ms": self.duration_ms,
            "error_message": self.error_message,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "ToolCallRecord":
        """从字典创建"""
        return cls(
            tool_call_id=data["tool_call_id"],
            tool_name=data["tool_name"],
            arguments=data.get("arguments", {}),
            result=data.get("result"),
            status=data.get("status", "pending"),
            started_at=datetime.fromisoformat(data["started_at"]) if data.get("started_at") else None,
            finished_at=datetime.fromisoformat(data["finished_at"]) if data.get("finished_at") else None,
            duration_ms=data.get("duration_ms"),
            error_message=data.get("error_message"),
        )


@dataclass
class TokenUsage:
    """Token 使用记录 - 值对象"""

    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0

    def to_dict(self) -> dict[str, int]:
        return {
            "prompt_tokens": self.prompt_tokens,
            "completion_tokens": self.completion_tokens,
            "total_tokens": self.total_tokens,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "TokenUsage":
        return cls(
            prompt_tokens=data.get("prompt_tokens", 0),
            completion_tokens=data.get("completion_tokens", 0),
            total_tokens=data.get("total_tokens", 0),
        )


@dataclass
class AgentStep:
    """
    Agent 执行作业类型 - 实体

    记录 Agent 执行过程中的单个作业类型。
    """

    step_id: str
    run_id: str
    sequence: int
    step_type: AgentStepType
    role: str
    content: str
    tool_calls: list[ToolCallRecord] | None = None
    token_usage: TokenUsage | None = None
    latency_ms: int | None = None
    created_at: datetime = field(default_factory=datetime.now)
    metadata: dict[str, Any] = field(default_factory=dict)

    # 决策追踪字段
    thinking: str | None = None
    decision_summary: str | None = None

    @classmethod
    def create(
        cls,
        run_id: str,
        sequence: int,
        step_type: AgentStepType,
        role: str,
        content: str,
        thinking: str | None = None,
        decision_summary: str | None = None,
        **kwargs,
    ) -> "AgentStep":
        """创建新作业类型"""
        return cls(
            step_id=generate_id(),
            run_id=run_id,
            sequence=sequence,
            step_type=step_type,
            role=role,
            content=content,
            created_at=utc_now(),
            thinking=thinking,
            decision_summary=decision_summary,
            **kwargs,
        )

    def to_dict(self) -> dict[str, Any]:
        """转换为字典"""
        return {
            "step_id": self.step_id,
            "run_id": self.run_id,
            "sequence": self.sequence,
            "step_type": self.step_type.value,
            "role": self.role,
            "content": self.content,
            "tool_calls": [tc.to_dict() for tc in self.tool_calls] if self.tool_calls else None,
            "token_usage": self.token_usage.to_dict() if self.token_usage else None,
            "latency_ms": self.latency_ms,
            "created_at": self.created_at.isoformat(),
            "metadata": self.metadata,
            "thinking": self.thinking,
            "decision_summary": self.decision_summary,
        }


@dataclass
class AgentExecution:
    """
    Agent 执行记录 - 聚合根

    代表一次完整的 AI Agent 任务执行，关联到一个 TODO。
    """

    run_id: str
    todo_id: str
    entity_id: str
    entity_config: dict[str, Any]
    status: AgentExecutionStatus = AgentExecutionStatus.PENDING
    total_steps: int = 0
    total_tokens: int = 0
    total_tool_calls: int = 0
    started_at: datetime = field(default_factory=datetime.now)
    finished_at: datetime | None = None
    error_message: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)
    task_types: list[AgentStep] = field(default_factory=list)

    @classmethod
    def create(
        cls, todo_id: str, entity_id: str, entity_config: dict[str, Any], metadata: dict[str, Any] | None = None
    ) -> "AgentExecution":
        """创建新的执行记录"""
        return cls(
            run_id=generate_id(),
            todo_id=todo_id,
            entity_id=entity_id,
            entity_config=entity_config,
            status=AgentExecutionStatus.PENDING,
            started_at=utc_now(),
            metadata=metadata or {},
        )

    def start(self) -> None:
        """开始执行"""
        self.status = AgentExecutionStatus.RUNNING
        self.started_at = utc_now()

    def complete(self) -> None:
        """完成执行"""
        self.status = AgentExecutionStatus.COMPLETED
        self.finished_at = utc_now()

    def fail(self, error_message: str) -> None:
        """执行失败"""
        self.status = AgentExecutionStatus.FAILED
        self.error_message = error_message
        self.finished_at = utc_now()

    def cancel(self) -> None:
        """取消执行"""
        self.status = AgentExecutionStatus.CANCELLED
        self.finished_at = utc_now()

    def add_step(self, step: AgentStep) -> None:
        """添加执行作业类型"""
        self.task_types.append(step)
        self.total_steps += 1

        if step.token_usage:
            self.total_tokens += step.token_usage.total_tokens

        if step.tool_calls:
            self.total_tool_calls += len(step.tool_calls)

    @staticmethod
    def _normalized_runtime_string(value: Any) -> str | None:
        if value is None:
            return None
        normalized = str(value).strip().lower()
        return normalized or None

    @staticmethod
    def _optional_runtime_text(value: Any) -> str | None:
        if value is None:
            return None
        normalized = str(value).strip()
        return normalized or None

    def get_runtime_info(self) -> dict[str, Any]:
        """归一化运行时元数据，供 API 与观测层稳定消费。"""
        metadata = self.metadata if isinstance(self.metadata, dict) else {}
        graph_runtime = metadata.get("graph_runtime")
        is_graph_runtime = isinstance(graph_runtime, dict)
        runtime_path = self._normalized_runtime_string(metadata.get("runtime_path"))
        resolved_path = runtime_path or ("graph" if is_graph_runtime else "legacy")
        requested_path = self._normalized_runtime_string(metadata.get("runtime_path_requested"))
        fallback_reason = self._optional_runtime_text(metadata.get("runtime_fallback_reason"))

        runtime_status = self._normalized_runtime_string(metadata.get("runtime_status")) or self.status.value
        invocation_mode = None
        tool_names: list[str] = []

        if is_graph_runtime:
            invocation_mode = self._normalized_runtime_string(graph_runtime.get("invocation_mode"))
            raw_tool_names = graph_runtime.get("tool_names") or []
            if isinstance(raw_tool_names, list):
                tool_names = [str(name).strip() for name in raw_tool_names if str(name).strip()]

        if invocation_mode is None:
            invocation_mode = self._normalized_runtime_string(metadata.get("invocation_mode"))

        return {
            "path": resolved_path,
            "status": runtime_status,
            "invocation_mode": invocation_mode,
            "resumable": bool(metadata.get("langgraph_checkpoint")),
            "requested_path": requested_path or ("graph" if is_graph_runtime else resolved_path),
            "fallback_reason": fallback_reason,
            "tool_names": tool_names,
        }

    def to_dict(self) -> dict[str, Any]:
        """转换为字典"""
        runtime_info = self.get_runtime_info()
        return {
            "run_id": self.run_id,
            "todo_id": self.todo_id,
            "entity_id": self.entity_id,
            "entity_config": self.entity_config,
            "status": self.status.value,
            "runtime": runtime_info,
            "runtime_path": runtime_info["path"],
            "runtime_status": runtime_info["status"],
            "total_steps": self.total_steps,
            "total_tokens": self.total_tokens,
            "total_tool_calls": self.total_tool_calls,
            "started_at": self.started_at.isoformat(),
            "finished_at": self.finished_at.isoformat() if self.finished_at else None,
            "error_message": self.error_message,
            "metadata": self.metadata,
            "task_types": [s.to_dict() for s in self.task_types],
        }

    @property
    def duration_ms(self) -> int | None:
        """计算执行时长（毫秒）"""
        if self.finished_at:
            return int((self.finished_at - self.started_at).total_seconds() * 1000)
        return None


@dataclass
class ExecutionGraph:
    """
    执行图 - 管理 TODO 依赖关系和并行执行

    用于构建和管理 TODO 树的执行顺序。
    """

    todos: dict[str, "TodoExecutionNode"] = field(default_factory=dict)
    completed: set = field(default_factory=set)
    _dependents: dict[str, set] = field(default_factory=dict)
    _remaining_dependencies: dict[str, int] = field(default_factory=dict)
    _ready_todo_ids: set = field(default_factory=set)

    def add_todo(self, todo_id: str, depends_on: list[str], entity_id: str) -> None:
        """添加 TODO 到执行图"""
        dependency_set = set(depends_on)
        self.todos[todo_id] = TodoExecutionNode(todo_id=todo_id, depends_on=dependency_set, entity_id=entity_id)

        unresolved = 0
        for dependency_id in dependency_set:
            self._dependents.setdefault(dependency_id, set()).add(todo_id)
            if dependency_id not in self.completed:
                unresolved += 1

        self._remaining_dependencies[todo_id] = unresolved
        if unresolved == 0 and todo_id not in self.completed:
            self._ready_todo_ids.add(todo_id)

    def get_ready_todos(self) -> list["TodoExecutionNode"]:
        """获取所有可以执行的 TODO（依赖已满足）"""
        return [
            self.todos[todo_id]
            for todo_id in self._ready_todo_ids
            if todo_id not in self.completed and todo_id in self.todos
        ]

    def mark_completed(self, todo_id: str) -> None:
        """标记 TODO 为已完成"""
        if todo_id in self.completed:
            return

        self.completed.add(todo_id)
        self._ready_todo_ids.discard(todo_id)

        for dependent_id in self._dependents.get(todo_id, set()):
            remaining = self._remaining_dependencies.get(dependent_id, 0) - 1
            self._remaining_dependencies[dependent_id] = remaining
            if remaining <= 0 and dependent_id not in self.completed:
                self._ready_todo_ids.add(dependent_id)

    def has_pending(self) -> bool:
        """是否还有待执行的 TODO"""
        return len(self.completed) < len(self.todos)


@dataclass
class TodoExecutionNode:
    """TODO 执行节点"""

    todo_id: str
    depends_on: set
    entity_id: str
    execution: AgentExecution | None = None


__all__ = [
    "AgentExecution",
    "AgentExecutionStatus",
    "AgentStep",
    "AgentStepType",
    "ExecutionGraph",
    "TodoExecutionNode",
    "TodoSourceType",
    "TokenUsage",
    "ToolCallRecord",
]
