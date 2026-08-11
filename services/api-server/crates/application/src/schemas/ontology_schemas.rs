//! 本体 V1 请求/响应 DTO（ONTOLOGY_V1.md §6/§7）

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ReassignAircraft（§7）
// ---------------------------------------------------------------------------

/// §7.4 最小入参：flight_id + new_registration；批量时为列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReassignAircraftChange {
    pub flight_id: String,
    pub new_registration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReassignAircraftRequest {
    pub changes: Vec<ReassignAircraftChange>,
    /// 可选：审计/幂等关联 ID
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReassignAppliedResult {
    pub flight_id: String,
    pub old_registration: Option<String>,
    pub new_registration: String,
    pub broken_links: Vec<String>,
    pub created_links: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReassignAircraftResponse {
    pub applied: Vec<ReassignAppliedResult>,
}

// ---------------------------------------------------------------------------
// 建议（§4.9）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionAcceptRequest {
    pub accepted_by: String,
    /// 接受者权限（服务层二次校验，不变量 12）
    pub actor_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionRejectRequest {
    pub rejected_by: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionQuery {
    pub flight_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

// ---------------------------------------------------------------------------
// draft 整批确认（§3.3）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmDraftFlightsRequest {
    pub flight_ids: Vec<String>,
    pub confirmed_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmDraftFlightsResponse {
    pub confirmed: Vec<String>,
    pub missing: Vec<String>,
}

// ---------------------------------------------------------------------------
// 双视图（§5.3）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightResourceView {
    pub flight_id: String,
    pub registration: Option<String>,
    pub plan_stand: Option<String>,
    pub plan_gate: Option<String>,
    pub occupations: Vec<serde_json::Value>,
    pub assignments: Vec<serde_json::Value>,
    pub turnaround_links: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AircraftResourceView {
    pub registration: String,
    pub in_field: bool,
    pub current_stand: Option<String>,
    pub current_gate: Option<String>,
    pub occupations: Vec<serde_json::Value>,
    pub assignments: Vec<serde_json::Value>,
    pub flights: Vec<serde_json::Value>,
}
