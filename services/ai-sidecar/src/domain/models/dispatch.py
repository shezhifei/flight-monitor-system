"""
派工系统领域模型
定义班组、设备、派工单等核心实体
"""

from dataclasses import dataclass, field
from datetime import date, datetime
from decimal import Decimal
from enum import StrEnum
from typing import Any, Optional

from src.domain.utils.time_utils import utc_now

# 枚举定义


class TeamStatus(StrEnum):
    """班组状态"""

    ON_DUTY = "on_duty"  # 在岗
    OFF_DUTY = "off_duty"  # 离岗
    BREAK = "break"  # 休息


class EquipmentStatus(StrEnum):
    """设备状态"""

    AVAILABLE = "available"  # 可用
    IN_USE = "in_use"  # 使用中
    MAINTENANCE = "maintenance"  # 维护中
    RETIRED = "retired"  # 已退役


class DispatchOrderStatus(StrEnum):
    """派工单状态"""

    PENDING = "pending"  # 待处理
    ASSIGNED = "assigned"  # 已分配
    IN_PROGRESS = "in_progress"  # 执行中
    COMPLETED = "completed"  # 已完成
    CANCELLED = "cancelled"  # 已取消


class AssigneeType(StrEnum):
    """分配单位类型"""

    TEAM = "team"  # 班组
    INDIVIDUAL = "individual"  # 个人


class DispatchType(StrEnum):
    """派工类型"""

    AUTO = "auto"  # 自动派工
    MANUAL = "manual"  # 手动派工


class DispatchLockLevel(StrEnum):
    """派工单锁定级别"""

    ACTIVE = "active"
    FROZEN = "frozen"
    MANUAL_LOCK = "manual_lock"
    OPTIMIZABLE = "optimizable"


class ScheduleSource(StrEnum):
    """资源可用性来源"""

    SHIFT_INSTANCE = "shift_instance"
    CURRENT_STATUS_FALLBACK = "current_status_fallback"


class DepartmentRuleStatus(StrEnum):
    """部门规则状态"""

    DRAFT = "draft"
    PUBLISHED = "published"
    ARCHIVED = "archived"


class DispatchPublicationState(StrEnum):
    """工单发布状态"""

    PREPUBLISHED = "prepublished"
    PUBLISHED = "published"
    CANCELLED = "cancelled"


class DispatchSourceType(StrEnum):
    """工单来源类型"""

    GENERATED = "generated"
    MANUAL = "manual"
    SYSTEM_ADJUSTED = "system_adjusted"


class LegScope(StrEnum):
    """航班腿范围"""

    INBOUND = "inbound"
    OUTBOUND = "outbound"
    NONE = "none"


class PublishTriggerMode(StrEnum):
    """正式发布触发模式"""

    TIME = "time"
    EVENT = "event"
    EITHER = "either"
    BOTH_REQUIRED = "both_required"


class TurnaroundConstraintMode(StrEnum):
    """过站关联约束模式"""

    SAME_PERSON = "same_person"
    SOFT_PREFER_SAME_PERSON = "soft_prefer_same_person"
    HANDOVER_REQUIRED = "handover_required"
    DISABLED = "disabled"


class QualificationGrantStatus(StrEnum):
    """人员资质状态"""

    ACTIVE = "active"
    EXPIRED = "expired"
    SUSPENDED = "suspended"


class MemberRole(StrEnum):
    """成员角色"""

    LEADER = "leader"  # 组长
    MEMBER = "member"  # 成员
    DRIVER = "driver"  # 司机


class AlertSeverity(StrEnum):
    """告警级别"""

    INFO = "info"
    WARNING = "warning"
    CRITICAL = "critical"


# 值对象


@dataclass(frozen=True)
class Position:
    """位置坐标"""

    lat: Decimal
    lng: Decimal

    def to_dict(self) -> dict[str, float]:
        return {"lat": float(self.lat), "lng": float(self.lng)}

    @classmethod
    def from_dict(cls, data: dict) -> Optional["Position"]:
        if data and "lat" in data and "lng" in data:
            return cls(lat=Decimal(str(data["lat"])), lng=Decimal(str(data["lng"])))
        return None


# 实体：科室


@dataclass
class Department:
    """科室/部门"""

    id: str
    name: str
    code: str | None = None
    description: str | None = None
    manager_id: str | None = None
    terminal: str | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None
    is_active: bool = True


# 实体：班组类型


@dataclass
class TeamType:
    """班组类型"""

    id: str
    name: str
    department_id: str | None = None
    code: str | None = None
    description: str | None = None
    color: str | None = None  # UI显示颜色
    is_driver_type: bool = False
    created_at: datetime | None = None
    updated_at: datetime | None = None
    is_active: bool = True

    # 关联：可保障的作业类型
    task_types: list[str] = field(default_factory=list)


# 实体：班组


@dataclass
class Team:
    """换班班组/值班归属单元"""

    id: str
    name: str
    team_type_id: str | None = None
    code: str | None = None
    leader_id: str | None = None
    terminal: str | None = None
    current_status: TeamStatus = TeamStatus.OFF_DUTY
    current_position_lat: Decimal | None = None
    current_position_lng: Decimal | None = None
    current_stand_id: str | None = None
    last_position_update: datetime | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None
    is_active: bool = True

    # 关联
    team_type: TeamType | None = None
    members: list["TeamMember"] = field(default_factory=list)

    @property
    def current_position(self) -> Position | None:
        if self.current_position_lat and self.current_position_lng:
            return Position(self.current_position_lat, self.current_position_lng)
        return None

    def set_position(self, lat: Decimal, lng: Decimal, stand_id: str | None = None):
        self.current_position_lat = lat
        self.current_position_lng = lng
        self.current_stand_id = stand_id
        self.last_position_update = utc_now()


@dataclass
class TeamMember:
    """换班班组成员归属"""

    id: str
    team_id: str
    user_id: str
    role: MemberRole = MemberRole.MEMBER
    can_drive: bool = False
    joined_at: datetime | None = None
    left_at: datetime | None = None
    is_active: bool = True

    # 关联的用户信息（可选加载）
    username: str | None = None
    user_display_name: str | None = None


@dataclass
class ShiftTemplate:
    """排班模板"""

    id: str
    name: str
    resource_type: str  # team, equipment, employee
    resource_id: str
    terminal: str | None = None
    start_time_local: str = "08:00"
    end_time_local: str = "16:00"
    weekdays: list[int] = field(default_factory=list)
    max_continuous_minutes: int | None = None
    min_rest_minutes: int | None = None
    enabled: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class ShiftInstance:
    """排班实例"""

    id: str
    template_id: str | None
    resource_type: str
    resource_id: str
    terminal: str | None = None
    start_time: datetime = field(default_factory=utc_now)
    end_time: datetime = field(default_factory=utc_now)
    status: str = "scheduled"
    max_continuous_minutes: int | None = None
    min_rest_minutes: int | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class StaffCalendar:
    """人员班历聚合视图"""

    user_id: str
    team_id: str | None = None
    shift_instances: list[ShiftInstance] = field(default_factory=list)


@dataclass
class DepartmentQualificationCatalog:
    """部门资质目录定义"""

    id: str
    department_id: str
    qualification_code: str
    qualification_name: str
    description: str | None = None
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class DepartmentQualificationLevel:
    """部门资质等级及覆盖关系"""

    id: str
    department_id: str
    qualification_code: str
    level_code: str
    level_name: str
    level_rank: int = 0
    covered_level_codes: list[str] = field(default_factory=list)
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class QualificationGrant:
    """人员资质授予记录"""

    id: str
    user_id: str
    department_id: str
    qualification_code: str
    level_code: str
    valid_from: datetime | None = None
    valid_to: datetime | None = None
    status: QualificationGrantStatus = QualificationGrantStatus.ACTIVE
    source_team_id: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class TaskTypeCrewSlotRequirement:
    """作业类型资质槽位要求"""

    slot_code: str
    qualification_code: str
    min_level_code: str | None = None
    required_count: int = 1
    must_be_distinct: bool = True
    exclusive_group: str | None = None
    remarks: str | None = None


@dataclass
class TaskTypeEquipmentRequirement:
    """作业类型设备槽位要求"""

    slot_code: str
    equipment_type_id: str | None = None
    equipment_type_code: str | None = None
    required_count: int = 1
    must_be_distinct: bool = True
    requires_driver: bool = False
    driver_qualification_code: str | None = None
    driver_min_level_code: str | None = None
    remarks: str | None = None


@dataclass
class TurnaroundSlotPair:
    """过站关联的前后槽位配对"""

    inbound_slot_code: str
    outbound_slot_code: str


@dataclass
class TurnaroundContinuityRule:
    """作业类型级过站连续性规则"""

    enabled: bool = False
    counterpart_leg_scope: LegScope = LegScope.OUTBOUND
    counterpart_task_type: str = ""
    slot_pairs: list[TurnaroundSlotPair] = field(default_factory=list)
    constraint_mode: TurnaroundConstraintMode = TurnaroundConstraintMode.DISABLED
    tight_threshold_minutes: int | None = None
    relax_threshold_minutes: int | None = None
    flight_filters: dict[str, Any] = field(default_factory=dict)
    aircraft_type_filters: list[str] = field(default_factory=list)
    notes: str | None = None


@dataclass
class DepartmentTaskTypeRequirementVersion:
    """部门作业类型要求版本"""

    id: str
    department_id: str
    task_type: str
    version_no: int = 1
    status: DepartmentRuleStatus = DepartmentRuleStatus.DRAFT
    crew_requirements: list[TaskTypeCrewSlotRequirement] = field(default_factory=list)
    equipment_requirements: list[TaskTypeEquipmentRequirement] = field(default_factory=list)
    turnaround_continuity_rules: list[TurnaroundContinuityRule] = field(default_factory=list)
    notes: str | None = None
    published_at: datetime | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @property
    def requirements(self) -> list[TaskTypeCrewSlotRequirement]:
        return self.crew_requirements

    @requirements.setter
    def requirements(self, value: list[TaskTypeCrewSlotRequirement]) -> None:
        self.crew_requirements = list(value or [])


@dataclass
class FlightGenerationRule:
    """航班驱动基础生成规则"""

    id: str
    department_id: str
    task_type: str
    leg_scope: LegScope
    version_no: int = 1
    status: DepartmentRuleStatus = DepartmentRuleStatus.DRAFT
    rule_name: str | None = None
    conditions: dict[str, Any] = field(default_factory=dict)
    generation_anchor_type: str = "scheduled_time"
    start_offset_minutes: int = 0
    duration_minutes: int | None = None
    publication_state: DispatchPublicationState = DispatchPublicationState.PREPUBLISHED
    publish_trigger_mode: PublishTriggerMode = PublishTriggerMode.TIME
    publish_at: datetime | None = None
    publish_offset_minutes: int | None = None
    publish_event_code: str | None = None
    notes: str | None = None
    published_at: datetime | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class GenerationAdjustmentRule:
    """对已生成任务做 patch 的增量规则"""

    id: str
    department_id: str
    task_type: str
    version_no: int = 1
    status: DepartmentRuleStatus = DepartmentRuleStatus.DRAFT
    rule_name: str | None = None
    conditions: dict[str, Any] = field(default_factory=dict)
    actions: list[dict[str, Any]] = field(default_factory=list)
    notes: str | None = None
    published_at: datetime | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class TemporaryTaskTemplate:
    """临时任务模板，预定义手工加单使用的作业类型编码与资源需求。"""

    id: str
    department_id: str
    template_code: str
    template_name: str
    task_type: str
    crew_requirements: list[TaskTypeCrewSlotRequirement] = field(default_factory=list)
    equipment_requirements: list[TaskTypeEquipmentRequirement] = field(default_factory=list)
    notes: str | None = None
    is_active: bool = True
    created_at: datetime | None = None
    updated_at: datetime | None = None


@dataclass
class TaskCrewMember:
    """任务临时编组成员"""

    user_id: str
    username: str | None = None
    source_team_id: str | None = None
    source_team_name: str | None = None
    slot_code: str | None = None
    qualification_code: str | None = None
    qualification_level_code: str | None = None


@dataclass
class TaskCrew:
    """一次任务的临时执行编组"""

    members: list[TaskCrewMember] = field(default_factory=list)
    source_team_ids: list[str] = field(default_factory=list)
    source_team_names: list[str] = field(default_factory=list)
    generated_from: str = "auto"


@dataclass
class LeaveRecord:
    """请休假记录"""

    id: str
    user_id: str
    team_id: str | None = None
    start_time: datetime = field(default_factory=utc_now)
    end_time: datetime = field(default_factory=utc_now)
    reason: str | None = None
    status: str = "approved"
    created_at: datetime | None = None


@dataclass
class EquipmentDowntime:
    """设备停机窗口"""

    id: str
    equipment_id: str
    start_time: datetime = field(default_factory=utc_now)
    end_time: datetime = field(default_factory=utc_now)
    reason: str | None = None
    status: str = "scheduled"
    created_at: datetime | None = None


@dataclass
class DispatchLockRule:
    """派工锁定规则"""

    id: str
    dispatch_order_id: str | None = None
    flight_id: str | None = None
    team_id: str | None = None
    lock_level: DispatchLockLevel = DispatchLockLevel.OPTIMIZABLE
    start_time: datetime = field(default_factory=utc_now)
    end_time: datetime = field(default_factory=utc_now)
    reason: str | None = None
    created_at: datetime | None = None


# 实体：机位


@dataclass
class Stand:
    """机位/停机位"""

    id: str
    code: str
    name: str | None = None
    terminal: str | None = None
    area: str | None = None
    position_lat: Decimal = Decimal("0")
    position_lng: Decimal = Decimal("0")
    stand_type: str | None = None  # contact, remote
    size_category: str | None = None  # ICAO size
    is_active: bool = True
    created_at: datetime | None = None

    @property
    def position(self) -> Position:
        return Position(self.position_lat, self.position_lng)


# 实体：作业类型


@dataclass
class TaskType:
    """作业类型定义"""

    id: str
    code: str
    name: str
    default_department_id: str | None = None
    category: str | None = None  # arrival, departure, turnaround
    sequence_order: int | None = None
    default_duration_minutes: int | None = None
    trigger_offset_minutes: int = 30
    trigger_type: str = "before_eta"
    description: str | None = None
    is_active: bool = True
    created_at: datetime | None = None


# 实体：设备类型


@dataclass
class EquipmentType:
    """设备类型"""

    id: str
    name: str
    code: str | None = None
    category: str | None = None  # vehicle, loader, support
    requires_driver: bool = False
    driver_team_type_id: str | None = None
    icon: str | None = None
    description: str | None = None
    created_at: datetime | None = None
    is_active: bool = True

    # 关联：需要该设备的作业类型
    task_types: list[str] = field(default_factory=list)


# 实体：设备


@dataclass
class Equipment:
    """设备"""

    id: str
    code: str
    equipment_type_id: str | None = None
    name: str | None = None
    license_plate: str | None = None
    terminal: str | None = None
    status: EquipmentStatus = EquipmentStatus.AVAILABLE
    current_position_lat: Decimal | None = None
    current_position_lng: Decimal | None = None
    current_stand_id: str | None = None
    last_position_update: datetime | None = None
    current_dispatch_id: str | None = None
    last_maintenance_date: date | None = None
    next_maintenance_date: date | None = None
    metadata: dict[str, Any] | None = None
    created_at: datetime | None = None
    updated_at: datetime | None = None
    is_active: bool = True

    # 关联
    equipment_type: EquipmentType | None = None

    @property
    def current_position(self) -> Position | None:
        if self.current_position_lat and self.current_position_lng:
            return Position(self.current_position_lat, self.current_position_lng)
        return None

    def set_position(self, lat: Decimal, lng: Decimal, stand_id: str | None = None):
        self.current_position_lat = lat
        self.current_position_lng = lng
        self.current_stand_id = stand_id
        self.last_position_update = utc_now()


# 实体：派工单


@dataclass
class DispatchOrder:
    """派工单"""

    id: str
    flight_id: str
    task_type: str
    stand_id: str | None = None
    task_type_name: str | None = None
    stand_code: str | None = None
    terminal: str | None = None

    # 分配单位
    assignee_type: AssigneeType = AssigneeType.TEAM
    team_id: str | None = None
    team_name: str | None = None
    department: str | None = None
    individual_user_id: str | None = None
    individual_username: str | None = None

    # 司机资源
    driver_type: AssigneeType | None = None
    driver_team_id: str | None = None
    driver_user_id: str | None = None

    # 时间节点
    planned_start_time: datetime | None = None
    planned_end_time: datetime | None = None
    actual_start_time: datetime | None = None
    actual_end_time: datetime | None = None
    estimated_completion_time: datetime | None = None
    estimated_completion_reported_by: str | None = None
    estimated_completion_reported_at: datetime | None = None
    estimated_completion_note: str | None = None

    # 状态
    status: DispatchOrderStatus = DispatchOrderStatus.PENDING
    dispatch_type: DispatchType = DispatchType.AUTO
    dispatched_at: datetime | None = None
    dispatched_by: str | None = None

    # 快照
    snapshot_assignee_position: dict | None = None
    snapshot_equipment_positions: list[dict] | None = None
    estimated_arrival_minutes: int | None = None

    # 流程编排
    process_instance_id: str | None = None
    process_task_id: str | None = None
    workflow_context: dict[str, Any] = field(default_factory=dict)
    workflow_status: str = "pending_assignment"
    source: str = "system"
    schedule_source: ScheduleSource = ScheduleSource.CURRENT_STATUS_FALLBACK
    lock_level: DispatchLockLevel = DispatchLockLevel.OPTIMIZABLE
    publication_state: DispatchPublicationState = DispatchPublicationState.PUBLISHED
    source_type: DispatchSourceType = DispatchSourceType.MANUAL
    department_id: str | None = None
    leg_scope: LegScope = LegScope.NONE
    generation_rule_id: str | None = None
    generation_rule_version: int | None = None
    generation_anchor_type: str | None = None
    generation_anchor_time: datetime | None = None
    publish_trigger_mode: PublishTriggerMode | None = None
    publish_at: datetime | None = None
    turnaround_pair_key: str | None = None
    turnaround_constraint_mode: TurnaroundConstraintMode | None = None
    department_rule_version: str | None = None
    crew_requirement_snapshot: list[dict[str, Any]] = field(default_factory=list)
    equipment_requirement_snapshot: list[dict[str, Any]] = field(default_factory=list)
    task_crew: dict[str, Any] = field(default_factory=dict)
    equipment_assignment: list[dict[str, Any]] = field(default_factory=list)
    qualification_gap: list[dict[str, Any]] = field(default_factory=list)
    equipment_gap: list[dict[str, Any]] = field(default_factory=list)
    availability_reason: str | None = None
    score_breakdown: dict[str, Any] = field(default_factory=dict)
    conflict_reason: str | None = None
    recommended_assignees: list[dict[str, Any]] = field(default_factory=list)
    recommendation_score: float | None = None
    supervisor_notified: bool = False
    supervisor_notified_at: datetime | None = None
    assignment_deadline: datetime | None = None

    # 完成信息
    completed_by: str | None = None
    completion_notes: str | None = None
    gate: str | None = None

    created_at: datetime | None = None
    updated_at: datetime | None = None

    # 关联
    members: list["DispatchOrderMember"] = field(default_factory=list)
    equipment_list: list[Equipment] = field(default_factory=list)

    def can_be_started_by(self, user_id: str) -> bool:
        """验证用户是否可以开始此派工单"""
        if self.assignee_type == AssigneeType.INDIVIDUAL:
            if self.individual_user_id == user_id:
                return True
            return any(m.user_id == user_id and m.is_active for m in self.members)
        return any(m.user_id == user_id and m.is_active for m in self.members)

    def can_be_completed_by(self, user_id: str) -> bool:
        """验证用户是否可以完成此派工单"""
        return self.can_be_started_by(user_id)


@dataclass
class DispatchOrderMember:
    """派工单人员明细"""

    id: str
    dispatch_order_id: str
    user_id: str
    role: MemberRole = MemberRole.MEMBER
    source_type: AssigneeType = AssigneeType.TEAM
    source_team_id: str | None = None
    slot_code: str | None = None
    qualification_code: str | None = None
    qualification_level_code: str | None = None
    assigned_at: datetime | None = None
    check_in_time: datetime | None = None
    check_out_time: datetime | None = None
    is_active: bool = True

    # 关联的用户信息
    username: str | None = None


@dataclass
class DispatchOrderLog:
    """派工单操作日志"""

    id: str
    dispatch_order_id: str
    action: str
    actor_id: str | None = None
    details: dict | None = None
    created_at: datetime | None = None


# 实体：派工告警


@dataclass
class DispatchAlert:
    """派工告警"""

    id: str
    flight_id: str | None = None
    task_type: str | None = None
    alert_type: str = ""
    severity: AlertSeverity = AlertSeverity.WARNING
    message: str = ""
    is_resolved: bool = False
    resolved_at: datetime | None = None
    resolved_by: str | None = None
    resolution_notes: str | None = None
    notify_users: list[str] = field(default_factory=list)
    created_at: datetime | None = None
