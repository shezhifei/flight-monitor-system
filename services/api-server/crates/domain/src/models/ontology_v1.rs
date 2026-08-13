//! 本体 V1 领域模型（ONTOLOGY_V1.md §4）
//!
//! 飞机中心：`registration` 为 Aircraft 主键；机位/口是独立的时段关系对象；
//! 周转链接连接任务对；资源调整建议为 Operational 子域对象。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::value_objects::{FlightId, GateNumber, StandNumber};

// ---------------------------------------------------------------------------
// Aircraft — 飞机（本体中心）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aircraft {
    /// 机号，原样存储 + 唯一索引（不变量 1）
    pub registration: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// StandOccupation — 机位占用（主体=飞机）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccupationKind {
    /// 常规占用
    Normal,
    /// 拖曳过渡占用：from_stand → to_stand
    Moving,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccupationStatus {
    Active,
    Released,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandOccupation {
    pub id: String,
    pub registration: String,
    pub stand_code: StandNumber,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub kind: OccupationKind,
    pub moving_to_stand: Option<StandNumber>,
    pub flight_id: Option<FlightId>,
    pub status: OccupationStatus,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// GateAssignment — 登机口分配（首次分配即生效）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Active,
    Released,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateAssignment {
    pub id: String,
    pub registration: String,
    pub gate_code: GateNumber,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub flight_id: Option<FlightId>,
    pub status: AssignmentStatus,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// TurnaroundLink — 进-出任务衔接边（不是机号边）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnaroundLinkStatus {
    Active,
    Broken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnaroundLinkSource {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnaroundLink {
    pub id: String,
    pub inbound_flight_id: FlightId,
    pub outbound_flight_id: FlightId,
    pub status: TurnaroundLinkStatus,
    pub source: TurnaroundLinkSource,
    pub broken_reason: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ResourceAdjustmentSuggestion — 分权建议（Operational 子域）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    /// 机位建议（仅 AOC 可接受）
    Stand,
    /// 登机口建议（仅 TOC 可接受）
    Gate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Pending,
    AcceptedExecuted,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAdjustmentSuggestion {
    pub id: String,
    pub flight_id: FlightId,
    pub kind: SuggestionKind,
    pub current_value: Option<String>,
    pub suggested_value: String,
    pub status: SuggestionStatus,
    pub reason: Option<String>,
    /// 内嵌资源 Action 载荷（Allocate/Adjust 参数 + 触发上下文）
    pub payload: serde_json::Value,
    pub created_by: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ResourceAdjustmentSuggestion {
    pub fn is_expired(&self) -> bool {
        matches!(self.status, SuggestionStatus::Expired) || self.expires_at.is_some_and(|expires| expires <= Utc::now())
    }

    /// §4.9: 接受语义 — 机位建议仅 AOC、口建议仅 TOC。
    pub fn required_accept_permission(&self) -> &'static str {
        match self.kind {
            SuggestionKind::Stand => "ontology.suggestion.accept_stand",
            SuggestionKind::Gate => "ontology.suggestion.accept_gate",
        }
    }
}
