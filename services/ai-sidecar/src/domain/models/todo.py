"""待办事项领域模型

定义待办事项实体及其相关值对象和枚举
"""

import re
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any

from src.domain.utils.time_utils import to_utc, utc_now
from src.shared.id_generator import generate_id

from ..exceptions.base import BusinessRuleException, ValidationException, ValueObjectValidationException
from .value_objects import BaseValueObject


class TodoPriority(Enum):
    """待办事项优先级枚举"""

    CRITICAL = "关键"
    HIGH = "高"
    MEDIUM = "中"
    LOW = "低"
    BACKGROUND = "后台"


class TodoStatus(Enum):
    """待办事项状态枚举"""

    PENDING = "待办"
    IN_PROGRESS = "进行中"
    COMPLETED = "已完成"
    CANCELLED = "已取消"
    BLOCKED = "阻塞中"


class TodoCategory(Enum):
    """待办事项分类枚举"""

    WORK = "工作"
    PERSONAL = "个人"
    MEETING = "会议"
    DEADLINE = "截止日期"
    RECURRING = "重复任务"


_ULID_PATTERN = re.compile(r"^[0-9A-HJKMNP-TV-Z]{26}$")


def _validate_enum_value(value: str, enum_cls: type[Enum], object_name: str, field_name: str) -> None:
    if not isinstance(value, str):
        raise ValueObjectValidationException(object_name, f"待办事项{field_name}必须是字符串", "value")

    try:
        enum_cls(value)
    except ValueError as exc:
        valid_values = [item.value for item in enum_cls]
        raise ValueObjectValidationException(
            object_name, f"无效的{field_name}: {value}，有效值: {', '.join(valid_values)}", "value"
        ) from exc


@dataclass(frozen=True)
class TodoId(BaseValueObject[str]):
    """待办事项ID值对象"""

    value: str

    def _validate(self, value: str) -> None:
        """验证待办事项ID值"""
        if not value or not isinstance(value, str):
            raise ValueObjectValidationException("TodoId", "待办事项ID不能为空", "value")

        if not _ULID_PATTERN.match(value):
            raise ValueObjectValidationException("TodoId", "待办事项ID必须是有效的ULID格式", "value")

    def _get_value_type(self) -> type:
        return str

    @classmethod
    def generate(cls) -> "TodoId":
        """生成新的待办事项ID"""
        return cls(value=generate_id())


@dataclass(frozen=True)
class TodoTitle(BaseValueObject[str]):
    """待办事项标题值对象"""

    value: str

    def _validate(self, value: str) -> None:
        """验证待办事项标题值"""
        if not value or not isinstance(value, str):
            raise ValueObjectValidationException("TodoTitle", "待办事项标题不能为空", "value")

        # 验证标题长度
        if len(value.strip()) == 0:
            raise ValueObjectValidationException("TodoTitle", "待办事项标题不能只包含空白字符", "value")

        if len(value) > 200:
            raise ValueObjectValidationException("TodoTitle", "待办事项标题不能超过200个字符", "value")

    def _get_value_type(self) -> type:
        return str


@dataclass(frozen=True)
class TodoDescription(BaseValueObject[str]):
    """待办事项描述值对象"""

    value: str

    def _validate(self, value: str) -> None:
        """验证待办事项描述值"""
        if not isinstance(value, str):
            raise ValueObjectValidationException("TodoDescription", "待办事项描述必须是字符串", "value")

        # 验证描述长度
        if len(value) > 2000:
            raise ValueObjectValidationException("TodoDescription", "待办事项描述不能超过2000个字符", "value")

    def _get_value_type(self) -> type:
        return str


@dataclass(frozen=True)
class TodoPriorityValue(BaseValueObject[str]):
    """待办事项优先级值对象"""

    value: str

    def _validate(self, value: str) -> None:
        """验证待办事项优先级值"""
        _validate_enum_value(value, TodoPriority, "TodoPriorityValue", "优先级")

    def _get_value_type(self) -> type:
        return str

    def to_enum(self) -> TodoPriority:
        """转换为枚举值"""
        return TodoPriority(self.value)


@dataclass(frozen=True)
class TodoStatusValue(BaseValueObject[str]):
    """待办事项状态值对象"""

    value: str

    def _validate(self, value: str) -> None:
        """验证待办事项状态值"""
        _validate_enum_value(value, TodoStatus, "TodoStatusValue", "状态")

    def _get_value_type(self) -> type:
        return str

    def to_enum(self) -> TodoStatus:
        """转换为枚举值"""
        return TodoStatus(self.value)


@dataclass(frozen=True)
class TodoCategoryValue(BaseValueObject[str]):
    """待办事项分类值对象"""

    value: str

    def _validate(self, value: str) -> None:
        """验证待办事项分类值"""
        _validate_enum_value(value, TodoCategory, "TodoCategoryValue", "分类")

    def _get_value_type(self) -> type:
        return str

    def to_enum(self) -> TodoCategory:
        """转换为枚举值"""
        return TodoCategory(self.value)


@dataclass
class Todo:
    """待办事项实体 - 领域模型"""

    todo_id: TodoId
    title: TodoTitle
    description: TodoDescription | None = None
    priority: TodoPriorityValue = field(default_factory=lambda: TodoPriorityValue("中"))
    status: TodoStatusValue = field(default_factory=lambda: TodoStatusValue("待办"))
    category: TodoCategoryValue | None = None
    due_date: datetime | None = None
    assigned_to: str | None = None
    tags: list[str] = field(default_factory=list)
    estimated_duration: int | None = None  # 预计完成时间（分钟）
    actual_duration: int | None = None  # 实际完成时间（分钟）
    progress: int = 0  # 完成进度百分比（0-100）
    is_recurring: bool = False
    recurring_pattern: str | None = None  # 重复模式（如："daily", "weekly", "monthly"）

    # 层级结构字段
    parent_todo_id: str | None = None  # 父待办事项 ID
    execution_order: int = 0  # 执行顺序
    depends_on: list[str] = field(default_factory=list)  # 依赖的待办事项列表

    # 来源信息
    source_type: str = "manual"  # 来源类型（手动/自动）
    source_id: str | None = None  # 来源 ID

    # 软删除标记
    is_deleted: bool = False
    deleted_at: datetime | None = None

    # 审计字段
    created_at: datetime = field(default_factory=datetime.now)
    updated_at: datetime = field(default_factory=datetime.now)
    created_by: str = "System"
    updated_by: str = "System"
    version: int = 0

    def __post_init__(self):
        """初始化后处理 - 验证业务规则"""
        # 验证待办事项对象的合法性
        self._validate_todo()

        # 验证进度范围
        if not 0 <= self.progress <= 100:
            raise ValidationException("进度必须在0-100之间", "progress")

    def _validate_todo(self):
        """验证待办事项对象的合法性"""
        status = self.status.to_enum()

        if status == TodoStatus.COMPLETED and self.progress != 100:
            raise ValidationException("已完成的任务进度必须为100%", "progress")

        if status == TodoStatus.PENDING and self.progress > 0:
            raise ValidationException("待办任务进度必须为0%", "progress")

        if self.actual_duration is not None and self.actual_duration < 0:
            raise ValidationException("实际完成时间不能为负数", "actual_duration")

        if self.estimated_duration is not None and self.estimated_duration <= 0:
            raise ValidationException("预计完成时间必须大于0", "estimated_duration")

        if self.is_recurring and not self.recurring_pattern:
            raise ValidationException("重复任务必须设置重复模式", "recurring_pattern")

    def _touch(self, updated_by: str) -> None:
        self.updated_at = utc_now()
        self.updated_by = updated_by
        self.version += 1

    def _update_field(self, field_name: str, value: Any, updated_by: str = "System") -> None:
        setattr(self, field_name, value)
        self._touch(updated_by)

    def update_title(self, new_title: TodoTitle, updated_by: str = "System") -> None:
        """更新待办事项标题"""
        self._update_field("title", new_title, updated_by)

    def update_description(self, new_description: TodoDescription | None, updated_by: str = "System") -> None:
        """更新待办事项描述"""
        self._update_field("description", new_description, updated_by)

    def update_priority(self, new_priority: TodoPriorityValue, updated_by: str = "System") -> None:
        """更新待办事项优先级"""
        self._update_field("priority", new_priority, updated_by)

    def update_status(self, new_status: TodoStatusValue, updated_by: str = "System") -> None:
        """更新待办事项状态"""
        # 验证状态转换规则
        if not self._can_transition_to(new_status.to_enum()):
            raise BusinessRuleException(f"无法从 {self.status.value} 转换为 {new_status.value}", "status_transition")

        self.status = new_status

        self._touch(updated_by)

        # 根据状态更新进度
        if new_status.to_enum() == TodoStatus.COMPLETED:
            self.progress = 100
        elif new_status.to_enum() == TodoStatus.PENDING:
            self.progress = 0

    def _can_transition_to(self, new_status: TodoStatus) -> bool:
        """检查是否可以转换为新状态"""
        # 定义状态转换规则
        allowed_transitions = {
            TodoStatus.PENDING: [TodoStatus.IN_PROGRESS, TodoStatus.CANCELLED, TodoStatus.BLOCKED],
            TodoStatus.IN_PROGRESS: [
                TodoStatus.COMPLETED,
                TodoStatus.PENDING,
                TodoStatus.CANCELLED,
                TodoStatus.BLOCKED,
            ],
            TodoStatus.COMPLETED: [],  # 已完成的任务不能转换到其他状态
            TodoStatus.CANCELLED: [],  # 已取消的任务不能转换到其他状态
            TodoStatus.BLOCKED: [TodoStatus.PENDING, TodoStatus.IN_PROGRESS, TodoStatus.CANCELLED],
        }

        return new_status in allowed_transitions.get(self.status.to_enum(), [])

    def update_progress(self, new_progress: int, updated_by: str = "System") -> None:
        """更新完成进度"""
        if not 0 <= new_progress <= 100:
            raise ValidationException("进度必须在0-100之间", "progress")

        # 验证进度与状态的一致性
        if new_progress == 100 and self.status.to_enum() != TodoStatus.COMPLETED:
            # 如果进度达到100%，自动更新状态为已完成
            self.status = TodoStatusValue("已完成")
        elif new_progress > 0 and self.status.to_enum() == TodoStatus.PENDING:
            # 如果进度大于0，自动更新状态为进行中
            self.status = TodoStatusValue("进行中")

        self.progress = new_progress
        self._touch(updated_by)

    def assign_to(self, assignee: str, assigned_by: str = "System") -> None:
        """分配待办事项"""
        if not assignee or not assignee.strip():
            raise ValidationException("被分配人不能为空", "assigned_to")

        self._update_field("assigned_to", assignee.strip(), assigned_by)

    def set_due_date(self, due_date: datetime | None, updated_by: str = "System") -> None:
        """设置截止日期"""
        self._update_field("due_date", due_date, updated_by)

    def add_tag(self, tag: str, updated_by: str = "System") -> None:
        """添加标签"""
        if not tag or not tag.strip():
            raise ValidationException("标签不能为空", "tag")

        tag = tag.strip()
        if tag not in self.tags:
            self.tags.append(tag)
            self._touch(updated_by)

    def set_tags(self, tags: list[str], updated_by: str = "System") -> None:
        """批量设置标签（去重且保持原始顺序）。"""
        unique_tags: list[str] = []
        seen = set()
        for raw_tag in tags or []:
            tag = str(raw_tag or "").strip()
            if not tag or tag in seen:
                continue
            seen.add(tag)
            unique_tags.append(tag)

        self.tags = unique_tags
        self._touch(updated_by)

    def remove_tag(self, tag: str, updated_by: str = "System") -> None:
        """移除标签"""
        if tag in self.tags:
            self.tags.remove(tag)
            self._touch(updated_by)

    def update_estimated_duration(self, duration: int | None, updated_by: str = "System") -> None:
        """更新预计完成时间"""
        if duration is not None and duration <= 0:
            raise ValidationException("预计完成时间必须大于0", "estimated_duration")

        self._update_field("estimated_duration", duration, updated_by)

    def record_actual_duration(self, duration: int | None, updated_by: str = "System") -> None:
        """记录实际完成时间"""
        if duration is not None and duration < 0:
            raise ValidationException("实际完成时间不能为负数", "actual_duration")

        self._update_field("actual_duration", duration, updated_by)

    def mark_as_completed(self, completed_by: str = "System") -> None:
        """标记为已完成"""
        self.update_status(TodoStatusValue("已完成"), completed_by)
        self.progress = 100
        self.updated_at = utc_now()
        self.updated_by = completed_by

    def mark_as_cancelled(self, cancelled_by: str = "System") -> None:
        """标记为已取消"""
        self.update_status(TodoStatusValue("已取消"), cancelled_by)

    def is_overdue(self) -> bool:
        """检查是否逾期"""
        if not self.due_date:
            return False

        if self.status.to_enum() in [TodoStatus.COMPLETED, TodoStatus.CANCELLED]:
            return False

        due_date_utc = to_utc(self.due_date)
        if due_date_utc is None:
            return False
        return utc_now() > due_date_utc

    def get_priority_level(self) -> int:
        """获取优先级数值等级（数值越小优先级越高）"""
        priority_mapping = {
            TodoPriority.CRITICAL: 1,
            TodoPriority.HIGH: 2,
            TodoPriority.MEDIUM: 3,
            TodoPriority.LOW: 4,
            TodoPriority.BACKGROUND: 5,
        }
        return priority_mapping.get(self.priority.to_enum(), 3)

    def is_high_priority(self) -> bool:
        """是否为高优先级任务"""
        return self.get_priority_level() <= 2

    def can_be_edited(self) -> bool:
        """是否可以编辑"""
        return self.status.to_enum() not in [TodoStatus.COMPLETED, TodoStatus.CANCELLED]

    def get_summary(self) -> str:
        """获取待办事项摘要"""
        status_emoji = {
            TodoStatus.PENDING: "⏳",
            TodoStatus.IN_PROGRESS: "🔄",
            TodoStatus.COMPLETED: "✅",
            TodoStatus.CANCELLED: "❌",
            TodoStatus.BLOCKED: "🚫",
        }

        emoji = status_emoji.get(self.status.to_enum(), "📋")
        overdue_marker = " ⚠️" if self.is_overdue() else ""

        return f"{emoji} {self.title.value} {overdue_marker}"
