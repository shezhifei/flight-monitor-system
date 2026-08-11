"""
Equipment (设备) 对象类型定义
"""

from .base import ObjectType, create_action, create_property, create_relationship

EQUIPMENT_STATUS_VALUES = ["available", "in_use", "maintenance", "retired"]
EQUIPMENT_TYPE_VALUES = [
    "pushback_tractor",
    "baggage_cart",
    "boarding_bridge",
    "cargo_loader",
    "catering_truck",
    "water_truck",
    "fuel_truck",
    "cleaning_truck",
    "gpu",
    "air_stairs",
]

EQUIPMENT_OBJECT = ObjectType(
    name="Equipment",
    plural_name="Equipment",
    description="地面保障设备，如加油车、摆渡车、拖车等",
    properties=[
        create_property("equipment_id", "string", required=True, description="设备唯一标识"),
        create_property("equipment_code", "string", required=True, description="设备编号，如EQ-001"),
        create_property(
            "equipment_type", "enum", required=True, enum_values=EQUIPMENT_TYPE_VALUES, description="设备类型"
        ),
        create_property("status", "enum", enum_values=EQUIPMENT_STATUS_VALUES, description="设备状态"),
        create_property("location", "string", description="当前位置"),
        create_property("operator_id", "string", description="操作员ID"),
        create_property("capacity", "float", description="容量（如载客量、载重）"),
        create_property("fuel_level", "integer", description="燃油百分比"),
        create_property("last_maintenance", "datetime", description="最近维护时间"),
        create_property("next_maintenance", "datetime", description="下次维护时间"),
        create_property("notes", "string", description="备注信息"),
    ],
    relationships=[
        create_relationship("assigned_team", "Team", cardinality="one", description="分配的班组", inverse="equipment"),
        create_relationship("current_flight", "Flight", cardinality="one", description="正在保障的航班"),
    ],
    actions=["assign", "release", "start_maintenance", "end_maintenance", "update_location"],
    tags=["resource", "infrastructure"],
)

EQUIPMENT_ACTIONS = [
    create_action(
        name="assign",
        object_type="Equipment",
        description="分配设备给班组或航班",
        parameters=[
            create_property("equipment_id", "string", required=True),
            create_property("assignee_type", "string", required=True, description="分配对象类型: team/flight"),
            create_property("assignee_id", "string", required=True, description="分配对象ID"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="release",
        object_type="Equipment",
        description="释放设备",
        parameters=[
            create_property("equipment_id", "string", required=True),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="start_maintenance",
        object_type="Equipment",
        description="开始设备维护",
        parameters=[
            create_property("equipment_id", "string", required=True),
            create_property("reason", "string", description="维护原因"),
            create_property("expected_duration", "integer", description="预计时长（小时）"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="end_maintenance",
        object_type="Equipment",
        description="结束设备维护",
        parameters=[
            create_property("equipment_id", "string", required=True),
            create_property("maintenance_notes", "string", description="维护记录"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="update_location",
        object_type="Equipment",
        description="更新设备位置",
        parameters=[
            create_property("equipment_id", "string", required=True),
            create_property("new_location", "string", required=True, description="新位置"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
]


class Equipment:
    OBJECT = EQUIPMENT_OBJECT
    ACTIONS = EQUIPMENT_ACTIONS
