from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


@dataclass
class Department:
    id: str
    name: str
    code: str
    description: str = ""
    is_active: bool = True


@dataclass
class TeamType:
    id: str
    name: str
    description: str = ""


@dataclass
class Team:
    id: str
    name: str
    department_id: str
    team_type_id: str = ""
    member_count: int = 0
    is_active: bool = True
    members: list[dict[str, Any]] = field(default_factory=list)


@dataclass
class EquipmentType:
    id: str
    name: str
    description: str = ""


@dataclass
class Equipment:
    id: str
    name: str
    type: str
    equipment_type_id: str = ""
    status: str = "available"
    is_active: bool = True


@dataclass
class Stand:
    id: str
    code: str
    terminal: str = ""
    is_active: bool = True


@dataclass
class TaskType:
    id: str
    name: str
    category: str = ""
    description: str = ""


@dataclass
class DispatchOrder:
    id: str
    flight_id: str
    task_type: str
    status: str = "pending"
    department_id: str = ""
    team_id: str = ""
    scheduled_at: datetime | None = None
    completed_at: datetime | None = None
    members: list[dict[str, Any]] = field(default_factory=list)
