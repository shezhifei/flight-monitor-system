"""Domain models for anomaly monitoring."""

from dataclasses import dataclass, field
from datetime import datetime
from enum import StrEnum
from typing import Any

from src.domain.utils.time_utils import utc_now


class AnomalyType(StrEnum):
    """Supported anomaly categories."""

    SERVICE_NODE_TIMEOUT = "service_node_timeout"
    GATE_STAND_CONFLICT = "gate_stand_conflict"
    KPI_DEGRADATION = "kpi_degradation"
    AI_RISK = "ai_risk"
    DISPATCH_ISSUE = "dispatch_issue"

    @classmethod
    def _missing_(cls, value: object) -> "AnomalyType | None":
        if value is None:
            return None
        normalized = str(value).strip().lower()
        legacy_aliases = {
            "turnaround_delay": cls.SERVICE_NODE_TIMEOUT.value,
            "service_timeout": cls.SERVICE_NODE_TIMEOUT.value,
            "gate_conflict": cls.GATE_STAND_CONFLICT.value,
            "replay_stream": cls.DISPATCH_ISSUE.value,
        }
        mapped = legacy_aliases.get(normalized, normalized)
        for item in cls:
            if item.value == mapped:
                return item
        return None


class AnomalySeverity(StrEnum):
    """Anomaly severity levels."""

    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


class AnomalyStatus(StrEnum):
    """Anomaly lifecycle states."""

    OPEN = "open"
    ACKNOWLEDGED = "acknowledged"
    RESOLVED = "resolved"


@dataclass
class Anomaly:
    """Anomaly entity persisted in anomalies table."""

    anomaly_id: str
    flight_id: str
    anomaly_type: AnomalyType
    severity: AnomalySeverity
    title: str
    description: str | None = None
    status: AnomalyStatus = AnomalyStatus.OPEN
    detected_at: datetime = field(default_factory=utc_now)
    resolved_at: datetime | None = None
    escalation_level: int = 0
    last_escalated_at: datetime | None = None
    linked_todo_id: str | None = None
    rule_id: str | None = None
    context_data: dict[str, Any] = field(default_factory=dict)
    created_at: datetime = field(default_factory=utc_now)
    updated_at: datetime = field(default_factory=utc_now)

    def acknowledge(self) -> None:
        """Mark anomaly as acknowledged."""
        if self.status == AnomalyStatus.RESOLVED:
            return
        self.status = AnomalyStatus.ACKNOWLEDGED
        self.updated_at = utc_now()

    def resolve(self) -> None:
        """Mark anomaly as resolved."""
        self.status = AnomalyStatus.RESOLVED
        self.resolved_at = utc_now()
        self.updated_at = utc_now()

    def escalate(self) -> None:
        """Increase escalation level and mark timestamp."""
        self.escalation_level += 1
        self.last_escalated_at = utc_now()
        self.updated_at = utc_now()
