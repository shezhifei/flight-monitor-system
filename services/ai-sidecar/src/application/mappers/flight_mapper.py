from __future__ import annotations

import datetime as dt_mod
from datetime import datetime
from typing import Any, ClassVar, cast

from src.domain.aggregates.flight import FlightAggregate
from src.domain.exceptions.validation import ValidationException
from src.domain.models.flight import Flight as DomainFlight
from src.domain.models.flight_leg import FlightLeg, LegType
from src.domain.models.route_station import normalize_route_stations
from src.domain.models.value_objects import (
    AircraftType,
    FlightId,
    FlightNumber,
    FlightStatus,
    GateNumber,
    StandNumber,
)


class FlightMapper:
    _FORBIDDEN_UPDATE_FIELDS: ClassVar[set[str]] = {"execution_date", "workspace_date", "aircraft_type_binary"}

    @staticmethod
    def _fmt_dt(dt: datetime | str | None) -> str | None:
        if not dt:
            return None
        if isinstance(dt, str):
            return dt
        if dt.tzinfo is None:
            from datetime import timedelta, timezone

            china_tz = timezone(timedelta(hours=8))
            dt = dt.replace(tzinfo=china_tz)
        from datetime import timezone

        return dt.astimezone(dt_mod.UTC).strftime("%Y-%m-%dT%H:%M:%SZ")

    @staticmethod
    def _serialize_leg(leg: FlightLeg | None) -> dict[str, Any] | None:
        if leg is None:
            return None
        return {
            "leg_type": leg.leg_type,
            "flight_no": leg.flight_no,
            "flight_type": leg.flight_type,
            "mission": leg.mission,
            "origin_stations": [station.to_dict() for station in leg.origin_stations or []],
            "destination_stations": [station.to_dict() for station in leg.destination_stations or []],
            "is_vip": bool(leg.is_vip),
            "stand_type": leg.stand_type,
            "scheduled_time": FlightMapper._fmt_dt(leg.scheduled_time),
        }

    @staticmethod
    def _parse_leg(raw: Any, expected_leg_type: str) -> FlightLeg | None:
        if raw is None:
            return None
        if isinstance(raw, FlightLeg):
            return raw if raw.leg_type == expected_leg_type else None
        if not isinstance(raw, dict):
            return None

        payload = dict(raw)
        leg_type = str(payload.get("leg_type") or "").strip().lower()
        if leg_type != expected_leg_type:
            return None
        flight_no = str(payload.get("flight_no") or "").strip()
        if not flight_no:
            return None

        scheduled_time = payload.get("scheduled_time")
        if isinstance(scheduled_time, str):
            try:
                scheduled_time = datetime.fromisoformat(scheduled_time.replace("Z", "+00:00"))
            except ValueError:
                scheduled_time = None

        return FlightLeg(
            leg_type=cast(LegType, leg_type),
            flight_no=flight_no,
            flight_type=str(payload.get("flight_type") or "domestic"),
            mission=payload.get("mission"),
            origin_stations=normalize_route_stations(payload.get("origin_stations")),
            destination_stations=normalize_route_stations(payload.get("destination_stations")),
            is_vip=bool(payload.get("is_vip", False)),
            stand_type=payload.get("stand_type"),
            scheduled_time=scheduled_time,
        )

    def to_dict(self, flight: DomainFlight | FlightAggregate, context: dict | None = None) -> dict[str, Any]:
        f = flight.get_flight() if hasattr(flight, "get_flight") else flight
        ctx = context or {}

        return {
            "flight_id": f.flight_id.value if f.flight_id else "",
            "flight_number": getattr(f.flight_number, "value", None),
            "airline_code": f.airline_code,
            "registration": f.registration,
            "aircraft_type_detail": getattr(f.aircraft_type_detail, "value", None) if f.aircraft_type_detail else None,
            "status": f.status.label if hasattr(f.status, "label") else f.status,
            "scheduled_departure": self._fmt_dt(f.scheduled_departure),
            "scheduled_arrival": self._fmt_dt(f.scheduled_arrival),
            "estimated_departure": self._fmt_dt(f.estimated_departure),
            "estimated_arrival": self._fmt_dt(f.estimated_arrival),
            "actual_departure": self._fmt_dt(f.actual_departure),
            "actual_arrival": self._fmt_dt(f.actual_arrival),
            "stand": getattr(f.stand, "value", None),
            "gate": getattr(f.gate, "value", None),
            "terminal": f.terminal,
            "position": f.position,
            "baggage_carousel": f.baggage_carousel,
            "has_boarding_restriction": f.has_boarding_restriction,
            "is_quick_turnaround": f.is_quick_turnaround,
            "is_commercial_signed": f.is_commercial_signed,
            "inbound_leg": self._serialize_leg(f.inbound_leg),
            "outbound_leg": self._serialize_leg(f.outbound_leg),
            "anomaly_summary": f.anomaly_summary
            or {
                "has_open_anomaly": False,
                "open_count": 0,
                "acknowledged_count": 0,
            },
            "created_at": self._fmt_dt(getattr(f, "created_at", None)),
            "updated_at": self._fmt_dt(getattr(f, "updated_at", None)),
            "version": getattr(f, "version", 1),
            "flight_remarks": f.flight_remarks,
            "load_planning_remarks": f.load_planning_remarks,
            "aircraft_maintenance_remarks": f.aircraft_maintenance_remarks,
            "aircraft_check_remarks": f.aircraft_check_remarks,
            "business_cases": ctx.get("business_cases") or getattr(flight, "business_cases", []),
        }

    def api_to_domain_create(self, flight_create: Any) -> dict[str, Any]:
        def g(k: str, d: Any = None) -> Any:
            if isinstance(flight_create, dict):
                return flight_create.get(k, d)
            return getattr(flight_create, k, d)

        def _normalize_leg(value: Any) -> Any:
            if value is None:
                return None
            if isinstance(value, dict):
                return value
            model_dump = getattr(value, "model_dump", None)
            if callable(model_dump):
                return model_dump(exclude_unset=True)
            dict_method = getattr(value, "dict", None)
            if callable(dict_method):
                return dict_method(exclude_unset=True)
            return value

        status_val = g("status")
        if hasattr(status_val, "value"):
            status_val = status_val.value

        return {
            "flight_number": g("flight_number"),
            "airline_code": g("airline_code"),
            "registration": g("registration"),
            "aircraft_type_detail": g("aircraft_type_detail"),
            "scheduled_departure": g("scheduled_departure"),
            "scheduled_arrival": g("scheduled_arrival"),
            "estimated_departure": g("estimated_departure"),
            "estimated_arrival": g("estimated_arrival"),
            "actual_departure": g("actual_departure"),
            "actual_arrival": g("actual_arrival"),
            "status": status_val,
            "gate": g("gate"),
            "stand": g("stand"),
            "terminal": g("terminal"),
            "position": g("position"),
            "baggage_carousel": g("baggage_carousel"),
            "has_boarding_restriction": g("has_boarding_restriction", False),
            "is_quick_turnaround": g("is_quick_turnaround", False),
            "is_commercial_signed": g("is_commercial_signed", True),
            "flight_remarks": g("flight_remarks"),
            "load_planning_remarks": g("load_planning_remarks"),
            "aircraft_maintenance_remarks": g("aircraft_maintenance_remarks"),
            "aircraft_check_remarks": g("aircraft_check_remarks"),
            "inbound_leg": _normalize_leg(g("inbound_leg")),
            "outbound_leg": _normalize_leg(g("outbound_leg")),
        }

    def api_to_domain_update(self, flight_update: Any) -> dict[str, Any]:
        if isinstance(flight_update, dict):
            provided_data = dict(flight_update)
        elif hasattr(flight_update, "model_dump"):
            provided_data = flight_update.model_dump(exclude_unset=True)
        elif hasattr(flight_update, "dict"):
            provided_data = flight_update.dict(exclude_unset=True)
        else:
            provided_data = {
                key: getattr(flight_update, key)
                for key in dir(flight_update)
                if not key.startswith("_") and hasattr(flight_update, key)
            }

        forbidden_fields = [key for key in provided_data if key in self._FORBIDDEN_UPDATE_FIELDS]
        if forbidden_fields:
            forbidden_str = ", ".join(sorted(forbidden_fields))
            raise ValidationException(f"字段不可写: {forbidden_str}", "flight_update_fields")

        domain_data: dict[str, Any] = {}
        for source_field, val in provided_data.items():
            if source_field in {
                "status",
                "gate",
                "terminal",
                "stand",
                "position",
                "baggage_carousel",
                "scheduled_departure",
                "scheduled_arrival",
                "estimated_departure",
                "estimated_arrival",
                "actual_departure",
                "actual_arrival",
                "aircraft_type_detail",
                "registration",
                "has_boarding_restriction",
                "is_quick_turnaround",
                "is_commercial_signed",
                "inbound_leg",
                "outbound_leg",
                "flight_remarks",
                "load_planning_remarks",
                "aircraft_maintenance_remarks",
                "aircraft_check_remarks",
            }:
                if source_field == "status" and hasattr(val, "value"):
                    domain_data[source_field] = val.value
                elif source_field in {"inbound_leg", "outbound_leg"} and hasattr(val, "model_dump"):
                    domain_data[source_field] = val.model_dump(exclude_unset=True)
                else:
                    domain_data[source_field] = val

        return domain_data

    def from_json(self, data: dict[str, Any]) -> DomainFlight:
        def parse_dt(value: Any) -> datetime | None:
            if not value:
                return None
            if isinstance(value, datetime):
                return value
            return dt_mod.datetime.fromisoformat(str(value).replace("Z", "+00:00"))

        flight = DomainFlight(
            flight_id=FlightId(data.get("flight_id", "")),
            flight_number=FlightNumber(data.get("flight_number")) if data.get("flight_number") else None,
            airline_code=data.get("airline_code"),
            registration=data.get("registration"),
            aircraft_type_detail=AircraftType(data["aircraft_type_detail"])
            if data.get("aircraft_type_detail")
            else None,
            status=FlightStatus.from_any(data.get("status")) or FlightStatus.SCHEDULED,
            scheduled_departure=parse_dt(data.get("scheduled_departure")),
            scheduled_arrival=parse_dt(data.get("scheduled_arrival")),
            estimated_departure=parse_dt(data.get("estimated_departure")),
            estimated_arrival=parse_dt(data.get("estimated_arrival")),
            actual_departure=parse_dt(data.get("actual_departure")),
            actual_arrival=parse_dt(data.get("actual_arrival")),
            stand=StandNumber(data.get("stand")) if data.get("stand") else None,
            gate=GateNumber(data.get("gate")) if data.get("gate") else None,
            terminal=data.get("terminal"),
            position=data.get("position"),
            baggage_carousel=data.get("baggage_carousel"),
            has_boarding_restriction=bool(data.get("has_boarding_restriction", False)),
            is_quick_turnaround=bool(data.get("is_quick_turnaround", False)),
            is_commercial_signed=bool(data.get("is_commercial_signed", True)),
            inbound_leg=self._parse_leg(data.get("inbound_leg"), "inbound"),
            outbound_leg=self._parse_leg(data.get("outbound_leg"), "outbound"),
            anomaly_summary=data.get("anomaly_summary") or {},
            flight_remarks=data.get("flight_remarks"),
            load_planning_remarks=data.get("load_planning_remarks"),
            aircraft_maintenance_remarks=data.get("aircraft_maintenance_remarks"),
            aircraft_check_remarks=data.get("aircraft_check_remarks"),
        )
        return flight

    def to_db_params(self, flight: DomainFlight) -> tuple:
        from datetime import datetime as dt_now

        status_code = getattr(flight.status, "code", None)
        if status_code is None:
            status_enum = FlightStatus.from_any(flight.status)
            status_code = status_enum.code if status_enum else FlightStatus.SCHEDULED.code

        return (
            flight.flight_id.value,
            flight.airline_code,
            flight.flight_number.value if flight.flight_number else None,
            flight.registration,
            flight.aircraft_type_detail.value if flight.aircraft_type_detail else None,
            status_code,
            flight.scheduled_departure,
            flight.scheduled_arrival,
            flight.estimated_departure,
            flight.estimated_arrival,
            flight.actual_departure,
            flight.actual_arrival,
            flight.gate.value if flight.gate else None,
            flight.stand.value if flight.stand else None,
            flight.terminal,
            flight.position,
            flight.baggage_carousel,
            flight.has_boarding_restriction,
            flight.is_quick_turnaround,
            flight.is_commercial_signed,
            dt_now.now(),
            flight.version,
            flight.flight_remarks,
            flight.load_planning_remarks,
            flight.aircraft_maintenance_remarks,
            flight.aircraft_check_remarks,
        )

    def from_db_row(self, row: dict[str, Any]) -> DomainFlight:
        return DomainFlight(
            flight_id=FlightId(row["flight_id"]),
            flight_number=FlightNumber(row["flight_number"]) if row.get("flight_number") else None,
            airline_code=row.get("airline_code"),
            registration=row.get("registration"),
            aircraft_type_detail=AircraftType(row["aircraft_type_detail"]) if row.get("aircraft_type_detail") else None,
            status=FlightStatus.from_code(row["status"])
            if isinstance(row.get("status"), int)
            else (FlightStatus.from_any(row.get("status")) or FlightStatus.SCHEDULED),
            scheduled_departure=row.get("scheduled_departure"),
            scheduled_arrival=row.get("scheduled_arrival"),
            estimated_departure=row.get("estimated_departure"),
            estimated_arrival=row.get("estimated_arrival"),
            actual_departure=row.get("actual_departure"),
            actual_arrival=row.get("actual_arrival"),
            gate=GateNumber(row["gate"]) if row.get("gate") else None,
            stand=StandNumber(row["stand"]) if row.get("stand") else None,
            terminal=row.get("terminal"),
            position=row.get("position"),
            baggage_carousel=row.get("baggage_carousel"),
            has_boarding_restriction=row.get("has_boarding_restriction", False),
            is_quick_turnaround=row.get("is_quick_turnaround", False),
            is_commercial_signed=row.get("is_commercial_signed", True),
            inbound_leg=self._parse_leg(row.get("inbound_leg"), "inbound"),
            outbound_leg=self._parse_leg(row.get("outbound_leg"), "outbound"),
            anomaly_summary=row.get("anomaly_summary") or {},
            version=row.get("version", 1),
            flight_remarks=row.get("flight_remarks"),
            load_planning_remarks=row.get("load_planning_remarks"),
            aircraft_maintenance_remarks=row.get("aircraft_maintenance_remarks"),
            aircraft_check_remarks=row.get("aircraft_check_remarks"),
            created_at=row.get("created_at") or datetime.now(),
            updated_at=row.get("updated_at") or datetime.now(),
        )


mapper = FlightMapper()
