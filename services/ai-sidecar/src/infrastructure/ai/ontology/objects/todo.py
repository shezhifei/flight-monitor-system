"""
Todo (待办事项) 对象类型定义
"""

from .base import ObjectType, create_action, create_property, create_relationship

TODO_STATUS_VALUES = ["pending", "in_progress", "completed", "cancelled", "expired"]
TODO_PRIORITY_VALUES = ["low", "medium", "high", "urgent"]

TODO_OBJECT = ObjectType(
    name="Todo",
    plural_name="Todos",
    description="可追踪的任务项，支持优先级、分配、进度管理",
    properties=[
        create_property("todo_id", "string", required=True, description="待办唯一标识"),
        create_property("title", "string", required=True, description="待办标题"),
        create_property("description", "string", description="待办描述"),
        create_property("status", "enum", required=True, enum_values=TODO_STATUS_VALUES, description="待办状态"),
        create_property("priority", "enum", enum_values=TODO_PRIORITY_VALUES, description="优先级"),
        create_property("assignee_id", "string", description="分配给的用户ID"),
        create_property("assignee_name", "string", description="分配给的用户名"),
        create_property("created_by", "string", description="创建人"),
        create_property("created_at", "datetime", required=True, description="创建时间"),
        create_property("due_date", "datetime", description="截止日期"),
        create_property("completed_at", "datetime", description="完成时间"),
        create_property("flight_id", "string", description="关联航班ID"),
        create_property("anomaly_id", "string", description="关联异常ID"),
        create_property("parent_todo_id", "string", description="父待办ID（子任务）"),
        create_property("tags", "string", description="标签，多个用逗号分隔"),
    ],
    relationships=[
        create_relationship("flight", "Flight", cardinality="one", description="关联航班"),
        create_relationship("anomaly", "Anomaly", cardinality="one", description="关联异常"),
        create_relationship("assignee", "User", cardinality="one", description="负责人"),
        create_relationship("parent", "Todo", cardinality="one", description="父待办"),
        create_relationship("subtasks", "Todo", cardinality="many", description="子待办", inverse="parent"),
    ],
    actions=["create", "update", "assign", "complete", "cancel", "add_subtask"],
    tags=["task", "operation"],
)

TODO_ACTIONS = [
    create_action(
        name="create",
        object_type="Todo",
        description="创建待办事项",
        parameters=[
            create_property("title", "string", required=True, description="标题"),
            create_property("description", "string", description="描述"),
            create_property("priority", "string", enum_values=TODO_PRIORITY_VALUES, description="优先级"),
            create_property("assignee_id", "string", description="分配给的用户ID"),
            create_property("due_date", "datetime", description="截止日期"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="update",
        object_type="Todo",
        description="更新待办内容",
        parameters=[
            create_property("todo_id", "string", required=True),
            create_property("title", "string", description="新标题"),
            create_property("description", "string", description="新描述"),
            create_property("priority", "string", enum_values=TODO_PRIORITY_VALUES, description="新优先级"),
            create_property("due_date", "datetime", description="新截止日期"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="assign",
        object_type="Todo",
        description="分配待办给用户",
        parameters=[
            create_property("todo_id", "string", required=True),
            create_property("assignee_id", "string", required=True, description="用户ID"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="complete",
        object_type="Todo",
        description="标记待办为已完成",
        parameters=[
            create_property("todo_id", "string", required=True),
            create_property("completion_notes", "string", description="完成备注"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="cancel",
        object_type="Todo",
        description="取消待办",
        parameters=[
            create_property("todo_id", "string", required=True),
            create_property("cancel_reason", "string", description="取消原因"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="add_subtask",
        object_type="Todo",
        description="添加子待办",
        parameters=[
            create_property("todo_id", "string", required=True, description="父待办ID"),
            create_property("subtask_title", "string", required=True, description="子待办标题"),
            create_property("subtask_description", "string", description="子待办描述"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
]


class Todo:
    OBJECT = TODO_OBJECT
    ACTIONS = TODO_ACTIONS
