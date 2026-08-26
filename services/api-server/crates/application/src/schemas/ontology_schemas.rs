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
// 机位占用 StandOccupation 对象的正式写路径。
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocateStandRequest {
    /// 机号（主体）；原样存储
    pub registration: String,
    pub stand_code: String,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    /// normal | moving
    #[serde(default = "default_occupation_kind")]
    pub kind: String,
    /// kind=moving 时必填
    pub moving_to_stand: Option<String>,
    /// 可选原因航段
    pub flight_id: Option<String>,
    /// 是否同步回写 Flight.stand 计划字段
    #[serde(default = "default_true")]
    pub sync_flight_plan: bool,
}

fn default_occupation_kind() -> String {
    "normal".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustStandRequest {
    pub stand_code: Option<String>,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    pub kind: Option<String>,
    pub moving_to_stand: Option<String>,
    /// 是否同步回写 Flight.stand
    #[serde(default = "default_true")]
    pub sync_flight_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseResourceRequest {
    pub released_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandOccupationResult {
    pub occupation: serde_json::Value,
    /// 时段重叠告警（不硬拦，§4.4）
    pub overlap_warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// 登机口分配 GateAssignment（§4.5）— 正式写路径
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocateGateRequest {
    pub registration: String,
    pub gate_code: String,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub flight_id: Option<String>,
    #[serde(default = "default_true")]
    pub sync_flight_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustGateRequest {
    pub gate_code: Option<String>,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "default_true")]
    pub sync_flight_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateAssignmentResult {
    pub assignment: serde_json::Value,
    /// 口-位弱校验不一致告警（§4.5）
    pub consistency_warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// 转盘分配 CarouselAssignment — 正式写路径（主体=航班；零业务约束）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocateCarouselRequest {
    /// 转盘 code（必须在启用目录 + 某座启用楼成员表里）
    pub carousel_code: String,
    /// 主体：航班
    pub flight_id: String,
    /// 可选机号投影（不参与任何规则）
    pub registration: Option<String>,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    /// 客户端幂等 token；重复 token 返回既有行而非新建
    pub client_action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustCarouselRequest {
    pub carousel_code: Option<String>,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarouselAssignmentResult {
    pub assignment: serde_json::Value,
    /// false = 重复幂等 token 命中既有行（未新建，不重复回写展示列）
    pub inserted: bool,
}

// ---------------------------------------------------------------------------
// 周转链接 TurnaroundLink（§4.8）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTurnaroundLinkRequest {
    pub inbound_flight_id: String,
    pub outbound_flight_id: String,
    /// auto | manual；默认 manual
    #[serde(default = "default_link_source_manual")]
    pub source: String,
    pub created_by: Option<String>,
}

fn default_link_source_manual() -> String {
    "manual".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreakTurnaroundLinkRequest {
    pub reason: Option<String>,
    pub broken_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLinkScanRequest {
    /// 时间窗分钟（出港计划 − 进港到达），默认 360
    pub window_minutes: Option<i64>,
    /// 单次扫描出港候选上限，默认 100
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLinkScanResult {
    pub evaluated: usize,
    pub created: Vec<String>,
    pub skipped: usize,
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// 新建建议（§4.9）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSuggestionRequest {
    pub flight_id: String,
    /// stand | gate
    pub kind: String,
    pub suggested_value: String,
    pub current_value: Option<String>,
    pub reason: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by: Option<String>,
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
    pub carousel_assignments: Vec<serde_json::Value>,
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
