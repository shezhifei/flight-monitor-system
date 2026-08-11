//! Flight domain event outbox — single source of truth.
//!
//! Centralizes the flight event-type names, the payload-builder helpers, and
//! the thin outbox write helper so the various flight writers (flight_service,
//! flight_runtime_service/timeline) no longer duplicate the raw outbox INSERT.

use fms_infrastructure::PgDomainEventOutboxRepository;
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use ulid::Ulid;

use fms_domain::error::DomainError;
use fms_domain::ports::flight_repository::FlightUpdatePatch;

// ---------------------------------------------------------------------------
// Event-type constants (single source of truth)
// ---------------------------------------------------------------------------

pub const FLIGHT_AGGREGATE_TYPE: &str = "flight";
pub const FLIGHT_CREATED_EVENT: &str = "flight.created_v2";
pub const FLIGHT_STATUS_UPDATED_EVENT: &str = "flight.status_updated_v2";
pub const FLIGHT_RESOURCE_UPDATED_EVENT: &str = "flight.resource_updated_v2";
pub const FLIGHT_LEG_UPSERTED_EVENT: &str = "flight.leg_upserted_v2";
pub const FLIGHT_REMARKS_UPDATED_EVENT: &str = "flight.remarks_updated_v2";
pub const FLIGHT_TIMELINE_UPSERTED_EVENT: &str = "flight.timeline_upserted_v2";
pub const FLIGHT_TIMELINE_DELETED_EVENT: &str = "flight.timeline_deleted_v2";
pub const FLIGHT_DELETED_EVENT: &str = "flight.deleted_v2";

// ---------------------------------------------------------------------------
// Payload builders (preserve exact field shapes of the previous inline payloads)
// ---------------------------------------------------------------------------

pub fn build_created_payload(flight_id: &str, status: &str, actor_id: Option<&str>) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "status": status,
        },
        "actor_id": actor_id,
    })
}

pub fn build_status_updated_payload(flight_id: &str, status: &str, actor_id: Option<&str>) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "field_name": "status",
            "status": status,
        },
        "actor_id": actor_id,
    })
}

pub fn build_resource_updated_payload(flight_id: &str, field_name: &str, actor_id: Option<&str>) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "field_name": field_name,
        },
        "actor_id": actor_id,
    })
}

pub fn build_leg_upserted_payload(flight_id: &str, leg_type: &str, actor_id: Option<&str>) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "leg_type": leg_type,
        },
        "actor_id": actor_id,
    })
}

pub fn build_remarks_updated_payload(flight_id: &str, field_name: &str, actor_id: Option<&str>) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "field_name": field_name,
        },
        "actor_id": actor_id,
    })
}

pub fn build_timeline_upserted_payload(
    flight_id: &str,
    milestone_code: &str,
    timeline_id: &str,
    timeline_value: Value,
    actor_id: Option<&str>,
) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "field_name": milestone_code,
            "timeline_id": timeline_id,
            "milestone_code": milestone_code,
        },
        "timeline_event": timeline_value,
        "actor_id": actor_id,
    })
}

pub fn build_timeline_deleted_payload(flight_id: &str, timeline_id: &str) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
            "field_name": "timeline",
            "timeline_id": timeline_id,
        },
    })
}

pub fn build_deleted_payload(flight_id: &str, actor_id: Option<&str>) -> Value {
    json!({
        "data": {
            "flight_id": flight_id,
        },
        "actor_id": actor_id,
    })
}

// ---------------------------------------------------------------------------
// Thin outbox write helper
// ---------------------------------------------------------------------------

/// Writes a single flight domain event row into `domain_event_outbox` within the
/// provided transaction. This is the canonical replacement for the previously
/// duplicated `insert_domain_event_outbox` functions in flight_service.rs and
/// flight_runtime_service/timeline.rs. The SQL and error mapping are unchanged.
pub async fn write_flight_outbox_event(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<(), DomainError> {
    let source_change_id = Ulid::new().to_string();
    PgDomainEventOutboxRepository::insert_event(
        tx,
        aggregate_type,
        aggregate_id,
        event_type,
        payload,
        &source_change_id,
    )
    .await
    .map_err(|e| DomainError::Internal(format!("outbox write failed: {e}")))?;
    Ok(())
}

/// Emit one outbox row per touched field on a flight update patch.
pub async fn write_flight_update_outbox_events(
    tx: &mut Transaction<'_, Postgres>,
    flight_id: &str,
    patch: &FlightUpdatePatch,
    actor_id: Option<&str>,
) -> Result<(), DomainError> {
    let mut events: Vec<(&str, Value)> = Vec::new();

    if let Some(status) = patch.status {
        events.push((
            FLIGHT_STATUS_UPDATED_EVENT,
            build_status_updated_payload(flight_id, &status.to_string(), actor_id),
        ));
    }

    let resource_fields = [
        ("gate", patch.gate.is_touched()),
        ("stand", patch.stand.is_touched()),
        ("terminal", patch.terminal.is_touched()),
        ("position", patch.position.is_touched()),
        ("baggage_carousel", patch.baggage_carousel.is_touched()),
        ("registration", patch.registration.is_touched()),
        ("aircraft_type_detail", patch.aircraft_type_detail.is_touched()),
        ("scheduled_departure", patch.scheduled_departure.is_touched()),
        ("scheduled_arrival", patch.scheduled_arrival.is_touched()),
        ("estimated_departure", patch.estimated_departure.is_touched()),
        ("estimated_arrival", patch.estimated_arrival.is_touched()),
        ("actual_departure", patch.actual_departure.is_touched()),
        ("actual_arrival", patch.actual_arrival.is_touched()),
        ("cobt_time", patch.cobt_time.is_touched()),
        ("has_boarding_restriction", patch.has_boarding_restriction.is_some()),
        ("is_quick_turnaround", patch.is_quick_turnaround.is_some()),
        ("is_commercial_signed", patch.is_commercial_signed.is_some()),
    ];
    for (field_name, touched) in resource_fields {
        if touched {
            events.push((
                FLIGHT_RESOURCE_UPDATED_EVENT,
                build_resource_updated_payload(flight_id, field_name, actor_id),
            ));
        }
    }

    if patch.inbound_leg.is_touched() {
        events.push((
            FLIGHT_LEG_UPSERTED_EVENT,
            build_leg_upserted_payload(flight_id, "inbound", actor_id),
        ));
    }
    if patch.outbound_leg.is_touched() {
        events.push((
            FLIGHT_LEG_UPSERTED_EVENT,
            build_leg_upserted_payload(flight_id, "outbound", actor_id),
        ));
    }

    let remark_fields = [
        ("flight_remarks", patch.flight_remarks.is_touched()),
        ("load_planning_remarks", patch.load_planning_remarks.is_touched()),
        (
            "aircraft_maintenance_remarks",
            patch.aircraft_maintenance_remarks.is_touched(),
        ),
        ("aircraft_check_remarks", patch.aircraft_check_remarks.is_touched()),
    ];
    for (field_name, touched) in remark_fields {
        if touched {
            events.push((
                FLIGHT_REMARKS_UPDATED_EVENT,
                build_remarks_updated_payload(flight_id, field_name, actor_id),
            ));
        }
    }

    for (event_type, payload) in events {
        write_flight_outbox_event(tx, FLIGHT_AGGREGATE_TYPE, flight_id, event_type, payload).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_outbox_event_types_match_subscriber_contract() {
        assert_eq!(FLIGHT_CREATED_EVENT, "flight.created_v2");
        assert_eq!(FLIGHT_DELETED_EVENT, "flight.deleted_v2");
        assert_eq!(FLIGHT_STATUS_UPDATED_EVENT, "flight.status_updated_v2");
        assert_eq!(FLIGHT_RESOURCE_UPDATED_EVENT, "flight.resource_updated_v2");
        assert_eq!(FLIGHT_LEG_UPSERTED_EVENT, "flight.leg_upserted_v2");
        assert_eq!(FLIGHT_REMARKS_UPDATED_EVENT, "flight.remarks_updated_v2");
    }
}
