//! 异常监控领域模型
//!
//! 对应 Python `src/domain/models/anomaly.py` + `anomaly_rule.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    ServiceNodeTimeout,
    GateStandConflict,
    KpiDegradation,
    AiRisk,
    DispatchIssue,
}

impl AsRef<str> for AnomalyType {
    fn as_ref(&self) -> &str {
        match self {
            Self::ServiceNodeTimeout => "service_node_timeout",
            Self::GateStandConflict => "gate_stand_conflict",
            Self::KpiDegradation => "kpi_degradation",
            Self::AiRisk => "ai_risk",
            Self::DispatchIssue => "dispatch_issue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl AsRef<str> for AnomalySeverity {
    fn as_ref(&self) -> &str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyStatus {
    Open,
    Acknowledged,
    Resolved,
}

impl AsRef<str> for AnomalyStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::Acknowledged => "acknowledged",
            Self::Resolved => "resolved",
        }
    }
}

/// 异常实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    pub anomaly_id: String,
    pub flight_id: String,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub title: String,
    pub description: Option<String>,
    #[serde(default = "default_open")]
    pub status: AnomalyStatus,
    pub detected_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub escalation_level: i32,
    pub last_escalated_at: Option<DateTime<Utc>>,
    pub linked_todo_id: Option<String>,
    pub rule_id: Option<String>,
    #[serde(default)]
    pub context_data: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_open() -> AnomalyStatus {
    AnomalyStatus::Open
}

impl Anomaly {
    pub fn acknowledge(&mut self) {
        if self.status != AnomalyStatus::Resolved {
            self.status = AnomalyStatus::Acknowledged;
            self.updated_at = Utc::now();
        }
    }

    pub fn resolve(&mut self) {
        self.status = AnomalyStatus::Resolved;
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn escalate(&mut self) {
        self.escalation_level += 1;
        self.last_escalated_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

/// 异常规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyRule {
    pub rule_id: String,
    pub rule_type: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
    pub severity: String,
    #[serde(default = "default_true")]
    pub auto_create_todo: bool,
    pub todo_priority: String,
    #[serde(default = "default_rule_intervals")]
    pub escalation_intervals: Vec<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

fn default_rule_intervals() -> Vec<i64> {
    vec![5, 15, 30]
}

impl AnomalyRule {
    pub fn normalized_intervals(&self) -> Vec<i64> {
        let mut normalized = self
            .escalation_intervals
            .iter()
            .copied()
            .filter(|value| *value > 0)
            .collect::<Vec<_>>();
        if normalized.is_empty() {
            return vec![5, 15, 30];
        }
        normalized.sort_unstable();
        normalized.dedup();
        normalized
    }
}
