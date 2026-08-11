"""Core flight domain model after strong-cut refactor.

The Flight aggregate carries only core operational state. Direction-specific
attributes are represented by `FlightLeg` objects.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

from ..validation.flight_validator import FlightValidator
from .flight_leg import FlightLeg
from .route_station import RouteStation
from .value_objects import (
    AircraftType,
    FlightId,
    FlightNumber,
    FlightStatus,
    GateNumber,
    StandNumber,
)

logger = logging.getLogger(__name__)


@dataclass
class Flight:
    """Core Flight entity."""

    flight_id: FlightId

    # Core identity and resource state.
    airline_code: str | None = None
    flight_number: FlightNumber | None = None
    registration: str | None = None
    aircraft_type_detail: AircraftType | None = None
    stand: StandNumber | None = None
    gate: GateNumber | None = None
    terminal: str | None = None
    position: str | None = None
    baggage_carousel: str | None = None

    # Core time state.
    scheduled_departure: datetime | None = None
    scheduled_arrival: datetime | None = None
    estimated_departure: datetime | None = None
    estimated_arrival: datetime | None = None
    actual_departure: datetime | None = None
    actual_arrival: datetime | None = None
    cobt_time: datetime | None = None
    codt: datetime | None = None

    # Core policy flags.
    has_boarding_restriction: bool = False
    is_quick_turnaround: bool = False
    is_commercial_signed: bool = True

    # Labels (flight-level tags).
    labels: list[str] = field(default_factory=list)

    # Aggregate state.
    status: FlightStatus = FlightStatus.SCHEDULED
    inbound_leg: FlightLeg | None = None
    outbound_leg: FlightLeg | None = None
    anomaly_summary: dict[str, Any] = field(default_factory=dict)

    # Audit.
    created_at: datetime = field(default_factory=datetime.now)
    updated_at: datetime = field(default_factory=datetime.now)
    version: int = 0

    # Remarks.
    flight_remarks: str | None = None
    load_planning_remarks: str | None = None
    aircraft_maintenance_remarks: str | None = None
    aircraft_check_remarks: str | None = None

    def __post_init__(self) -> None:
        self._sync_primary_flight_number()
        self._validate_flight()

    def _sync_primary_flight_number(self) -> None:
        if self.flight_number is not None:
            return
        for leg in (self.outbound_leg, self.inbound_leg):
            if leg is None:
                continue
            flight_no = str(getattr(leg, "flight_no", "") or "").strip()
            if not flight_no:
                continue
            try:
                self.flight_number = FlightNumber(flight_no)
                return
            except Exception as exc:  # noqa: BLE001 - FlightNumber construction may fail in various ways
                logger.warning("FlightNumber construction failed; skipping leg: %s", exc)
                continue

    def _validate_flight(self) -> None:
        FlightValidator.validate_flight_entity(self)

    @property
    def current_status(self) -> FlightStatus:
        return self.status

    def _can_transition_to(self, new_status: FlightStatus) -> bool:
        return FlightValidator.can_transition(self.status, new_status)

    def upsert_leg(self, leg: FlightLeg) -> None:
        if leg.leg_type == "inbound":
            self.inbound_leg = leg
        else:
            self.outbound_leg = leg
        self._sync_primary_flight_number()

    def is_arrival_flight(self) -> bool:
        return self.inbound_leg is not None

    def is_departure_flight(self) -> bool:
        return self.outbound_leg is not None

    def is_turnaround_flight(self) -> bool:
        return self.inbound_leg is not None and self.outbound_leg is not None

    def get_destination_codes(self) -> list[str]:
        result: list[str] = []
        for leg in (self.inbound_leg, self.outbound_leg):
            if leg is None:
                continue
            for station in getattr(leg, "destination_stations", []) or []:
                code = str(getattr(station, "code", "") or "").strip()
                if code:
                    result.append(code)
        return result

    def get_origins_codes(self) -> list[str]:
        result: list[str] = []
        for leg in (self.inbound_leg, self.outbound_leg):
            if leg is None:
                continue
            for station in getattr(leg, "origin_stations", []) or []:
                code = str(getattr(station, "code", "") or "").strip()
                if code:
                    result.append(code)
        return result

    def get_origin_stations(self) -> list[RouteStation]:
        result: list[RouteStation] = []
        for leg in (self.inbound_leg, self.outbound_leg):
            if leg is None:
                continue
            result.extend(list(getattr(leg, "origin_stations", []) or []))
        return result

    def get_destination_stations(self) -> list[RouteStation]:
        result: list[RouteStation] = []
        for leg in (self.inbound_leg, self.outbound_leg):
            if leg is None:
                continue
            result.extend(list(getattr(leg, "destination_stations", []) or []))
        return result

    def get_flight_numbers(self) -> list[str]:
        numbers: list[str] = []
        for leg in (self.inbound_leg, self.outbound_leg):
            if leg is None:
                continue
            value = str(getattr(leg, "flight_no", "") or "").strip()
            if value:
                numbers.append(value)
        return numbers
