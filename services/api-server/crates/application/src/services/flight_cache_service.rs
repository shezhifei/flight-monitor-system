use serde::Serialize;
use std::sync::Arc;
use tracing::warn;

use crate::schemas::flight_schemas::FlightResponse;
use fms_domain::ports::flight_cache_backend::FlightCacheBackend;

pub struct FlightCacheService {
    backend: Option<Arc<dyn FlightCacheBackend>>,
}

impl FlightCacheService {
    pub fn disabled() -> Self {
        Self { backend: None }
    }

    pub fn with_backend(backend: Arc<dyn FlightCacheBackend>) -> Self {
        Self { backend: Some(backend) }
    }

    pub async fn invalidate_related_flight_cache(&self, flight_id: Option<&str>) {
        self.invalidate_single_flight_cache(flight_id).await;
    }

    pub async fn invalidate_single_flight_cache(&self, flight_id: Option<&str>) {
        let Some(flight_id) = flight_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        let Some(backend) = self.backend.as_ref() else {
            return;
        };

        backend.invalidate_single_flight_cache(flight_id).await;
    }

    pub async fn refresh_single_flight_cache(&self, flight: &FlightResponse) {
        let Some(flight_id) = flight
            .flight_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return;
        };
        let Some(backend) = self.backend.as_ref() else {
            return;
        };
        let Some(payload) = serialize_cache_payload(flight, flight_id) else {
            return;
        };

        backend.refresh_single_flight_cache(flight_id, payload.as_str()).await;
    }

    pub async fn invalidate_flights_cache(&self) {
        let Some(backend) = self.backend.as_ref() else {
            return;
        };

        backend.invalidate_flights_cache().await;
    }
}

pub fn flight_list_requires_global_invalidation(
    changed_fields: &[String],
    append_to_list_cache: bool,
    remove_from_list_cache: bool,
) -> bool {
    if append_to_list_cache || remove_from_list_cache {
        return true;
    }

    changed_fields.iter().any(|field| {
        matches!(
            field.trim().to_ascii_lowercase().as_str(),
            "create" | "delete" | "flight_id"
        )
    })
}

fn serialize_cache_payload<T>(value: &T, flight_id: &str) -> Option<String>
where
    T: Serialize,
{
    serde_json::to_string(value)
        .map_err(|error| {
            warn!(flight_id, error = %error, "failed to serialize flight cache payload");
            error
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{flight_list_requires_global_invalidation, FlightCacheService};

    #[tokio::test]
    async fn disabled_cache_service_is_best_effort_noop() {
        let service = FlightCacheService::disabled();
        service.invalidate_related_flight_cache(Some("flight_001")).await;
        service.invalidate_single_flight_cache(Some("flight_001")).await;
        service.invalidate_flights_cache().await;
        service.invalidate_related_flight_cache(None).await;
        service
            .refresh_single_flight_cache(&crate::schemas::flight_schemas::FlightResponse {
                flight_id: Some("flight_001".to_string()),
                flight_number: None,
                airline_code: None,
                registration: None,
                aircraft_type_detail: None,
                status: None,
                scheduled_departure: None,
                scheduled_arrival: None,
                estimated_departure: None,
                estimated_arrival: None,
                actual_departure: None,
                actual_arrival: None,
                cobt_time: None,
                codt: None,
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
                stand: None,
                gate: None,
                terminal: None,
                position: None,
                baggage_carousel: None,
                has_boarding_restriction: false,
                is_quick_turnaround: false,
                is_commercial_signed: true,
                inbound_leg: None,
                outbound_leg: None,
                anomaly_summary: crate::schemas::flight_schemas::FlightAnomalySummary::default(),
                business_cases: Vec::new(),
                created_at: None,
                updated_at: None,
                version: 0,
                labels: Vec::new(),
                flight_remarks: None,
                load_planning_remarks: None,
                aircraft_maintenance_remarks: None,
                aircraft_check_remarks: None,
                created_by: None,
                updated_by: None,
                risk_score: None,
                risk_level: None,
                risk_reasons: None,
                next_primary_action: None,
                data_freshness: None,
            })
            .await;
    }

    #[test]
    fn global_invalidation_is_reserved_for_membership_changes() {
        assert!(!flight_list_requires_global_invalidation(
            &["status".to_string(), "gate".to_string()],
            false,
            false,
        ));
        assert!(!flight_list_requires_global_invalidation(
            &["flight_remarks".to_string()],
            false,
            false,
        ));
        assert!(flight_list_requires_global_invalidation(
            &["create".to_string()],
            false,
            false,
        ));
        assert!(flight_list_requires_global_invalidation(
            &["delete".to_string()],
            false,
            false,
        ));
        assert!(flight_list_requires_global_invalidation(
            &["status".to_string()],
            true,
            false,
        ));
    }
}
