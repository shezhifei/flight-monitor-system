//! Dashboard workbench response schemas.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct DashboardWorkbenchResponse {
    pub generated_at: DateTime<Utc>,
    pub user_context: DashboardUserContext,
    pub role_hint: String,
    pub attention_items: Vec<DashboardAttentionItem>,
    pub risk_summary: DashboardRiskSummary,
    pub recent_changes: Vec<DashboardRecentChange>,
    pub quick_links: Vec<DashboardQuickLink>,
    pub module_status: Vec<DashboardModuleStatus>,
    pub degraded_sources: Vec<DashboardDegradedSource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardUserContext {
    pub user_id: String,
    pub username: Option<String>,
    pub department: Option<String>,
    pub is_admin: bool,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardAttentionItem {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub source: String,
    pub source_id: Option<String>,
    pub owner_id: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardRiskSummary {
    pub unresolved_anomalies: i64,
    pub high_risk_flights: i64,
    pub dispatch_conflicts: i64,
    pub stale_data_indicators: Vec<DashboardStaleDataIndicator>,
    pub high_risk_flight_refs: Vec<DashboardRiskFlightRef>,
    pub dispatch_conflict_refs: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardRiskFlightRef {
    pub flight_id: String,
    pub anomaly_id: String,
    pub severity: String,
    pub title: String,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardStaleDataIndicator {
    pub source: String,
    pub state: String,
    pub detail: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardRecentChange {
    pub id: String,
    pub title: String,
    pub source: String,
    pub changed_at: DateTime<Utc>,
    pub severity: Option<String>,
    pub entity_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardQuickLink {
    pub id: String,
    pub label: String,
    pub href: String,
    pub module: String,
    pub intent: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardModuleStatus {
    pub module: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardDegradedSource {
    pub source: String,
    pub reason: String,
}
