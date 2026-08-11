"""
待办事项AI工具定义

提供与OpenAI function calling兼容的待办事项工具schema定义。
这些工具允许AI通过自然语言对话管理待办事项。
"""

from enum import StrEnum
from typing import Any

from .base import BaseToolDefinition, OperationLevel, ToolCategory, build_openai_tools


class TodoToolName(StrEnum):
    """待办事项工具名称枚举"""

    CREATE_TODO = "create_todo"
    GET_TODO = "get_todo"
    LIST_TODOS = "list_todos"
    UPDATE_TODO = "update_todo"
    COMPLETE_TODO = "complete_todo"
    CANCEL_TODO = "cancel_todo"
    ASSIGN_TODO = "assign_todo"
    UPDATE_PROGRESS = "update_progress"
    SEARCH_TODOS = "search_todos"
    GET_TODO_STATS = "get_todo_stats"


# 别名保持向后兼容
TodoToolDefinition = BaseToolDefinition


# 待办事项工具定义列表
TODO_TOOL_DEFINITIONS: list[TodoToolDefinition] = [
    TodoToolDefinition(
        name=TodoToolName.CREATE_TODO.value,
        description="创建一个新的待办事项。返回新创建待办事项的ID。",
        parameters={
            "title": {"type": "string", "description": "待办事项的标题，不超过200个字符"},
            "description": {"type": "string", "description": "待办事项的详细描述，可选，不超过2000个字符"},
            "priority": {
                "type": "string",
                "enum": ["关键", "高", "中", "低", "后台"],
                "description": "待办事项的优先级，默认为'中'",
            },
            "category": {
                "type": "string",
                "enum": ["工作", "个人", "会议", "截止日期", "重复任务"],
                "description": "待办事项的分类，可选",
            },
            "due_date": {"type": "string", "description": "截止日期，ISO 8601格式 (YYYY-MM-DDTHH:MM:SS)，可选"},
            "estimated_duration": {"type": "integer", "description": "预计完成时间（分钟），可选"},
            "tags": {"type": "array", "items": {"type": "string"}, "description": "标签列表，可选"},
            "assignee": {"type": "string", "description": "分配给谁，可选"},
        },
        required_params=["title"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.WORKSPACE_WRITE,
        side_effect=True,
    ),
    TodoToolDefinition(
        name=TodoToolName.GET_TODO.value,
        description="根据ID获取待办事项的详细信息。",
        parameters={"todo_id": {"type": "string", "description": "待办事项的唯一ID"}},
        required_params=["todo_id"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.READ,
    ),
    TodoToolDefinition(
        name=TodoToolName.LIST_TODOS.value,
        description="列出待办事项，支持多种筛选条件。",
        parameters={
            "assignee": {"type": "string", "description": "按分配人筛选"},
            "status": {
                "type": "string",
                "enum": ["待办", "进行中", "已完成", "已取消", "阻塞中"],
                "description": "按状态筛选",
            },
            "priority": {"type": "string", "enum": ["关键", "高", "中", "低", "后台"], "description": "按优先级筛选"},
            "category": {
                "type": "string",
                "enum": ["工作", "个人", "会议", "截止日期", "重复任务"],
                "description": "按分类筛选",
            },
            "overdue_only": {"type": "boolean", "description": "仅显示过期的待办事项"},
            "due_today": {"type": "boolean", "description": "仅显示今天到期的待办事项"},
            "high_priority_only": {"type": "boolean", "description": "仅显示高优先级（关键/高）待办事项"},
            "limit": {"type": "integer", "description": "返回结果数量限制，默认20"},
        },
        required_params=[],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.READ,
    ),
    TodoToolDefinition(
        name=TodoToolName.UPDATE_TODO.value,
        description="更新待办事项的属性。只需提供要更新的字段。",
        parameters={
            "todo_id": {"type": "string", "description": "待办事项的唯一ID"},
            "title": {"type": "string", "description": "新标题"},
            "description": {"type": "string", "description": "新描述"},
            "priority": {"type": "string", "enum": ["关键", "高", "中", "低", "后台"], "description": "新优先级"},
            "due_date": {"type": "string", "description": "新截止日期，ISO 8601格式"},
            "estimated_duration": {"type": "integer", "description": "新预计完成时间（分钟）"},
            "tags": {"type": "array", "items": {"type": "string"}, "description": "新标签列表（会替换原有标签）"},
        },
        required_params=["todo_id"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    TodoToolDefinition(
        name=TodoToolName.COMPLETE_TODO.value,
        description="将待办事项标记为已完成。",
        parameters={
            "todo_id": {"type": "string", "description": "待办事项的唯一ID"},
            "actual_duration": {"type": "integer", "description": "实际完成时间（分钟），可选"},
        },
        required_params=["todo_id"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    TodoToolDefinition(
        name=TodoToolName.CANCEL_TODO.value,
        description="取消待办事项。",
        parameters={
            "todo_id": {"type": "string", "description": "待办事项的唯一ID"},
            "reason": {"type": "string", "description": "取消原因，可选"},
        },
        required_params=["todo_id"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    TodoToolDefinition(
        name=TodoToolName.ASSIGN_TODO.value,
        description="将待办事项分配给指定用户。",
        parameters={
            "todo_id": {"type": "string", "description": "待办事项的唯一ID"},
            "assignee": {"type": "string", "description": "被分配人的用户名或ID"},
        },
        required_params=["todo_id", "assignee"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    TodoToolDefinition(
        name=TodoToolName.UPDATE_PROGRESS.value,
        description="更新待办事项的完成进度。",
        parameters={
            "todo_id": {"type": "string", "description": "待办事项的唯一ID"},
            "progress": {"type": "integer", "description": "完成进度百分比，0-100之间的整数"},
        },
        required_params=["todo_id", "progress"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.ASSISTED_WRITE,
        side_effect=True,
    ),
    TodoToolDefinition(
        name=TodoToolName.SEARCH_TODOS.value,
        description="通过关键词搜索待办事项。",
        parameters={
            "query": {"type": "string", "description": "搜索关键词"},
            "limit": {"type": "integer", "description": "返回结果数量限制，默认20"},
        },
        required_params=["query"],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.READ,
    ),
    TodoToolDefinition(
        name=TodoToolName.GET_TODO_STATS.value,
        description="获取待办事项统计信息。",
        parameters={"assignee": {"type": "string", "description": "按分配人筛选统计，可选"}},
        required_params=[],
        category=ToolCategory.TODO,
        operation_level=OperationLevel.READ,
    ),
]


# OpenAI格式的工具列表
TODO_TOOLS: list[dict[str, Any]] = build_openai_tools(TODO_TOOL_DEFINITIONS)


def get_todo_tools(include: list[str] | None = None, exclude: list[str] | None = None) -> list[dict[str, Any]]:
    """
    获取待办事项工具列表

    Args:
        include: 仅包含指定的工具名称列表
        exclude: 排除指定的工具名称列表

    Returns:
        OpenAI格式的工具定义列表
    """
    tools = []

    for tool_def in TODO_TOOL_DEFINITIONS:
        # 如果指定了include，仅包含指定的工具
        if include and tool_def.name not in include:
            continue

        # 如果指定了exclude，排除指定的工具
        if exclude and tool_def.name in exclude:
            continue

        tools.append(tool_def.to_openai_schema())

    return tools


def get_tool_by_name(name: str) -> TodoToolDefinition | None:
    """
    根据名称获取工具定义

    Args:
        name: 工具名称

    Returns:
        工具定义，如果不存在则返回None
    """
    for tool_def in TODO_TOOL_DEFINITIONS:
        if tool_def.name == name:
            return tool_def
    return None


__all__ = [
    "TODO_TOOLS",
    "TODO_TOOL_DEFINITIONS",
    "TodoToolDefinition",
    "TodoToolName",
    "get_todo_tools",
    "get_tool_by_name",
]
