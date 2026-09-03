from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


@dataclass
class FlightStateChange:
    """Base class for flight state changes used in replay."""

    change_type: str
    occurred_at: datetime = field(default_factory=datetime.now)
    data: dict[str, Any] = field(default_factory=dict)


@dataclass
class FlightCreatedChange(FlightStateChange):
    """Strong-cut v2 seed event."""

    change_type: str = "created_v2"


@dataclass
class FlightStatusUpdatedChange(FlightStateChange):
    change_type: str = "status_updated_v2"
    new_status: str | None = None
    old_status: str | None = None


@dataclass
class GateAssignedChange(FlightStateChange):
    change_type: str = "resource_updated_v2"
    gate: str | None = None


@dataclass
class StandAssignedChange(FlightStateChange):
    change_type: str = "resource_updated_v2"
    stand: str | None = None


@dataclass
class TimeUpdatedChange(FlightStateChange):
    change_type: str = "resource_updated_v2"
    time_type: str | None = None  # scheduled, estimated, actual
    arrival: datetime | None = None
    departure: datetime | None = None


@dataclass
class FlightFieldUpdatedChange(FlightStateChange):
    """Generic resource/core field update in v2 stream."""

    change_type: str = "resource_updated_v2"
    field_name: str | None = None
    new_value: Any = None


@dataclass
class FlightLegUpsertedChange(FlightStateChange):
    change_type: str = "leg_upserted_v2"
    leg_type: str | None = None
    leg_payload: dict[str, Any] = field(default_factory=dict)


@dataclass
class FlightRemarksUpdatedChange(FlightStateChange):
    change_type: str = "remarks_updated_v2"
    field_name: str | None = None
    new_value: Any = None
