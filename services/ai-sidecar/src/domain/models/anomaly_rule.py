"""Domain model for anomaly detection rules."""

from dataclasses import dataclass, field
from typing import Any

from src.domain.models.anomaly import AnomalySeverity


@dataclass
class AnomalyRule:
    """Anomaly rule configuration loaded from anomaly_rules table."""

    rule_id: str
    rule_type: str
    name: str
    enabled: bool = True
    config: dict[str, Any] = field(default_factory=dict)
    severity: AnomalySeverity = AnomalySeverity.MEDIUM
    auto_create_todo: bool = True
    todo_priority: str = "HIGH"
    escalation_intervals: list[int] = field(default_factory=lambda: [5, 15, 30])

    def normalized_intervals(self) -> list[int]:
        """Return sorted, positive escalation intervals in minutes."""
        normalized = []
        for item in self.escalation_intervals:
            try:
                value = int(item)
            except (TypeError, ValueError):
                continue
            if value > 0:
                normalized.append(value)
        if not normalized:
            return [5, 15, 30]
        return sorted(set(normalized))
