//! 微模型输入输出 Schema 定义
//!
//! 航班风险摘要微模型和派工重排顾问微模型的 I/O schema。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use fms_domain::models::ai_proposal::RiskLevel;

// ---------------------------------------------------------------------------
// 通用微模型输出部分
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModelConfidence {
    pub score: f64,
    pub level: ConfidenceLevel,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
    Unknown,
}

impl ConfidenceLevel {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::High
        } else if score >= 0.5 {
            Self::Medium
        } else if score > 0.0 {
            Self::Low
        } else {
            Self::Unknown
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}

impl Default for MicroModelConfidence {
    fn default() -> Self {
        Self {
            score: 0.0,
            level: ConfidenceLevel::Unknown,
            reasons: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 航班风险摘要微模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRiskInput {
    pub flight_id: String,
    #[serde(default = "default_context_window")]
    pub context_window_minutes: i64,
    #[serde(default)]
    pub include_weather: bool,
    #[serde(default)]
    pub include_manual_context: bool,
    pub risk_ceiling: Option<String>,
}

fn default_context_window() -> i64 {
    60
}

impl FlightRiskInput {
    pub fn risk_ceiling_level(&self) -> Option<RiskLevel> {
        self.risk_ceiling.as_ref().and_then(|s| RiskLevel::from_str_loose(s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRiskEvidence {
    pub signal_code: String,
    pub signal_label: String,
    pub severity: String,
    pub evidence_type: EvidenceType,
    pub object_id: Option<String>,
    pub data_view: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub weight: f64,
    pub raw_value: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    ObjectReference,
    DataView,
    ManualAnnotation,
}

impl EvidenceType {
    pub fn label(self) -> &'static str {
        match self {
            Self::ObjectReference => "object_reference",
            Self::DataView => "data_view",
            Self::ManualAnnotation => "manual_annotation",
        }
    }
}

impl FlightRiskEvidence {
    pub fn new(
        signal_code: impl Into<String>,
        signal_label: impl Into<String>,
        severity: impl Into<String>,
        evidence_type: EvidenceType,
    ) -> Self {
        Self {
            signal_code: signal_code.into(),
            signal_label: signal_label.into(),
            severity: severity.into(),
            evidence_type,
            object_id: None,
            data_view: None,
            timestamp: Utc::now(),
            weight: 0.0,
            raw_value: None,
        }
    }

    pub fn with_object(mut self, object_id: impl Into<String>) -> Self {
        self.object_id = Some(object_id.into());
        self
    }

    pub fn with_data_view(mut self, view: impl Into<String>) -> Self {
        self.data_view = Some(view.into());
        self
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_raw_value(mut self, value: Value) -> Self {
        self.raw_value = Some(value);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRiskProposal {
    pub proposal_type: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub action_name: String,
    pub arguments: Value,
    pub rationale: String,
    pub priority: i32,
    pub risk_if_not_acted: Option<String>,
}

impl FlightRiskProposal {
    pub fn new(
        proposal_type: impl Into<String>,
        object_type: impl Into<String>,
        action_name: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            proposal_type: proposal_type.into(),
            object_type: object_type.into(),
            object_id: None,
            action_name: action_name.into(),
            arguments: Value::Null,
            rationale: rationale.into(),
            priority: 5,
            risk_if_not_acted: None,
        }
    }

    pub fn with_object_id(mut self, id: impl Into<String>) -> Self {
        self.object_id = Some(id.into());
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_risk_if_not_acted(mut self, risk: impl Into<String>) -> Self {
        self.risk_if_not_acted = Some(risk.into());
        self
    }

    pub fn to_ontology_action(&self) -> (String, String, String) {
        (
            self.object_type.clone(),
            self.object_id.clone().unwrap_or_default(),
            self.action_name.clone(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightRiskOutput {
    pub model_id: String,
    pub model_version: String,
    pub flight_id: String,
    pub risk_score: i32,
    pub risk_level: String,
    pub evidence: Vec<FlightRiskEvidence>,
    pub confidence: MicroModelConfidence,
    pub proposals: Vec<FlightRiskProposal>,
    pub limitations: Vec<String>,
    pub execution_time_ms: u64,
    pub input_snapshot: Value,
}

impl FlightRiskOutput {
    pub fn new(flight_id: impl Into<String>) -> Self {
        Self {
            model_id: "flight_risk_v1".to_string(),
            model_version: "1.0.0".to_string(),
            flight_id: flight_id.into(),
            risk_score: 0,
            risk_level: "low".to_string(),
            evidence: Vec::new(),
            confidence: MicroModelConfidence::default(),
            proposals: Vec::new(),
            limitations: Vec::new(),
            execution_time_ms: 0,
            input_snapshot: Value::Null,
        }
    }

    pub fn add_evidence(&mut self, evidence: FlightRiskEvidence) {
        self.evidence.push(evidence);
    }

    pub fn add_proposal(&mut self, proposal: FlightRiskProposal) {
        self.proposals.push(proposal);
    }

    pub fn add_limitation(&mut self, limitation: impl Into<String>) {
        self.limitations.push(limitation.into());
    }

    pub fn proposal_count(&self) -> usize {
        self.proposals.len()
    }

    pub fn has_critical_evidence(&self) -> bool {
        self.evidence.iter().any(|e| e.severity == "critical")
    }
}

// ---------------------------------------------------------------------------
// 派工重排顾问微模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanInput {
    pub shift_id: String,
    pub target_time_window: TimeWindow,
    #[serde(default)]
    pub dispatch_order_ids: Vec<String>,
    #[serde(default = "default_include_locked")]
    pub include_locked: bool,
    pub optimization_objective: OptObjective,
    #[serde(default)]
    pub hard_constraints: Vec<HardConstraintDef>,
    #[serde(default)]
    pub soft_constraints: Vec<SoftConstraintDef>,
    #[serde(default = "default_max_proposals")]
    pub max_proposals: usize,
}

fn default_include_locked() -> bool {
    false
}

fn default_max_proposals() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptObjective {
    MinimizeDelay,
    BalanceWorkload,
    MinimizeEquipmentMoves,
    PrioritizeVIP,
}

impl OptObjective {
    pub fn label(self) -> &'static str {
        match self {
            Self::MinimizeDelay => "minimize_delay",
            Self::BalanceWorkload => "balance_workload",
            Self::MinimizeEquipmentMoves => "minimize_equipment_moves",
            Self::PrioritizeVIP => "prioritize_vip",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "minimize_delay" | "delay" => Some(Self::MinimizeDelay),
            "balance_workload" | "workload" | "balance" => Some(Self::BalanceWorkload),
            "minimize_equipment_moves" | "equipment" | "moves" => Some(Self::MinimizeEquipmentMoves),
            "prioritize_vip" | "vip" | "priority" => Some(Self::PrioritizeVIP),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardConstraintDef {
    pub constraint_type: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftConstraintDef {
    pub constraint_type: String,
    pub weight: f64,
    pub desired_value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanProposal {
    pub proposal_id: String,
    pub action: ReplanAction,
    pub target_object_type: String,
    pub target_object_id: String,
    pub from_state: Option<Value>,
    pub to_state: Value,
    pub rationale: String,
    pub constraint_satisfaction: Vec<ConstraintSatisfaction>,
    pub estimated_impact: ImpactEstimate,
    pub risk_level: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplanAction {
    ReassignTeam,
    ReassignEquipment,
    Reschedule,
    Cancel,
    Create,
}

impl ReplanAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReassignTeam => "reassign_team",
            Self::ReassignEquipment => "reassign_equipment",
            Self::Reschedule => "reschedule",
            Self::Cancel => "cancel",
            Self::Create => "create",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "reassign_team" | "team" => Some(Self::ReassignTeam),
            "reassign_equipment" | "equipment" => Some(Self::ReassignEquipment),
            "reschedule" | "schedule" => Some(Self::Reschedule),
            "cancel" => Some(Self::Cancel),
            "create" => Some(Self::Create),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintSatisfaction {
    pub constraint_type: String,
    pub satisfied: bool,
    pub deviation: Option<f64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactEstimate {
    pub delay_reduction_minutes: Option<i64>,
    pub workload_balance_delta: Option<f64>,
    pub equipment_moves_saved: Option<i32>,
    pub affected_orders: usize,
    pub affected_teams: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintViolation {
    pub constraint_type: String,
    pub severity: String,
    pub description: String,
    pub affected_objects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverInfo {
    pub solver_type: String,
    pub iterations: u32,
    pub nodes_explored: Option<u64>,
    pub optimality_gap: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanOutput {
    pub model_id: String,
    pub model_version: String,
    pub shift_id: String,
    pub proposals: Vec<DispatchReplanProposal>,
    pub constraint_violations: Vec<ConstraintViolation>,
    pub optimization_score: Option<f64>,
    pub solver_info: SolverInfo,
    pub execution_time_ms: u64,
    pub limitations: Vec<String>,
    pub input_snapshot: Value,
}

impl DispatchReplanOutput {
    pub fn new(shift_id: impl Into<String>) -> Self {
        Self {
            model_id: "dispatch_replan_v1".to_string(),
            model_version: "1.0.0".to_string(),
            shift_id: shift_id.into(),
            proposals: Vec::new(),
            constraint_violations: Vec::new(),
            optimization_score: None,
            solver_info: SolverInfo {
                solver_type: "unknown".to_string(),
                iterations: 0,
                nodes_explored: None,
                optimality_gap: None,
            },
            execution_time_ms: 0,
            limitations: Vec::new(),
            input_snapshot: Value::Null,
        }
    }

    pub fn add_proposal(&mut self, proposal: DispatchReplanProposal) {
        self.proposals.push(proposal);
    }

    pub fn add_violation(&mut self, violation: ConstraintViolation) {
        self.constraint_violations.push(violation);
    }

    pub fn proposal_count(&self) -> usize {
        self.proposals.len()
    }

    pub fn has_hard_violations(&self) -> bool {
        self.constraint_violations
            .iter()
            .any(|v| v.severity == "critical" || v.severity == "high")
    }
}

// ---------------------------------------------------------------------------
// 微模型执行请求/响应 DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModelExecuteRequest {
    pub model_id: String,
    pub input: Value,
    pub job_id: String,
    pub run_id: String,
    #[serde(default)]
    pub generate_proposals: bool,
    #[serde(default)]
    pub include_input_snapshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModelExecuteResponse {
    pub execution_id: String,
    pub model_id: String,
    pub model_version: String,
    pub status: String,
    pub output: Value,
    pub execution_time_ms: u64,
    /// Advisory proposal candidates — NOT canonical persisted proposals.
    /// These are suggestions from the micro-model that have NOT been validated
    /// or ingested through AiOutputValidator / AiProposalIngestService.
    pub proposal_candidates: Vec<Value>,
    /// Canonical proposal IDs created through the AIP ingest pipeline.
    /// Always empty unless the full ingest pathway is enabled.
    pub canonical_proposals_created: Vec<String>,
    /// Input snapshot for replay/evaluation, included only when requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_snapshot: Option<Value>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// 停机位冲突微模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandConflictInput {
    pub flight_id: String,
    pub current_stand_id: String,
    #[serde(default)]
    pub conflict_flight_id: Option<String>,
    #[serde(default = "default_conflict_window")]
    pub conflict_window_minutes: i64,
}

fn default_conflict_window() -> i64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandConflictOutput {
    pub model_id: String,
    pub model_version: String,
    pub conflict_detected: bool,
    pub recommended_stand: Option<String>,
    pub conflict_details: String,
    pub confidence: MicroModelConfidence,
    pub execution_time_ms: u64,
}

impl Default for StandConflictOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl StandConflictOutput {
    pub fn new() -> Self {
        Self {
            model_id: "stand_conflict_v1".to_string(),
            model_version: "1.0.0".to_string(),
            conflict_detected: false,
            recommended_stand: None,
            conflict_details: String::new(),
            confidence: MicroModelConfidence::default(),
            execution_time_ms: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// 保障异常处置分流微模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyTriageInput {
    pub anomaly_id: String,
    pub severity: String,
    #[serde(default)]
    pub duration_minutes: Option<i64>,
    #[serde(default)]
    pub affected_flight_id: Option<String>,
    #[serde(default)]
    pub anomaly_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyTriageOutput {
    pub model_id: String,
    pub model_version: String,
    pub should_escalate: bool,
    pub assigned_tier: String,
    pub recommended_action: String,
    pub reasoning: String,
    pub confidence: MicroModelConfidence,
    pub execution_time_ms: u64,
}

impl Default for AnomalyTriageOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl AnomalyTriageOutput {
    pub fn new() -> Self {
        Self {
            model_id: "anomaly_triage_v1".to_string(),
            model_version: "1.0.0".to_string(),
            should_escalate: false,
            assigned_tier: "operator".to_string(),
            recommended_action: "monitor".to_string(),
            reasoning: String::new(),
            confidence: MicroModelConfidence::default(),
            execution_time_ms: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// 运行复盘摘要微模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsBriefingInput {
    pub shift_id: String,
    #[serde(default)]
    pub time_range_start: Option<String>,
    #[serde(default)]
    pub time_range_end: Option<String>,
    #[serde(default)]
    pub include_flight_ids: Vec<String>,
    #[serde(default)]
    pub focus_areas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsBriefingKeyEvent {
    pub event_type: String,
    pub description: String,
    pub severity: String,
    pub related_object_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsBriefingOutput {
    pub model_id: String,
    pub model_version: String,
    pub shift_id: String,
    pub briefing: String,
    pub key_events: Vec<OpsBriefingKeyEvent>,
    pub recommendations: Vec<String>,
    pub confidence: MicroModelConfidence,
    pub execution_time_ms: u64,
}

impl OpsBriefingOutput {
    pub fn new(shift_id: impl Into<String>) -> Self {
        Self {
            model_id: "ops_briefing_v1".to_string(),
            model_version: "1.0.0".to_string(),
            shift_id: shift_id.into(),
            briefing: String::new(),
            key_events: Vec::new(),
            recommendations: Vec::new(),
            confidence: MicroModelConfidence::default(),
            execution_time_ms: 0,
        }
    }
}
