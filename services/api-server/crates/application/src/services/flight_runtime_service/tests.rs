//! Tests for flight_runtime_service.

use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::ports::flight_timeline_event_repository::{
    FlightTimelineEvent, FlightTimelineEventTransactionalRepository,
};
use fms_infrastructure::repositories::pg_flight_timeline_event_repository::PgFlightTimelineEventRepository;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

use super::types::DispatchTimelineEventWriteResult;
use crate::schemas::flight_schemas::DispatchTimelineEventResponse;

async fn insert_dispatch_timeline_event(
    pool: &sqlx::PgPool,
    event: &DispatchTimelineEventResponse,
    client_action_id: Option<&str>,
) -> Result<DispatchTimelineEventWriteResult, DomainError> {
    let repo = PgFlightTimelineEventRepository::new(pool.clone());
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    let domain_event = FlightTimelineEvent {
        timeline_id: event.timeline_id.clone(),
        flight_id: event.flight_id.clone(),
        milestone_code: event.milestone_code.clone(),
        occurred_at: event.occurred_at,
        leg_type: event.leg_type.clone(),
        recorded_by: event.recorded_by.clone(),
        client_action_id: event.client_action_id.clone(),
        source: event.source.clone(),
        payload: event.payload.clone(),
        created_at: event.created_at,
    };
    let write = repo.insert_in_tx(&mut tx, &domain_event, client_action_id).await?;
    tx.commit()
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(DispatchTimelineEventWriteResult {
        event: DispatchTimelineEventResponse {
            timeline_id: write.event.timeline_id,
            flight_id: write.event.flight_id,
            milestone_code: write.event.milestone_code,
            occurred_at: write.event.occurred_at,
            leg_type: write.event.leg_type,
            recorded_by: write.event.recorded_by,
            client_action_id: write.event.client_action_id,
            source: write.event.source,
            payload: write.event.payload,
            created_at: write.event.created_at,
        },
        inserted: write.inserted,
    })
}

#[tokio::test]
async fn dispatch_timeline_insert_returns_error_when_db_write_fails() {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(50))
        .connect_lazy("postgres://invalid:invalid@127.0.0.1:1/invalid")
        .expect("lazy pool should not connect during construction");
    let event = DispatchTimelineEventResponse {
        timeline_id: "01HX0000000000000000000000".to_string(),
        flight_id: "01HX0000000000000000000001".to_string(),
        milestone_code: "boarding_start".to_string(),
        occurred_at: Utc::now(),
        leg_type: Some("outbound".to_string()),
        recorded_by: Some("tester".to_string()),
        client_action_id: Some("test-action".to_string()),
        source: "manual".to_string(),
        payload: json!({}),
        created_at: Utc::now(),
    };

    let result = insert_dispatch_timeline_event(&pool, &event, event.client_action_id.as_deref()).await;

    assert!(
        result.is_err(),
        "failed DB writes must not be converted into successful timeline events"
    );
}
