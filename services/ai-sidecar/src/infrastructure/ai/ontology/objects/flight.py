"""
Flight 对象类型定义
"""

from .base import ObjectType, create_action, create_property, create_relationship

FLIGHT_STATUS_VALUES = [
    "scheduled",
    "confirmed",
    "delayed",
    "boarding",
    "departed",
    "in_flight",
    "arrived",
    "cancelled",
    "diverted",
]

FLIGHT_OBJECT = ObjectType(
    name="Flight",
    plural_name="Flights",
    description="航班，包含进港和出港航班",
    properties=[
        create_property("flight_id", "string", required=True, description="航班唯一标识"),
        create_property("flight_number", "string", required=True, description="航班号，如CA1234"),
        create_property("flight_type", "string", description="flight_type: arrival/departure"),
        create_property("status", "enum", required=True, enum_values=FLIGHT_STATUS_VALUES, description="航班状态"),
        create_property("stand", "string", description="当前/目标机位"),
        create_property("gate", "string", description="登机口"),
        create_property("aircraft_type", "string", description="机型，如B737/A320"),
        create_property("registration", "string", description="飞机注册号"),
        create_property("origin", "string", description="始发地机场代码"),
        create_property("destination", "string", description="目的地机场代码"),
        create_property("scheduled_departure", "datetime", description="计划起飞时间"),
        create_property("scheduled_arrival", "datetime", description="计划到达时间"),
        create_property("actual_departure", "datetime", description="实际起飞时间"),
        create_property("actual_arrival", "datetime", description="实际到达时间"),
        create_property("estimated_departure", "datetime", description="预计起飞时间"),
        create_property("estimated_arrival", "datetime", description="预计到达时间"),
        create_property("delay_minutes", "integer", default=0, description="延误分钟数"),
        create_property("on_time_status", "string", description="准点状态: on_time/delayed/early"),
    ],
    relationships=[
        create_relationship(
            "assigned_team", "Team", cardinality="many", description="执行保障任务的班组", inverse="assigned_flights"
        ),
        create_relationship(
            "has_anomalies", "Anomaly", cardinality="many", description="关联的异常事件", inverse="related_flight"
        ),
        create_relationship(
            "dispatch_orders", "DispatchOrder", cardinality="many", description="派工单列表", inverse="flight"
        ),
        create_relationship("operating_at", "Stand", cardinality="one", description="停靠机位", inverse="flights"),
    ],
    actions=[
        "change_stand",
        "delay_flight",
        "notify_team",
        "assign_team",
        "update_status",
        "mark_arrived",
        "mark_departed",
    ],
    tags=["core", "operation"],
)

FLIGHT_ACTIONS = [
    create_action(
        name="change_stand",
        object_type="Flight",
        description="更改航班停靠机位",
        parameters=[
            create_property("flight_id", "string", required=True),
            create_property("new_stand", "string", required=True, description="新的机位编号"),
            create_property("reason", "string", description="变更原因"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="delay_flight",
        object_type="Flight",
        description="标记航班延误",
        parameters=[
            create_property("flight_id", "string", required=True),
            create_property("delay_minutes", "integer", required=True, description="延误分钟数"),
            create_property("reason", "string", description="延误原因"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="assign_team",
        object_type="Flight",
        description="为航班分配保障班组",
        parameters=[
            create_property("flight_id", "string", required=True),
            create_property("team_id", "string", required=True, description="班组ID"),
            create_property("role", "string", description="角色: handler/supervisor"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="update_status",
        object_type="Flight",
        description="更新航班状态",
        parameters=[
            create_property("flight_id", "string", required=True),
            create_property("status", "string", required=True, enum_values=FLIGHT_STATUS_VALUES, description="新状态"),
        ],
        requires_approval=True,
        risk_level="HIGH",
        category="mutation",
    ),
    create_action(
        name="mark_arrived",
        object_type="Flight",
        description="标记航班已到达",
        parameters=[
            create_property("flight_id", "string", required=True),
            create_property("actual_arrival", "datetime", description="实际到达时间"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="mark_departed",
        object_type="Flight",
        description="标记航班已起飞",
        parameters=[
            create_property("flight_id", "string", required=True),
            create_property("actual_departure", "datetime", description="实际起飞时间"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
]


class Flight:
    OBJECT = FLIGHT_OBJECT
    ACTIONS = FLIGHT_ACTIONS
