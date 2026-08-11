"""
Stand (机位) 对象类型定义
"""

from .base import ObjectType, create_action, create_property, create_relationship

STAND_STATUS_VALUES = ["available", "occupied", "maintenance", "reserved", "closed"]
STAND_SIZE_VALUES = ["small", "medium", "large", "xlarge"]

STAND_OBJECT = ObjectType(
    name="Stand",
    plural_name="Stands",
    description="停机位，包含机位编号、类型、状态等信息",
    properties=[
        create_property("stand_id", "string", required=True, description="机位唯一标识"),
        create_property("stand_code", "string", required=True, description="机位编号，如A1, B23"),
        create_property("terminal", "string", description="所属航站楼"),
        create_property("zone", "string", description="区域，如North/South"),
        create_property("status", "enum", enum_values=STAND_STATUS_VALUES, description="机位状态"),
        create_property("size", "enum", enum_values=STAND_SIZE_VALUES, description="机位大小"),
        create_property("max_wingspan", "float", description="最大翼展（米）"),
        create_property("max_weight", "float", description="最大承重（吨）"),
        create_property("has_bridge", "boolean", default=False, description="是否有廊桥"),
        create_property("has_baggage_belt", "boolean", default=False, description="是否有行李传送带"),
        create_property("priority", "integer", default=0, description="优先级（数字越大优先级越高）"),
    ],
    relationships=[
        create_relationship(
            "current_flight", "Flight", cardinality="one", description="当前停靠的航班", inverse="operating_at"
        ),
        create_relationship("scheduled_flights", "Flight", cardinality="many", description="计划停靠的航班"),
    ],
    actions=["occupy", "release", "reserve", "close", "update_status"],
    tags=["infrastructure", "operation"],
)

STAND_ACTIONS = [
    create_action(
        name="occupy",
        object_type="Stand",
        description="占用机位（航班停靠）",
        parameters=[
            create_property("stand_id", "string", required=True),
            create_property("flight_id", "string", required=True, description="航班ID"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="release",
        object_type="Stand",
        description="释放机位（航班离港）",
        parameters=[
            create_property("stand_id", "string", required=True),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="reserve",
        object_type="Stand",
        description="预留机位",
        parameters=[
            create_property("stand_id", "string", required=True),
            create_property("flight_id", "string", description="预留的航班ID"),
            create_property("start_time", "datetime", description="预留开始时间"),
            create_property("end_time", "datetime", description="预留结束时间"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="close",
        object_type="Stand",
        description="关闭机位（维护或临时关闭）",
        parameters=[
            create_property("stand_id", "string", required=True),
            create_property("reason", "string", description="关闭原因"),
            create_property("expected_reopen", "datetime", description="预计重新开放时间"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="update_status",
        object_type="Stand",
        description="更新机位状态",
        parameters=[
            create_property("stand_id", "string", required=True),
            create_property("status", "string", required=True, enum_values=STAND_STATUS_VALUES, description="新状态"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
]


class Stand:
    OBJECT = STAND_OBJECT
    ACTIONS = STAND_ACTIONS
