from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

from src.domain.utils.time_utils import utc_now
from src.shared.id_generator import generate_id

from ..models.todo import (
    Todo,
    TodoCategory,
    TodoCategoryValue,
    TodoDescription,
    TodoId,
    TodoPriority,
    TodoPriorityValue,
    TodoStatus,
    TodoStatusValue,
    TodoTitle,
)
from ..models.todo_changes import (
    TodoContentUpdatedChange,
    TodoCreatedChange,
    TodoStateChange,
    TodoStatusUpdatedChange,
)

_PRIORITY_RANK = {
    TodoPriority.CRITICAL.value: 1,
    TodoPriority.HIGH.value: 2,
    TodoPriority.MEDIUM.value: 3,
    TodoPriority.LOW.value: 4,
    TodoPriority.BACKGROUND.value: 5,
}

_RANK_PRIORITY = {rank: name for name, rank in _PRIORITY_RANK.items()}

_STATUS_ALIASES = {
    "PENDING": TodoStatus.PENDING.value,
    "IN_PROGRESS": TodoStatus.IN_PROGRESS.value,
    "COMPLETED": TodoStatus.COMPLETED.value,
    "CANCELLED": TodoStatus.CANCELLED.value,
    "BLOCKED": TodoStatus.BLOCKED.value,
    "pending": TodoStatus.PENDING.value,
    "in_progress": TodoStatus.IN_PROGRESS.value,
    "completed": TodoStatus.COMPLETED.value,
    "cancelled": TodoStatus.CANCELLED.value,
    "blocked": TodoStatus.BLOCKED.value,
}

_PRIORITY_ALIASES = {
    "CRITICAL": TodoPriority.CRITICAL.value,
    "HIGH": TodoPriority.HIGH.value,
    "MEDIUM": TodoPriority.MEDIUM.value,
    "LOW": TodoPriority.LOW.value,
    "BACKGROUND": TodoPriority.BACKGROUND.value,
    "critical": TodoPriority.CRITICAL.value,
    "high": TodoPriority.HIGH.value,
    "medium": TodoPriority.MEDIUM.value,
    "low": TodoPriority.LOW.value,
    "background": TodoPriority.BACKGROUND.value,
}

_CATEGORY_ALIASES = {
    "WORK": TodoCategory.WORK.value,
    "PERSONAL": TodoCategory.PERSONAL.value,
    "MEETING": TodoCategory.MEETING.value,
    "DEADLINE": TodoCategory.DEADLINE.value,
    "RECURRING": TodoCategory.RECURRING.value,
    "work": TodoCategory.WORK.value,
    "personal": TodoCategory.PERSONAL.value,
    "meeting": TodoCategory.MEETING.value,
    "deadline": TodoCategory.DEADLINE.value,
    "recurring": TodoCategory.RECURRING.value,
}


@dataclass
class TodoAggregate:
    todo: Todo
    _uncommitted_changes: list[TodoStateChange] = field(default_factory=list)

    def get_todo(self) -> Todo:
        return self.todo

    def get_uncommitted_changes(self) -> list[TodoStateChange]:
        return list(self._uncommitted_changes)

    def clear_uncommitted(self) -> None:
        self._uncommitted_changes.clear()

    # 与部分仓储适配器保持兼容
    clear_uncommitted_events = clear_uncommitted

    def _record(self, change: TodoStateChange) -> None:
        self._uncommitted_changes.append(change)

    @staticmethod
    def _touch(todo: Todo, updated_by: str) -> None:
        todo.updated_at = utc_now()
        todo.updated_by = updated_by
        todo.version += 1

    @staticmethod
    def _normalize_status(status: Any) -> str:
        if isinstance(status, TodoStatus):
            return status.value
        if status in _STATUS_ALIASES:
            return _STATUS_ALIASES[status]
        if isinstance(status, str):
            return status
        raise ValueError(f"Invalid status: {status}")

    @staticmethod
    def _normalize_priority(priority: Any) -> str:
        if isinstance(priority, TodoPriority):
            return priority.value
        if isinstance(priority, int):
            return _RANK_PRIORITY.get(priority, TodoPriority.MEDIUM.value)
        if isinstance(priority, str) and priority.isdigit():
            return _RANK_PRIORITY.get(int(priority), TodoPriority.MEDIUM.value)
        if priority in _PRIORITY_ALIASES:
            return _PRIORITY_ALIASES[priority]
        if isinstance(priority, str):
            return priority
        raise ValueError(f"Invalid priority: {priority}")

    @staticmethod
    def _normalize_category(category: Any | None) -> str | None:
        if category is None:
            return None
        if isinstance(category, TodoCategory):
            return category.value
        if category in _CATEGORY_ALIASES:
            return _CATEGORY_ALIASES[category]
        if isinstance(category, str):
            return category
        raise ValueError(f"Invalid category: {category}")

    @staticmethod
    def _priority_rank(priority: str) -> int:
        return _PRIORITY_RANK.get(priority, _PRIORITY_RANK[TodoPriority.MEDIUM.value])

    def apply_change(self, change: TodoStateChange) -> None:
        if isinstance(change, TodoCreatedChange):
            if change.title:
                self.todo.title = TodoTitle(change.title)
            if change.description is not None:
                self.todo.description = TodoDescription(change.description) if change.description else None
            if change.priority is not None:
                self.todo.priority = TodoPriorityValue(self._normalize_priority(change.priority))
            if change.status:
                self.todo.status = TodoStatusValue(self._normalize_status(change.status))
            if change.due_date is not None:
                self.todo.due_date = change.due_date
            return

        if isinstance(change, TodoStatusUpdatedChange) and change.new_status:
            self.todo.status = TodoStatusValue(self._normalize_status(change.new_status))

        if isinstance(change, TodoContentUpdatedChange):
            if change.title is not None:
                self.todo.title = TodoTitle(change.title)
            if change.description is not None:
                self.todo.description = TodoDescription(change.description) if change.description else None
            if change.priority is not None:
                self.todo.priority = TodoPriorityValue(self._normalize_priority(change.priority))
            if change.due_date is not None:
                self.todo.due_date = change.due_date

            payload = change.data or {}
            if payload.get("category") is not None:
                category = self._normalize_category(payload.get("category"))
                self.todo.category = TodoCategoryValue(category) if category else None
            if payload.get("assigned_to") is not None:
                self.todo.assigned_to = payload.get("assigned_to")
            if payload.get("tags") is not None:
                self.todo.tags = list(payload.get("tags") or [])
            if payload.get("estimated_duration") is not None:
                self.todo.estimated_duration = payload.get("estimated_duration")
            if payload.get("actual_duration") is not None:
                self.todo.actual_duration = payload.get("actual_duration")
            if payload.get("progress") is not None:
                self.todo.progress = payload.get("progress")
            if payload.get("depends_on") is not None:
                self.todo.depends_on = list(payload.get("depends_on") or [])
            if payload.get("execution_order") is not None:
                self.todo.execution_order = payload.get("execution_order")
            if payload.get("source_type") is not None:
                self.todo.source_type = payload.get("source_type")
            if payload.get("source_id") is not None:
                self.todo.source_id = payload.get("source_id")

    def update_title(self, title: str, updated_by: str = "System") -> None:
        self.todo.update_title(TodoTitle(title), updated_by)
        self._record(TodoContentUpdatedChange(title=title))

    def update_description(self, description: str | None, updated_by: str = "System") -> None:
        self.todo.update_description(
            TodoDescription(description) if description else None,
            updated_by,
        )
        self._record(TodoContentUpdatedChange(description=description))

    def update_status(self, status: str, updated_by: str = "System") -> None:
        old_status = self.todo.status.value
        new_status = self._normalize_status(status)
        self.todo.update_status(TodoStatusValue(new_status), updated_by)
        self._record(TodoStatusUpdatedChange(old_status=old_status, new_status=new_status))

    def update_priority(self, priority: str, updated_by: str = "System") -> None:
        normalized = self._normalize_priority(priority)
        self.todo.update_priority(TodoPriorityValue(normalized), updated_by)
        self._record(
            TodoContentUpdatedChange(
                priority=self._priority_rank(normalized),
                data={"priority": normalized},
            )
        )

    def update_details(
        self,
        title: str | None = None,
        description: str | None = None,
        priority: str | None = None,
        due_date: datetime | None = None,
        estimated_duration: int | None = None,
        tags: list[str] | None = None,
        category: str | None = None,
        updated_by: str = "System",
    ) -> None:
        if title is not None:
            self.todo.update_title(TodoTitle(title), updated_by)
        if description is not None:
            self.todo.update_description(
                TodoDescription(description) if description else None,
                updated_by,
            )
        if priority is not None:
            normalized_priority = self._normalize_priority(priority)
            self.todo.update_priority(TodoPriorityValue(normalized_priority), updated_by)
        else:
            normalized_priority = None

        if due_date is not None:
            self.todo.set_due_date(due_date, updated_by)
        if estimated_duration is not None:
            self.todo.update_estimated_duration(estimated_duration, updated_by)
        if category is not None:
            normalized_category = self._normalize_category(category)
            self.todo.category = TodoCategoryValue(normalized_category) if normalized_category else None
            self._touch(self.todo, updated_by)
        else:
            normalized_category = None

        if tags is not None:
            self.todo.set_tags(tags, updated_by)

        self._record(
            TodoContentUpdatedChange(
                title=title,
                description=description,
                due_date=due_date,
                priority=self._priority_rank(normalized_priority) if normalized_priority else None,
                data={
                    "priority": normalized_priority,
                    "estimated_duration": estimated_duration,
                    "tags": tags,
                    "category": normalized_category,
                },
            )
        )

    def add_tag(self, tag: str, updated_by: str = "System") -> None:
        self.todo.add_tag(tag, updated_by)
        self._record(TodoContentUpdatedChange(data={"tags": list(self.todo.tags)}))

    def assign(self, assignee: str, assigned_by: str = "System") -> None:
        self.todo.assign_to(assignee, assigned_by)
        self._record(TodoContentUpdatedChange(data={"assigned_to": assignee}))

    def complete(
        self,
        actual_duration: int | None = None,
        completed_by: str = "System",
    ) -> None:
        old_status = self.todo.status.value
        if actual_duration is not None:
            self.todo.record_actual_duration(actual_duration, completed_by)
        self.todo.mark_as_completed(completed_by)
        self._record(
            TodoStatusUpdatedChange(
                old_status=old_status,
                new_status=self.todo.status.value,
                data={"actual_duration": actual_duration},
            )
        )

    def cancel(self, reason: str | None = None, cancelled_by: str = "System") -> None:
        old_status = self.todo.status.value
        self.todo.mark_as_cancelled(cancelled_by)
        self._record(
            TodoStatusUpdatedChange(
                old_status=old_status,
                new_status=self.todo.status.value,
                data={"reason": reason},
            )
        )

    def update_progress(self, progress: int, updated_by: str = "System") -> None:
        old_status = self.todo.status.value
        self.todo.update_progress(progress, updated_by)
        self._record(TodoContentUpdatedChange(data={"progress": progress}))
        if self.todo.status.value != old_status:
            self._record(
                TodoStatusUpdatedChange(
                    old_status=old_status,
                    new_status=self.todo.status.value,
                )
            )

    def update_dependencies(self, depends_on: list[str], updated_by: str = "System") -> None:
        self.todo.depends_on = depends_on
        self._touch(self.todo, updated_by)
        self._record(TodoContentUpdatedChange(data={"depends_on": depends_on}))

    def update_execution_order(self, execution_order: int, updated_by: str = "System") -> None:
        self.todo.execution_order = execution_order
        self._touch(self.todo, updated_by)
        self._record(TodoContentUpdatedChange(data={"execution_order": execution_order}))

    def update_source_info(
        self,
        source_type: str,
        source_id: str | None = None,
        updated_by: str = "System",
    ) -> None:
        self.todo.source_type = source_type
        self.todo.source_id = source_id
        self._touch(self.todo, updated_by)
        self._record(TodoContentUpdatedChange(data={"source_type": source_type, "source_id": source_id}))

    @classmethod
    def create(
        cls,
        title: str,
        description: str | None = None,
        priority: str = TodoPriority.MEDIUM.value,
        category: str | None = None,
        due_date: datetime | None = None,
        estimated_duration: int | None = None,
        created_by: str = "System",
        parent_todo_id: str | None = None,
        depends_on: list[str] | None = None,
        execution_order: int = 0,
        source_type: str = "manual",
        source_id: str | None = None,
    ) -> "TodoAggregate":
        normalized_priority = cls._normalize_priority(priority)
        normalized_category = cls._normalize_category(category)

        todo = Todo(
            todo_id=TodoId(generate_id()),
            title=TodoTitle(title),
            description=TodoDescription(description) if description else None,
            priority=TodoPriorityValue(normalized_priority),
            status=TodoStatusValue(TodoStatus.PENDING.value),
            category=TodoCategoryValue(normalized_category) if normalized_category else None,
            due_date=due_date,
            estimated_duration=estimated_duration,
            created_at=utc_now(),
            created_by=created_by,
            updated_at=utc_now(),
            updated_by=created_by,
            parent_todo_id=parent_todo_id,
            depends_on=depends_on or [],
            execution_order=execution_order,
            source_type=source_type,
            source_id=source_id,
        )

        instance = cls(todo)
        instance._record(
            TodoCreatedChange(
                title=title,
                description=description or "",
                due_date=due_date,
                priority=cls._priority_rank(normalized_priority),
                status=TodoStatus.PENDING.value,
                data={
                    "priority": normalized_priority,
                    "category": normalized_category,
                    "estimated_duration": estimated_duration,
                    "parent_todo_id": parent_todo_id,
                    "depends_on": depends_on or [],
                    "execution_order": execution_order,
                    "source_type": source_type,
                    "source_id": source_id,
                },
            )
        )
        return instance
