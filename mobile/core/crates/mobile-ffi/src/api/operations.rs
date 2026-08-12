//! Mobile operations exports (plan §4 战情).

use mobile_core::dto::operations as core;

use super::runtime;

pub struct OperationsEvent {
    pub event_id: String,
    pub event_type: String,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub flight_id: Option<String>,
    pub occurred_at: String,
    pub source: String,
}

impl From<core::OperationsEventItem> for OperationsEvent {
    fn from(e: core::OperationsEventItem) -> Self {
        Self {
            event_id: e.event_id,
            event_type: e.event_type,
            severity: e.severity,
            status: e.status,
            title: e.title,
            flight_id: e.flight_id,
            occurred_at: e.occurred_at,
            source: e.source,
        }
    }
}

pub struct OperationsFeed {
    pub user_id: Option<String>,
    pub generated_at: Option<String>,
    pub total: i64,
    pub event_type_counts: std::collections::HashMap<String, i64>,
    pub severity_counts: std::collections::HashMap<String, i64>,
    pub events: Vec<OperationsEvent>,
}

impl From<core::OperationsEventsResponse> for OperationsFeed {
    fn from(r: core::OperationsEventsResponse) -> Self {
        Self {
            user_id: r.user_id,
            generated_at: r.generated_at,
            total: r.total,
            event_type_counts: r.event_type_counts,
            severity_counts: r.severity_counts,
            events: r.events.into_iter().map(Into::into).collect(),
        }
    }
}

/// `GET /api/v2/mobile/operations/events`.
pub async fn operations_events(limit: i64) -> anyhow::Result<OperationsFeed> {
    let rt = runtime()?;
    Ok(
        mobile_core::api::operations::operations_events(&rt.client, limit)
            .await?
            .into(),
    )
}
