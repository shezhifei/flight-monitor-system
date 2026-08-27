//! Flight write validation / authorization / state-transition guard (no IO beyond repo port).

use fms_domain::error::DomainError;
use fms_domain::models::flight_state::can_transition;
use fms_domain::ports::flight_repository::{FlightRepository, FlightUpdatePatch};

use crate::schemas::flight_schemas::{FlightCreate, FlightLegPayload, FlightUpdate};
use crate::services::flight_mappers::{normalize_optional_string, nullable_update_value};

pub fn denied_update_fields(dto: &FlightUpdate, is_admin: bool, permissions: &[String]) -> Vec<String> {
    if is_admin || permissions.iter().any(|value| value == "*") {
        return Vec::new();
    }

    let mut denied = Vec::new();
    for field in sync_locked_fields() {
        let touched = match *field {
            "registration" => dto.registration.is_touched(),
            "status" => dto.status.is_some(),
            "aircraft_type_detail" => dto.aircraft_type_detail.is_touched(),
            "inbound_leg" => dto.inbound_leg.is_touched(),
            "outbound_leg" => dto.outbound_leg.is_touched(),
            "position" => dto.position.is_touched(),
            "scheduled_arrival" => dto.scheduled_arrival.is_touched(),
            "scheduled_departure" => dto.scheduled_departure.is_touched(),
            "estimated_arrival" => dto.estimated_arrival.is_touched(),
            "estimated_departure" => dto.estimated_departure.is_touched(),
            "actual_arrival" => dto.actual_arrival.is_touched(),
            "actual_departure" => dto.actual_departure.is_touched(),
            "cobt_time" => dto.cobt_time.is_touched(),
            "is_draft" => dto.is_draft.is_some(),
            "divert" => dto.divert.is_some(),
            "flight_kind" => dto.flight_kind.is_touched(),
            "direction" => dto.direction.is_touched(),
            _ => false,
        };
        if touched {
            denied.push((*field).to_string());
        }
    }
    denied
}

pub async fn ensure_status_transition(
    repo: &(dyn FlightRepository + Send + Sync),
    flight_id: &str,
    patch: &FlightUpdatePatch,
) -> Result<(), DomainError> {
    let Some(target) = patch.status else {
        return Ok(());
    };
    let Some(current) = repo.find_by_id(flight_id).await? else {
        return Ok(());
    };
    if current.status == target {
        return Ok(());
    }
    if !can_transition(current.status, target) {
        return Err(DomainError::InvalidStateTransition {
            from: current.status.to_string(),
            to: target.to_string(),
        });
    }
    Ok(())
}

pub async fn validate_create_payload(
    repo: &(dyn FlightRepository + Send + Sync),
    mut dto: FlightCreate,
) -> Result<FlightCreate, DomainError> {
    validate_leg_payload(dto.inbound_leg.as_ref(), "inbound_leg", "inbound")?;
    validate_leg_payload(dto.outbound_leg.as_ref(), "outbound_leg", "outbound")?;

    dto.flight_id = normalize_optional_string(dto.flight_id);
    if let Some(flight_id) = dto.flight_id.as_deref() {
        if repo.find_by_id(flight_id).await?.is_some() {
            return Err(DomainError::Conflict(format!("航班 {flight_id} 已存在")));
        }
    }

    Ok(dto)
}

pub fn validate_update_payload(dto: &FlightUpdate) -> Result<(), DomainError> {
    let fields = update_fields_present(dto);
    if fields.is_empty() {
        return Err(DomainError::ValidationError("未提供任何更新字段".into()));
    }

    // PR3「只读展示列」：stand/gate/terminal/baggage_carousel 已从 FlightUpdate
    // 删除（serde deny_unknown_fields 直接 422），只能由占用服务回写，无需在此分支。

    validate_leg_payload(
        nullable_update_value(dto.inbound_leg.as_ref()),
        "inbound_leg",
        "inbound",
    )?;
    validate_leg_payload(
        nullable_update_value(dto.outbound_leg.as_ref()),
        "outbound_leg",
        "outbound",
    )?;

    Ok(())
}

pub fn update_fields_present(dto: &FlightUpdate) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if dto.status.is_some() {
        fields.push("status");
    }
    if dto.position.is_touched() {
        fields.push("position");
    }
    if dto.scheduled_departure.is_touched() {
        fields.push("scheduled_departure");
    }
    if dto.scheduled_arrival.is_touched() {
        fields.push("scheduled_arrival");
    }
    if dto.estimated_departure.is_touched() {
        fields.push("estimated_departure");
    }
    if dto.estimated_arrival.is_touched() {
        fields.push("estimated_arrival");
    }
    if dto.actual_departure.is_touched() {
        fields.push("actual_departure");
    }
    if dto.actual_arrival.is_touched() {
        fields.push("actual_arrival");
    }
    if dto.cobt_time.is_touched() {
        fields.push("cobt_time");
    }
    if dto.aircraft_type_detail.is_touched() {
        fields.push("aircraft_type_detail");
    }
    if dto.registration.is_touched() {
        fields.push("registration");
    }
    if dto.has_boarding_restriction.is_some() {
        fields.push("has_boarding_restriction");
    }
    if dto.is_quick_turnaround.is_some() {
        fields.push("is_quick_turnaround");
    }
    if dto.is_commercial_signed.is_some() {
        fields.push("is_commercial_signed");
    }
    if dto.inbound_leg.is_touched() {
        fields.push("inbound_leg");
    }
    if dto.outbound_leg.is_touched() {
        fields.push("outbound_leg");
    }
    if dto.flight_remarks.is_touched() {
        fields.push("flight_remarks");
    }
    if dto.load_planning_remarks.is_touched() {
        fields.push("load_planning_remarks");
    }
    if dto.aircraft_maintenance_remarks.is_touched() {
        fields.push("aircraft_maintenance_remarks");
    }
    if dto.aircraft_check_remarks.is_touched() {
        fields.push("aircraft_check_remarks");
    }
    if dto.is_draft.is_some() {
        fields.push("is_draft");
    }
    if dto.divert.is_some() {
        fields.push("divert");
    }
    if dto.flight_kind.is_touched() {
        fields.push("flight_kind");
    }
    if dto.direction.is_touched() {
        fields.push("direction");
    }
    fields
}

fn validate_leg_payload(
    payload: Option<&FlightLegPayload>,
    field_name: &str,
    expected_leg_type: &str,
) -> Result<(), DomainError> {
    let Some(payload) = payload else {
        return Ok(());
    };

    if payload.flight_no.trim().is_empty() {
        return Err(DomainError::ValidationError(format!("{field_name}.flight_no 不能为空")));
    }

    let actual_leg_type = payload.leg_type.trim().to_ascii_lowercase();
    if actual_leg_type != expected_leg_type {
        return Err(DomainError::ValidationError(format!(
            "{field_name}.leg_type 必须为 {expected_leg_type}"
        )));
    }

    Ok(())
}

fn sync_locked_fields() -> &'static [&'static str] {
    &[
        "registration",
        "status",
        "aircraft_type_detail",
        "inbound_leg",
        "outbound_leg",
        "position",
        "scheduled_arrival",
        "scheduled_departure",
        "estimated_arrival",
        "estimated_departure",
        "actual_arrival",
        "actual_departure",
        "cobt_time",
        "is_draft",
        "divert",
        "flight_kind",
        "direction",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::models::flight_state::can_transition;
    use fms_domain::models::value_objects::FlightStatus;

    #[test]
    fn status_transition_allows_scheduled_to_delayed() {
        assert!(can_transition(FlightStatus::Scheduled, FlightStatus::Delayed));
        assert!(!can_transition(FlightStatus::Cancelled, FlightStatus::Scheduled));
    }

    #[test]
    fn update_fields_present_tracks_status_and_resources() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "status": "delayed",
            "position": "P1",
            "flight_remarks": "note"
        }))
        .unwrap();
        let fields = update_fields_present(&dto);
        assert!(fields.contains(&"status"));
        assert!(fields.contains(&"position"));
        assert!(fields.contains(&"flight_remarks"));
    }

    #[test]
    fn denied_update_fields_blocks_sync_locked_for_non_admin() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "status": "delayed",
            "position": "P1",
            "cobt_time": "2026-07-17T10:00:00Z",
            "flight_remarks": "note"
        }))
        .unwrap();
        let denied = denied_update_fields(&dto, false, &[]);
        assert!(denied.contains(&"status".to_string()));
        assert!(denied.contains(&"position".to_string()));
        assert!(denied.contains(&"cobt_time".to_string()));
        assert!(!denied.iter().any(|f| f == "flight_remarks"));
    }

    #[test]
    fn denied_update_fields_allows_admin() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "status": "delayed",
            "position": "P1"
        }))
        .unwrap();
        assert!(denied_update_fields(&dto, true, &[]).is_empty());
        assert!(denied_update_fields(&dto, false, &["*".to_string()]).is_empty());
    }

    #[test]
    fn update_fields_present_tracks_ontology_v1_fields() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "is_draft": true,
            "divert": false,
            "flight_kind": "ferry",
            "direction": "outbound"
        }))
        .unwrap();
        let fields = update_fields_present(&dto);
        assert!(fields.contains(&"is_draft"));
        assert!(fields.contains(&"divert"));
        assert!(fields.contains(&"flight_kind"));
        assert!(fields.contains(&"direction"));
    }

    #[test]
    fn denied_update_fields_blocks_ontology_fields_for_non_admin() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "is_draft": false,
            "divert": true,
            "flight_kind": "ferry",
            "direction": "inbound",
            "flight_remarks": "note"
        }))
        .unwrap();
        let denied = denied_update_fields(&dto, false, &[]);
        for field in ["is_draft", "divert", "flight_kind", "direction"] {
            assert!(denied.contains(&field.to_string()), "{field} should be denied");
        }
        assert!(!denied.iter().any(|f| f == "flight_remarks"));
        assert!(denied_update_fields(&dto, true, &[]).is_empty());
    }
}
