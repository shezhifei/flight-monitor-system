"""Flight leg model.

Represents one directional leg (inbound or outbound) attached to a physical flight.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Literal

from src.domain.models.mission_type_enum import MissionTypeEnum
from src.domain.models.route_station import RouteStation, normalize_route_stations

LegType = Literal["inbound", "outbound"]
FlightTypeCode = Literal["domestic", "intl", "region"]


@dataclass
class FlightLeg:
    leg_type: LegType
    flight_no: str
    flight_type: FlightTypeCode = "domestic"
    mission: int | None = None
    origin_stations: list[RouteStation] | None = None
    destination_stations: list[RouteStation] | None = None
    is_vip: bool = False
    stand_type: str | None = None
    scheduled_time: datetime | None = None
    labels: list[str] | None = None

    def __post_init__(self) -> None:
        if self.leg_type not in ("inbound", "outbound"):
            raise ValueError(f"invalid leg_type: {self.leg_type}")
        if self.flight_type not in ("domestic", "intl", "region"):
            raise ValueError(f"invalid flight_type: {self.flight_type}")
        self.flight_no = str(self.flight_no or "").strip().upper()
        if not self.flight_no:
            raise ValueError("flight_no is required for flight leg")
        normalized_mission = MissionTypeEnum.normalize_numeric_value(self.mission)
        if self.mission is not None and normalized_mission is None:
            raise ValueError(f"invalid mission: {self.mission}")
        self.mission = normalized_mission
        self.origin_stations = normalize_route_stations(self.origin_stations)
        self.destination_stations = normalize_route_stations(self.destination_stations)
