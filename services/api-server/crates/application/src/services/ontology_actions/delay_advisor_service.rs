//! Generates a delay-handling proposal (`update_delay` plus impacted dispatch actions).

use std::sync::Arc;

use chrono::Duration;
use serde_json::{json, Value};

use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::flight_repository::FlightRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_datetime, constraint, required_str, suggestion_envelope};

pub struct DelayAdvisorService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
}

impl DelayAdvisorService {
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_repo,
            dispatch_repo,
            anomaly_repo,
        }
    }

    pub async fn suggest(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let flight_id = required_str(args, "flight_id")?;
        let flight = self
            .flight_repo
            .find_by_id(flight_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("flight {flight_id}")))?;

        let new_departure = match arg_datetime(args, "new_estimated_departure")? {
            Some(value) => value,
            None => flight
                .estimated_departure
                .or(flight.scheduled_departure)
                .map(|base| base + Duration::minutes(30))
                .ok_or_else(|| {
                    OntologyActionError::InvalidArguments(
                        "flight has no departure time; provide `new_estimated_departure`".to_string(),
                    )
                })?,
        };

        let delayed = flight.status == FlightStatus::Delayed;
        let open_anomalies = self
            .anomaly_repo
            .find_by_flight(flight_id)
            .await
            .map_err(repo_err)?
            .into_iter()
            .filter(|a| a.status != fms_domain::models::anomaly::AnomalyStatus::Resolved)
            .collect::<Vec<_>>();
        let pending_orders = self
            .dispatch_repo
            .find_by_flight(flight_id)
            .await
            .map_err(repo_err)?
            .into_iter()
            .filter(|o| matches!(o.status.as_ref(), "pending" | "assigned"))
            .collect::<Vec<_>>();
        let impacted_orders = pending_orders
            .iter()
            .filter(|o| o.planned_start_time.is_some_and(|t| t < new_departure))
            .map(|o| {
                json!({
                    "dispatch_order_id": o.id,
                    "task_type": o.task_type,
                    "planned_start_time": o.planned_start_time,
                    "suggested_action": "reschedule_after_new_departure",
                })
            })
            .collect::<Vec<_>>();

        let constraint_results = vec![
            constraint("flight_exists", true, "error", None),
            constraint("flight_delayed", delayed, "warning", None),
            constraint(
                "new_departure_after_current",
                flight
                    .estimated_departure
                    .or(flight.scheduled_departure)
                    .is_none_or(|base| new_departure > base),
                "warning",
                None,
            ),
        ];

        Ok(suggestion_envelope(
            "Flight",
            flight_id,
            "update_delay",
            json!({ "new_estimated_departure": new_departure }),
            "medium",
            constraint_results,
            json!({
                "status": flight.status.code(),
                "estimated_departure": flight.estimated_departure,
                "scheduled_departure": flight.scheduled_departure,
            }),
            json!({ "estimated_departure": new_departure }),
            if delayed { 0.85 } else { 0.5 },
            &format!(
                "delay handling for flight {}: new departure {} with {} impacted dispatch order(s)",
                flight_id,
                new_departure,
                impacted_orders.len()
            ),
            json!({
                "open_anomalies": open_anomalies.iter().map(|a| json!({
                    "anomaly_id": a.anomaly_id,
                    "severity": a.severity.as_ref(),
                })).collect::<Vec<_>>(),
                "related_dispatch_actions": impacted_orders,
            }),
        ))
    }
}
