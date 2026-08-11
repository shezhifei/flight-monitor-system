"""Flight aggregate root.

Strong-cut version: state transitions emit v2 change types and leg updates are
first-class events.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

from src.domain.policies.flight_modification_policy import (
    CommercialSigningPolicy,
    CompositeFlightModificationPolicy,
    FlightModificationPolicy,
)

from ..models.business_cases import FlightBusinessCase
from ..models.flight import Flight, FlightStatus
from ..models.flight_leg import FlightLeg
from ..models.state_changes import (
    FlightCreatedChange,
    FlightFieldUpdatedChange,
    FlightLegUpsertedChange,
    FlightRemarksUpdatedChange,
    FlightStateChange,
    FlightStatusUpdatedChange,
)
from .flight_components import FlightAggregateConverter, FlightAggregateValidator, FlightStateApplier

logger = logging.getLogger(__name__)


@dataclass
class FlightAggregate:
    """Flight aggregate root implementation."""

    flight: Flight
    _uncommitted_changes: list[FlightStateChange] = field(default_factory=list)
    _uncommitted_cases: list[FlightBusinessCase] = field(default_factory=list)
    _validator: FlightAggregateValidator = field(init=False, repr=False)
    _converter: FlightAggregateConverter = field(init=False, repr=False)
    _state_applier: FlightStateApplier = field(init=False, repr=False)
    _modification_policy: FlightModificationPolicy = field(init=False, repr=False)

    def __post_init__(self) -> None:
        self._validator = FlightAggregateValidator()
        self._converter = FlightAggregateConverter()
        self._state_applier = FlightStateApplier(self._converter)
        self._modification_policy = CompositeFlightModificationPolicy(
            policies=[CommercialSigningPolicy()],
        )

    def get_flight(self) -> Flight:
        return self.flight

    def get_uncommitted_changes(self) -> list[FlightStateChange]:
        return list(self._uncommitted_changes)

    def get_uncommitted_cases(self) -> list[FlightBusinessCase]:
        return list(self._uncommitted_cases)

    def clear_uncommitted(self) -> None:
        self._uncommitted_changes.clear()
        self._uncommitted_cases.clear()

    def apply_change(self, change: FlightStateChange) -> None:
        self._state_applier.apply_change(self.flight, change)
        self.flight.version += 1

    def _record_change(self, change: FlightStateChange) -> None:
        self._modification_policy.assert_change_allowed(self.flight, change)
        self._validator.validate_change_allowed(self.flight, change)
        self.apply_change(change)
        self._uncommitted_changes.append(change)

    def update_status(self, new_status: FlightStatus, updated_by: str = "System") -> None:
        _ = updated_by
        self._validator.validate_status_transition(self.flight.status, new_status)
        self._record_change(
            FlightStatusUpdatedChange(
                new_status=new_status.value,
                old_status=self.flight.status.value,
            )
        )

    def assign_stand(self, stand_number: str, assigned_by: str = "System") -> None:
        _ = assigned_by
        self._validator.validate_stand_number(stand_number)
        self._record_change(FlightFieldUpdatedChange(field_name="stand", new_value=stand_number))

    def assign_gate(self, gate_number: str, assigned_by: str = "System") -> None:
        _ = assigned_by
        self._validator.validate_gate_number(gate_number)
        self._record_change(FlightFieldUpdatedChange(field_name="gate", new_value=gate_number))

    def update_scheduled_times(
        self,
        departure: datetime | None = None,
        arrival: datetime | None = None,
        *,
        has_departure: bool = True,
        has_arrival: bool = True,
    ) -> None:
        if has_departure:
            self._record_change(FlightFieldUpdatedChange(field_name="scheduled_departure", new_value=departure))
        if has_arrival:
            self._record_change(FlightFieldUpdatedChange(field_name="scheduled_arrival", new_value=arrival))

    def update_estimated_times(
        self,
        departure: datetime | None = None,
        arrival: datetime | None = None,
        *,
        has_departure: bool = True,
        has_arrival: bool = True,
    ) -> None:
        if has_departure:
            self._record_change(FlightFieldUpdatedChange(field_name="estimated_departure", new_value=departure))
        if has_arrival:
            self._record_change(FlightFieldUpdatedChange(field_name="estimated_arrival", new_value=arrival))

    def update_actual_times(
        self,
        departure: datetime | None = None,
        arrival: datetime | None = None,
        *,
        has_departure: bool = True,
        has_arrival: bool = True,
    ) -> None:
        if has_departure:
            self._record_change(FlightFieldUpdatedChange(field_name="actual_departure", new_value=departure))
        if has_arrival:
            self._record_change(FlightFieldUpdatedChange(field_name="actual_arrival", new_value=arrival))

    def _update_field_if_changed(self, field_name: str, value: Any, updated_by: str = "System") -> None:
        _ = updated_by
        converted = value
        if value is not None:
            try:
                converted = self._converter.convert_field_value(field_name, value)
            except Exception as exc:  # noqa: BLE001 - field conversion may fail in various ways
                logger.warning("field value conversion failed (%s); using raw value: %s", field_name, exc)
                converted = value

        if not hasattr(self.flight, field_name):
            self._record_change(FlightFieldUpdatedChange(field_name=field_name, new_value=value))
            return

        if getattr(self.flight, field_name) == converted:
            return

        self._record_change(FlightFieldUpdatedChange(field_name=field_name, new_value=converted))

    def update_terminal(self, terminal: str | None, updated_by: str = "System") -> None:
        self._update_field_if_changed("terminal", terminal, updated_by=updated_by)

    def update_position(self, position: str | None, updated_by: str = "System") -> None:
        self._update_field_if_changed("position", position, updated_by=updated_by)

    def update_registration(self, registration: str | None, updated_by: str = "System") -> None:
        self._update_field_if_changed("registration", registration, updated_by=updated_by)

    def update_aircraft_type_detail(self, aircraft_type_detail: str | None, updated_by: str = "System") -> None:
        self._update_field_if_changed("aircraft_type_detail", aircraft_type_detail, updated_by=updated_by)

    def update_baggage_carousel(self, baggage_carousel: str | None, updated_by: str = "System") -> None:
        self._update_field_if_changed("baggage_carousel", baggage_carousel, updated_by=updated_by)

    def update_has_boarding_restriction(self, has_restriction: bool, updated_by: str = "System") -> None:
        self._update_field_if_changed("has_boarding_restriction", has_restriction, updated_by=updated_by)

    def update_is_quick_turnaround(self, is_quick_turnaround: bool, updated_by: str = "System") -> None:
        self._update_field_if_changed("is_quick_turnaround", is_quick_turnaround, updated_by=updated_by)

    def update_is_commercial_signed(self, is_commercial_signed: bool, updated_by: str = "System") -> None:
        self._update_field_if_changed("is_commercial_signed", is_commercial_signed, updated_by=updated_by)

    def upsert_leg(self, leg_type: str, leg_payload: dict[str, Any], updated_by: str = "System") -> None:
        _ = updated_by
        normalized_type = str(leg_type or "").strip().lower()
        if normalized_type not in {"inbound", "outbound"}:
            raise ValueError(f"invalid leg_type: {leg_type}")
        payload = dict(leg_payload or {})
        payload["leg_type"] = normalized_type
        self._record_change(
            FlightLegUpsertedChange(
                leg_type=normalized_type,
                leg_payload=payload,
            )
        )

    def update_flight_remarks(self, remarks: str, updated_by: str = "System") -> None:
        _ = updated_by
        self._record_change(FlightRemarksUpdatedChange(field_name="flight_remarks", new_value=remarks))

    def update_load_planning_remarks(self, remarks: str, updated_by: str = "System") -> None:
        _ = updated_by
        self._record_change(FlightRemarksUpdatedChange(field_name="load_planning_remarks", new_value=remarks))

    def update_aircraft_maintenance_remarks(self, remarks: str, updated_by: str = "System") -> None:
        _ = updated_by
        self._record_change(FlightRemarksUpdatedChange(field_name="aircraft_maintenance_remarks", new_value=remarks))

    def update_aircraft_check_remarks(self, remarks: str, updated_by: str = "System") -> None:
        _ = updated_by
        self._record_change(FlightRemarksUpdatedChange(field_name="aircraft_check_remarks", new_value=remarks))

    def _resolve_flight_no(self) -> str:
        for leg in (self.flight.outbound_leg, self.flight.inbound_leg):
            if leg is None:
                continue
            leg_no = getattr(leg, "flight_no", None)
            if leg_no:
                return str(leg_no)

        for candidate in (self.flight.flight_number,):
            if candidate is None:
                continue
            return getattr(candidate, "value", str(candidate))
        return self.flight.flight_id.value

    def record_takeoff(self, takeoff_time: datetime) -> None:
        self.update_actual_times(departure=takeoff_time, has_departure=True, has_arrival=False)
        self.update_status(FlightStatus.DEPARTED)

        flight_no = self._resolve_flight_no()
        case = FlightBusinessCase(
            case_type="flight_takeoff_alert",
            flight_id=self.flight.flight_id.value,
            flight_no=flight_no,
            description=f"航班 {flight_no} 已起飞",
            context={"takeoff_time": takeoff_time.isoformat()},
        )
        self._uncommitted_cases.append(case)

    def record_landing(self, landing_time: datetime) -> None:
        self.update_actual_times(arrival=landing_time, has_departure=False, has_arrival=True)
        self.update_status(FlightStatus.ARRIVED)

    def trigger_baggage_check(self, gate: str, passenger_count: int) -> None:
        flight_no = self._resolve_flight_no()
        case = FlightBusinessCase(
            case_type="gate_baggage_check",
            flight_id=self.flight.flight_id.value,
            flight_no=flight_no,
            description=f"登机口 {gate} 需要开包检查，涉及 {passenger_count} 名旅客",
            context={"gate": gate, "passenger_count": passenger_count},
        )
        self._uncommitted_cases.append(case)

    @staticmethod
    def _serialize_leg(leg: FlightLeg) -> dict[str, Any]:
        return {
            "leg_type": leg.leg_type,
            "flight_no": leg.flight_no,
            "flight_type": leg.flight_type,
            "mission": leg.mission,
            "origin_stations": [station.to_dict() for station in leg.origin_stations or []],
            "destination_stations": [station.to_dict() for station in leg.destination_stations or []],
            "is_vip": leg.is_vip,
            "stand_type": leg.stand_type,
            "scheduled_time": leg.scheduled_time,
        }

    @classmethod
    def create(cls, flight: Flight) -> FlightAggregate:
        aggregate = cls(flight=flight)

        data = {
            "flight_id": flight.flight_id.value,
            "flight_number": getattr(flight.flight_number, "value", None),
            "status": flight.status.name if flight.status else "SCHEDULED",
            "airline_code": flight.airline_code,
            "registration": flight.registration,
            "aircraft_type_detail": getattr(flight.aircraft_type_detail, "value", None),
            "stand": getattr(flight.stand, "value", None),
            "gate": getattr(flight.gate, "value", None),
            "terminal": flight.terminal,
            "position": flight.position,
            "baggage_carousel": flight.baggage_carousel,
            "scheduled_departure": flight.scheduled_departure,
            "scheduled_arrival": flight.scheduled_arrival,
            "estimated_departure": flight.estimated_departure,
            "estimated_arrival": flight.estimated_arrival,
            "actual_departure": flight.actual_departure,
            "actual_arrival": flight.actual_arrival,
            "has_boarding_restriction": flight.has_boarding_restriction,
            "is_quick_turnaround": flight.is_quick_turnaround,
            "is_commercial_signed": flight.is_commercial_signed,
            "flight_remarks": flight.flight_remarks,
            "load_planning_remarks": flight.load_planning_remarks,
            "aircraft_maintenance_remarks": flight.aircraft_maintenance_remarks,
            "aircraft_check_remarks": flight.aircraft_check_remarks,
            "anomaly_summary": flight.anomaly_summary,
            "inbound_leg": cls._serialize_leg(flight.inbound_leg) if flight.inbound_leg else None,
            "outbound_leg": cls._serialize_leg(flight.outbound_leg) if flight.outbound_leg else None,
        }

        full_data = {k: v for k, v in data.items() if v is not None}
        aggregate._record_change(FlightCreatedChange(data=full_data))
        return aggregate
