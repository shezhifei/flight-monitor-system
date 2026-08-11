"""待办事项查询和统计模型"""

from dataclasses import dataclass, field
from enum import Enum


class TodoSortBy(Enum):
    """排序字段枚举"""

    CREATED_AT = "created_at"
    UPDATED_AT = "updated_at"
    DUE_DATE = "due_date"
    PRIORITY = "priority"
    STATUS = "status"
    TITLE = "title"


class TodoSortOrder(Enum):
    """排序顺序枚举"""

    ASC = "asc"
    DESC = "desc"


@dataclass
class TodoQueryOptions:
    """待办事项查询选项"""

    page: int | None = 1
    limit: int | None = 20
    offset: int | None = 0
    sort_by: TodoSortBy | None = TodoSortBy.CREATED_AT
    sort_order: TodoSortOrder = TodoSortOrder.DESC
    sort_desc: bool = True  # 兼容性字段

    # 过滤字段
    status_filter: str | None = None
    priority_filter: str | None = None
    category_filter: str | None = None
    assignee_filter: str | None = None
    source_type_filter: str | None = None
    source_id_filter: str | None = None

    include_deleted: bool = False


@dataclass
class TodoStats:
    """待办事项统计数据"""

    total_count: int = 0
    completed_count: int = 0
    pending_count: int = 0
    cancelled_count: int = 0
    overdue_count: int = 0
    high_priority_count: int = 0
    recurring_count: int = 0
    due_today_count: int = 0
    due_soon_count: int = 0
    completion_rate: float = 0.0

    # 分布统计
    status_stats: dict[str, int] = field(default_factory=dict)
    priority_stats: dict[str, int] = field(default_factory=dict)
    category_stats: dict[str, int] = field(default_factory=dict)
    assignee_stats: dict[str, int] = field(default_factory=dict)

    # 兼容性
    priority_distribution: dict[str, int] = field(default_factory=dict)
    category_distribution: dict[str, int] = field(default_factory=dict)
