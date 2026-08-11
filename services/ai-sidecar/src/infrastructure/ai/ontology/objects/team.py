"""
Team (班组) 对象类型定义
"""

from .base import ObjectType, create_action, create_property, create_relationship

TEAM_STATUS_VALUES = ["on_duty", "off_duty", "break", "meeting", "off"]
TEAM_ROLE_VALUES = ["handler", "supervisor", "coordinator", "specialist"]

TEAM_OBJECT = ObjectType(
    name="Team",
    plural_name="Teams",
    description="执行航班保障任务的人员班组",
    properties=[
        create_property("team_id", "string", required=True, description="班组唯一标识"),
        create_property("team_name", "string", required=True, description="班组名称，如甲班/乙班"),
        create_property("team_type", "string", description="班组类型: ground/pushback/baggage/cleaning"),
        create_property("status", "enum", enum_values=TEAM_STATUS_VALUES, description="班组状态"),
        create_property("location", "string", description="当前位置"),
        create_property("shift_start", "datetime", description="班次开始时间"),
        create_property("shift_end", "datetime", description="班次结束时间"),
        create_property("member_count", "integer", default=0, description="成员数量"),
        create_property("max_capacity", "integer", default=10, description="最大容量"),
        create_property("skill_level", "string", description="技能等级: junior/senior/expert"),
        create_property("certifications", "string", description="资质认证，多个用逗号分隔"),
    ],
    relationships=[
        create_relationship("members", "User", cardinality="many", description="班组内的成员", inverse="team"),
        create_relationship(
            "assigned_flights", "Flight", cardinality="many", description="分配的航班", inverse="assigned_team"
        ),
        create_relationship("equipment", "Equipment", cardinality="many", description="分配的设备"),
    ],
    actions=["assign_flight", "unassign_flight", "update_status", "start_break", "end_break", "change_location"],
    tags=["resource", "operation"],
)

TEAM_ACTIONS = [
    create_action(
        name="assign_flight",
        object_type="Team",
        description="为班组分配航班保障任务",
        parameters=[
            create_property("team_id", "string", required=True),
            create_property("flight_id", "string", required=True, description="航班ID"),
            create_property("task_type", "string", description="任务类型"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="unassign_flight",
        object_type="Team",
        description="取消班组的航班分配",
        parameters=[
            create_property("team_id", "string", required=True),
            create_property("flight_id", "string", required=True, description="航班ID"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="update_status",
        object_type="Team",
        description="更新班组状态",
        parameters=[
            create_property("team_id", "string", required=True),
            create_property("status", "string", required=True, enum_values=TEAM_STATUS_VALUES, description="新状态"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="start_break",
        object_type="Team",
        description="开始休息",
        parameters=[
            create_property("team_id", "string", required=True),
            create_property("break_duration", "integer", description="休息时长（分钟）"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="end_break",
        object_type="Team",
        description="结束休息",
        parameters=[
            create_property("team_id", "string", required=True),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="change_location",
        object_type="Team",
        description="更新班组位置",
        parameters=[
            create_property("team_id", "string", required=True),
            create_property("new_location", "string", required=True, description="新位置"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
]


class Team:
    OBJECT = TEAM_OBJECT
    ACTIONS = TEAM_ACTIONS
