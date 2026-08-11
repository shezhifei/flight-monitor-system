"""
Anomaly (异常) 对象类型定义
"""

from .base import ObjectType, create_action, create_property, create_relationship

ANOMALY_TYPE_VALUES = [
    "gate_stand_conflict",
    "kpi_degradation",
    "dispatch_issue",
    "equipment_shortage",
    "crew_shortage",
    "weather_impact",
    "security_alert",
    "maintenance_alert",
    "capacity_warning",
]
ANOMALY_SEVERITY_VALUES = ["low", "medium", "high", "critical"]
ANOMALY_STATUS_VALUES = ["detected", "acknowledged", "investigating", "resolved", "escalated"]

ANOMALY_OBJECT = ObjectType(
    name="Anomaly",
    plural_name="Anomalies",
    description="系统检测到的异常事件或告警",
    properties=[
        create_property("anomaly_id", "string", required=True, description="异常唯一标识"),
        create_property("anomaly_type", "enum", required=True, enum_values=ANOMALY_TYPE_VALUES, description="异常类型"),
        create_property("severity", "enum", required=True, enum_values=ANOMALY_SEVERITY_VALUES, description="严重程度"),
        create_property("status", "enum", enum_values=ANOMALY_STATUS_VALUES, description="处理状态"),
        create_property("title", "string", required=True, description="异常标题"),
        create_property("description", "string", description="异常描述"),
        create_property("detected_at", "datetime", required=True, description="检测时间"),
        create_property("resolved_at", "datetime", description="解决时间"),
        create_property("resolved_by", "string", description="解决人"),
        create_property("resolution_notes", "string", description="解决备注"),
        create_property("flight_id", "string", description="关联航班ID"),
        create_property("stand_id", "string", description="关联机位ID"),
        create_property("team_id", "string", description="负责班组ID"),
        create_property("kpi_impact", "string", description="KPI影响"),
    ],
    relationships=[
        create_relationship(
            "related_flight", "Flight", cardinality="one", description="关联航班", inverse="has_anomalies"
        ),
        create_relationship("assigned_team", "Team", cardinality="one", description="负责班组"),
        create_relationship("stand", "Stand", cardinality="one", description="关联机位"),
    ],
    actions=["acknowledge", "assign_team", "resolve", "escalate", "add_note"],
    tags=["monitoring", "alert"],
)

ANOMALY_ACTIONS = [
    create_action(
        name="acknowledge",
        object_type="Anomaly",
        description="确认异常（开始处理）",
        parameters=[
            create_property("anomaly_id", "string", required=True),
            create_property("acknowledged_by", "string", description="确认人"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="assign_team",
        object_type="Anomaly",
        description="分配处理班组",
        parameters=[
            create_property("anomaly_id", "string", required=True),
            create_property("team_id", "string", required=True, description="班组ID"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
    create_action(
        name="resolve",
        object_type="Anomaly",
        description="解决异常",
        parameters=[
            create_property("anomaly_id", "string", required=True),
            create_property("resolution_notes", "string", description="解决备注"),
            create_property("resolved_by", "string", description="解决人"),
        ],
        requires_approval=True,
        risk_level="MEDIUM",
        category="mutation",
    ),
    create_action(
        name="escalate",
        object_type="Anomaly",
        description="升级异常",
        parameters=[
            create_property("anomaly_id", "string", required=True),
            create_property("escalation_reason", "string", required=True, description="升级原因"),
            create_property("escalate_to", "string", description="升级目标"),
        ],
        requires_approval=True,
        risk_level="HIGH",
        category="mutation",
    ),
    create_action(
        name="add_note",
        object_type="Anomaly",
        description="添加异常备注",
        parameters=[
            create_property("anomaly_id", "string", required=True),
            create_property("note", "string", required=True, description="备注内容"),
            create_property("added_by", "string", description="添加人"),
        ],
        requires_approval=False,
        risk_level="LOW",
        category="mutation",
    ),
]


class Anomaly:
    OBJECT = ANOMALY_OBJECT
    ACTIONS = ANOMALY_ACTIONS
