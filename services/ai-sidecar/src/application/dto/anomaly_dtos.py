from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


@dataclass
class Anomaly:
    id: str
    flight_id: str
    anomaly_type: str
    severity: str = "medium"
    status: str = "open"
    description: str = ""
    details: dict[str, Any] = field(default_factory=dict)
    detected_at: datetime | None = None
    resolved_at: datetime | None = None


@dataclass
class AnomalySummary:
    total: int = 0
    open: int = 0
    critical: int = 0
    flight_id: str | None = None
    warning: int = 0
    info: int = 0
    latest_anomaly_type: str | None = None


@dataclass
class AnomalyRule:
    id: str
    name: str
    enabled: bool = True
    description: str = ""
    rule_type: str = ""
    severity: str = "medium"
    config: dict[str, Any] = field(default_factory=dict)
