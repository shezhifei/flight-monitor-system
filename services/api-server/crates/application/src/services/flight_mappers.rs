//! Pure Flight DTO <-> domain mapping (no repo / IO).

use fms_domain::error::DomainError;
use fms_domain::models::flight::Flight;
use fms_domain::models::flight_leg::{FlightLeg, FlightTypeCode, LegType};
use fms_domain::models::value_objects::{AircraftType, FlightId, FlightNumber, FlightStatus, GateNumber, StandNumber};
use fms_domain::ports::flight_repository::{FlightUpdatePatch, PatchField};

use crate::schemas::flight_schemas::{
    FlightAnomalySummary, FlightCreate, FlightLegPayload, FlightResponse, FlightUpdate, NullableUpdate,
    RouteStationPayload,
};

pub fn to_response(f: &Flight) -> FlightResponse {
    FlightResponse {
        flight_id: Some(f.flight_id.0.clone()),
        flight_number: f.flight_number.as_ref().map(|n| n.0.clone()),
        airline_code: f.airline_code.clone(),
        registration: f.registration.clone(),
        aircraft_type_detail: f.aircraft_type_detail.as_ref().map(|a| a.0.clone()),
        status: Some(f.status.to_string()),
        scheduled_departure: f.scheduled_departure,
        scheduled_arrival: f.scheduled_arrival,
        estimated_departure: f.estimated_departure,
        estimated_arrival: f.estimated_arrival,
        actual_departure: f.actual_departure,
        actual_arrival: f.actual_arrival,
        cobt_time: f.cobt_time,
        codt: f.codt,
        on_blocks_time: None,
        cabin_door_open_time: None,
        deboarding_complete_time: None,
        cleaning_start_time: None,
        cleaning_end_time: None,
        boarding_allowed_time: None,
        start_boarding_time: None,
        passenger_ready_time: None,
        end_boarding_time: None,
        cabin_door_close_time: None,
        cargo_door_close_time: None,
        loading_complete_time: None,
        off_blocks_time: None,
        stand: f.stand.as_ref().map(|s| s.0.clone()),
        gate: f.gate.as_ref().map(|g| g.0.clone()),
        terminal: f.terminal.clone(),
        position: f.position.clone(),
        baggage_carousel: f.baggage_carousel.clone(),
        has_boarding_restriction: f.has_boarding_restriction,
        is_quick_turnaround: f.is_quick_turnaround,
        is_commercial_signed: f.is_commercial_signed,
        inbound_leg: f.inbound_leg.as_ref().map(leg_to_payload),
        outbound_leg: f.outbound_leg.as_ref().map(leg_to_payload),
        anomaly_summary: anomaly_summary_from_map(&f.anomaly_summary),
        business_cases: Vec::new(),
        created_at: Some(f.created_at),
        updated_at: Some(f.updated_at),
        version: f.version,
        labels: f.labels.clone(),
        flight_remarks: f.flight_remarks.clone(),
        load_planning_remarks: f.load_planning_remarks.clone(),
        aircraft_maintenance_remarks: f.aircraft_maintenance_remarks.clone(),
        aircraft_check_remarks: f.aircraft_check_remarks.clone(),
        direction: f.direction.clone(),
        flight_kind: Some(f.flight_kind.clone()),
        is_draft: Some(f.is_draft),
        divert: Some(f.divert),
        created_by: None,
        updated_by: None,
        risk_score: None,
        risk_level: None,
        risk_reasons: None,
        next_primary_action: None,
        data_freshness: None,
    }
}

pub fn from_create(dto: FlightCreate) -> Result<Flight, DomainError> {
    let now = chrono::Utc::now();
    let inbound_leg = dto.inbound_leg.map(payload_to_leg);
    let outbound_leg = dto.outbound_leg.map(payload_to_leg);
    if inbound_leg.is_none() && outbound_leg.is_none() {
        return Err(DomainError::ValidationError(
            "航班创建至少需要 inbound_leg 或 outbound_leg".into(),
        ));
    }

    Ok(Flight {
        flight_id: FlightId(dto.flight_id.unwrap_or_else(|| ulid::Ulid::new().to_string())),
        flight_number: dto.flight_number.map(FlightNumber),
        airline_code: dto.airline_code,
        registration: dto.registration,
        aircraft_type_detail: dto.aircraft_type_detail.map(AircraftType),
        stand: dto.stand.map(StandNumber),
        gate: dto.gate.map(GateNumber),
        terminal: dto.terminal,
        position: dto.position,
        baggage_carousel: dto.baggage_carousel,
        scheduled_departure: dto.scheduled_departure,
        scheduled_arrival: dto.scheduled_arrival,
        estimated_departure: dto.estimated_departure,
        estimated_arrival: dto.estimated_arrival,
        actual_departure: dto.actual_departure,
        actual_arrival: dto.actual_arrival,
        cobt_time: None,
        codt: None,
        has_boarding_restriction: dto.has_boarding_restriction,
        is_quick_turnaround: dto.is_quick_turnaround,
        is_commercial_signed: dto.is_commercial_signed,
        status: match dto.status {
            Some(status) => parse_status(&status)?,
            None => FlightStatus::Scheduled,
        },
        inbound_leg,
        outbound_leg,
        anomaly_summary: std::collections::HashMap::new(),
        created_at: now,
        updated_at: now,
        version: 1,
        labels: vec![],
        flight_remarks: dto.flight_remarks,
        load_planning_remarks: dto.load_planning_remarks,
        aircraft_maintenance_remarks: dto.aircraft_maintenance_remarks,
        aircraft_check_remarks: dto.aircraft_check_remarks,
        direction: None,
        flight_kind: "passenger".to_string(),
        is_draft: false,
        divert: false,
    })
}

pub fn update_patch_from_dto(dto: FlightUpdate) -> Result<FlightUpdatePatch, DomainError> {
    Ok(FlightUpdatePatch {
        expected_version: dto.expected_version,
        status: dto.status.as_deref().map(parse_status).transpose()?,
        gate: patch_field_map(dto.gate, GateNumber),
        terminal: patch_field_identity(dto.terminal),
        stand: patch_field_map(dto.stand, StandNumber),
        position: patch_field_identity(dto.position),
        baggage_carousel: patch_field_identity(dto.baggage_carousel),
        scheduled_departure: patch_field_identity(dto.scheduled_departure),
        scheduled_arrival: patch_field_identity(dto.scheduled_arrival),
        estimated_departure: patch_field_identity(dto.estimated_departure),
        estimated_arrival: patch_field_identity(dto.estimated_arrival),
        actual_departure: patch_field_identity(dto.actual_departure),
        actual_arrival: patch_field_identity(dto.actual_arrival),
        cobt_time: patch_field_identity(dto.cobt_time),
        aircraft_type_detail: patch_field_map(dto.aircraft_type_detail, AircraftType),
        registration: patch_field_identity(dto.registration),
        has_boarding_restriction: dto.has_boarding_restriction,
        is_quick_turnaround: dto.is_quick_turnaround,
        is_commercial_signed: dto.is_commercial_signed,
        inbound_leg: patch_field_map(dto.inbound_leg, payload_to_leg),
        outbound_leg: patch_field_map(dto.outbound_leg, payload_to_leg),
        flight_remarks: patch_field_identity(dto.flight_remarks),
        load_planning_remarks: patch_field_identity(dto.load_planning_remarks),
        aircraft_maintenance_remarks: patch_field_identity(dto.aircraft_maintenance_remarks),
        aircraft_check_remarks: patch_field_identity(dto.aircraft_check_remarks),
        is_draft: dto.is_draft,
        divert: dto.divert,
        flight_kind: patch_field_identity(dto.flight_kind),
        direction: patch_field_identity(dto.direction),
    })
}

pub fn patch_field_identity<T>(value: NullableUpdate<T>) -> PatchField<T> {
    patch_field_map(value, |value| value)
}

pub fn patch_field_map<T, U, F>(value: NullableUpdate<T>, convert: F) -> PatchField<U>
where
    F: FnOnce(T) -> U,
{
    match value {
        NullableUpdate::Unset => PatchField::Unset,
        NullableUpdate::Clear => PatchField::Clear,
        NullableUpdate::Set(value) => PatchField::Set(convert(value)),
    }
}

pub fn nullable_update_value<T>(value: NullableUpdate<&T>) -> Option<&T> {
    match value {
        NullableUpdate::Set(value) => Some(value),
        NullableUpdate::Unset | NullableUpdate::Clear => None,
    }
}

pub fn parse_status(value: &str) -> Result<FlightStatus, DomainError> {
    FlightStatus::from_str_loose(value)
        .ok_or_else(|| DomainError::ValidationError(format!("无效的航班状态: {}", value.trim())))
}

pub fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub fn anomaly_summary_from_map(
    summary: &std::collections::HashMap<String, serde_json::Value>,
) -> FlightAnomalySummary {
    let has_open_anomaly = summary
        .get("has_open_anomaly")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let open_count = summary
        .get("open_count")
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0);
    let acknowledged_count = summary
        .get("acknowledged_count")
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0);

    FlightAnomalySummary {
        has_open_anomaly,
        open_count,
        acknowledged_count,
    }
}

pub fn leg_to_payload(leg: &FlightLeg) -> FlightLegPayload {
    let origin_stations = station_payloads_from_parts(&leg.origin_code, &leg.origin_name);
    let destination_stations = station_payloads_from_parts(&leg.destination_code, &leg.destination_name);
    FlightLegPayload {
        leg_type: match leg.leg_type {
            LegType::Inbound => "inbound".to_string(),
            LegType::Outbound => "outbound".to_string(),
        },
        flight_no: leg.flight_no.clone(),
        flight_type: match leg.flight_type {
            FlightTypeCode::Domestic => "domestic".to_string(),
            FlightTypeCode::Intl => "intl".to_string(),
            FlightTypeCode::Region => "region".to_string(),
        },
        mission: leg.mission,
        origin_stations,
        destination_stations,
        origin_code: leg.origin_code.clone(),
        destination_code: leg.destination_code.clone(),
        origin_name: leg.origin_name.clone(),
        destination_name: leg.destination_name.clone(),
        is_vip: leg.is_vip,
        stand_type: leg.stand_type.clone(),
        scheduled_time: leg.scheduled_time,
    }
}

pub fn payload_to_leg(payload: FlightLegPayload) -> FlightLeg {
    let (origin_code, origin_name) =
        station_parts_from_payload(&payload.origin_stations, payload.origin_code, payload.origin_name);
    let (destination_code, destination_name) = station_parts_from_payload(
        &payload.destination_stations,
        payload.destination_code,
        payload.destination_name,
    );
    FlightLeg {
        leg_type: match payload.leg_type.trim().to_lowercase().as_str() {
            "inbound" => LegType::Inbound,
            _ => LegType::Outbound,
        },
        flight_no: payload.flight_no.trim().to_uppercase(),
        flight_type: match payload.flight_type.trim().to_lowercase().as_str() {
            "intl" => FlightTypeCode::Intl,
            "region" => FlightTypeCode::Region,
            _ => FlightTypeCode::Domestic,
        },
        mission: payload.mission,
        origin_code,
        destination_code,
        origin_name,
        destination_name,
        is_vip: payload.is_vip,
        stand_type: payload.stand_type,
        scheduled_time: payload.scheduled_time,
    }
}

pub fn station_payloads_from_parts(code: &Option<String>, name: &Option<String>) -> Vec<RouteStationPayload> {
    match normalize_station_pair(code.clone(), name.clone()) {
        Some((code, name)) => vec![RouteStationPayload { code, name }],
        None => Vec::new(),
    }
}

pub fn station_parts_from_payload(
    stations: &[RouteStationPayload],
    fallback_code: Option<String>,
    fallback_name: Option<String>,
) -> (Option<String>, Option<String>) {
    let primary_station = stations
        .iter()
        .find_map(|station| normalize_station_pair(Some(station.code.clone()), station.name.clone()));

    match primary_station {
        Some((code, name)) => (Some(code), name),
        None => normalize_station_pair(fallback_code, fallback_name)
            .map(|(code, name)| (Some(code), name))
            .unwrap_or((None, None)),
    }
}

pub fn normalize_station_pair(code: Option<String>, name: Option<String>) -> Option<(String, Option<String>)> {
    let normalized_code = code
        .map(|value| value.trim().to_uppercase())
        .filter(|value| !value.is_empty());
    let normalized_name = name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    normalized_code.map(|code| (code, normalized_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::ports::flight_repository::PatchField;

    #[test]
    fn update_patch_from_dto_preserves_clear_semantics() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "gate": null,
            "scheduled_departure": null,
            "registration": null,
            "inbound_leg": null,
            "terminal": "T2"
        }))
        .unwrap();

        let patch = update_patch_from_dto(dto).unwrap();

        assert!(matches!(patch.gate, PatchField::Clear));
        assert!(matches!(patch.scheduled_departure, PatchField::Clear));
        assert!(matches!(patch.registration, PatchField::Clear));
        assert!(matches!(patch.inbound_leg, PatchField::Clear));
        assert!(matches!(patch.terminal, PatchField::Set(ref value) if value == "T2"));
    }
}
