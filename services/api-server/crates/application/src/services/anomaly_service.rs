//! 异常告警应用服务。

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::anomaly::*;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::flight_monitor_row_repository::FlightMonitorRowRepository;
use fms_domain::ports::flight_repository::FlightRepository;

/// 异常告警服务
pub struct AnomalyService {
    repo: Arc<dyn AnomalyRepository + Send + Sync>,
    flight_repo: Option<Arc<dyn FlightRepository + Send + Sync>>,
    monitor_rows: Option<Arc<dyn FlightMonitorRowRepository + Send + Sync>>,
}

impl AnomalyService {
    pub fn new(repo: Arc<dyn AnomalyRepository + Send + Sync>) -> Self {
        Self {
            repo,
            flight_repo: None,
            monitor_rows: None,
        }
    }
}

impl AnomalyService {
    pub fn with_flight_repository(mut self, flight_repo: Arc<dyn FlightRepository + Send + Sync>) -> Self {
        self.flight_repo = Some(flight_repo);
        self
    }

    pub fn with_monitor_row_repository(
        mut self,
        monitor_rows: Arc<dyn FlightMonitorRowRepository + Send + Sync>,
    ) -> Self {
        self.monitor_rows = Some(monitor_rows);
        self
    }

    /// 查询异常列表 — 可按状态、类型筛选
    pub async fn list_anomalies(
        &self,
        status: Option<&str>,
        anomaly_type: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AnomalyResponse>, DomainError> {
        let anomalies = self.load_anomalies(status).await?;
        let page: Vec<_> = anomalies
            .into_iter()
            .filter(|anomaly| matches_anomaly_type(anomaly, anomaly_type))
            .filter(|anomaly| matches_date_range(anomaly, start_date, end_date))
            .skip(offset as usize)
            .take(limit as usize)
            .map(|a| anomaly_to_response(&a))
            .collect();
        Ok(page)
    }

    /// 获取单个异常
    pub async fn get_anomaly(&self, anomaly_id: &str) -> Result<Option<AnomalyResponse>, DomainError> {
        let a = self.repo.find_by_id(anomaly_id).await?;
        Ok(a.map(|a| anomaly_to_response(&a)))
    }

    /// 按航班查询异常
    pub async fn list_by_flight(&self, flight_id: &str) -> Result<Vec<AnomalyResponse>, DomainError> {
        let items = self.repo.find_by_flight(flight_id).await?;
        Ok(items.iter().map(anomaly_to_response).collect())
    }

    /// 确认异常
    pub async fn acknowledge(&self, anomaly_id: &str) -> Result<bool, DomainError> {
        let changed = self.repo.acknowledge(anomaly_id).await?;
        if changed {
            self.refresh_monitor_anomaly_flag(anomaly_id).await?;
        }
        Ok(changed)
    }

    pub async fn escalate(&self, anomaly_id: &str) -> Result<bool, DomainError> {
        self.repo.escalate(anomaly_id).await
    }

    pub async fn resolve(&self, anomaly_id: &str) -> Result<bool, DomainError> {
        let changed = self.repo.resolve(anomaly_id).await?;
        if changed {
            self.refresh_monitor_anomaly_flag(anomaly_id).await?;
        }
        Ok(changed)
    }

    /// 创建异常
    pub async fn create_anomaly(&self, dto: AnomalyCreate) -> Result<AnomalyResponse, DomainError> {
        let now = Utc::now();
        let subject_type = dto
            .subject_type
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Flight".to_string());
        let subject_id = dto
            .subject_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| dto.flight_id.clone());
        if subject_id.trim().is_empty() {
            return Err(DomainError::ValidationError("subject_id 不能为空".into()));
        }
        let flight_id = if subject_type.eq_ignore_ascii_case("Flight") {
            subject_id.clone()
        } else {
            dto.flight_id.clone()
        };
        let anomaly = Anomaly {
            anomaly_id: ulid::Ulid::new().to_string(),
            subject_type,
            subject_id,
            flight_id,
            anomaly_type: parse_type(&dto.anomaly_type),
            severity: parse_sev(&dto.severity),
            title: dto.title,
            description: dto.description,
            status: AnomalyStatus::Open,
            detected_at: now,
            resolved_at: None,
            escalation_level: 0,
            last_escalated_at: None,
            linked_todo_id: None,
            rule_id: dto.rule_id,
            context_data: dto.context_data.unwrap_or_default(),
            created_at: now,
            updated_at: now,
        };
        self.repo.save(&anomaly).await?;
        if let Some(repo) = &self.monitor_rows {
            repo.refresh_anomaly_flag(&anomaly.flight_id).await?;
        }
        Ok(anomaly_to_response(&anomaly))
    }

    async fn refresh_monitor_anomaly_flag(&self, anomaly_id: &str) -> Result<(), DomainError> {
        let Some(monitor_rows) = &self.monitor_rows else {
            return Ok(());
        };
        let Some(anomaly) = self.repo.find_by_id(anomaly_id).await? else {
            return Ok(());
        };
        monitor_rows.refresh_anomaly_flag(&anomaly.flight_id).await
    }

    pub async fn evaluate_flight(&self, flight_id: &str) -> Result<Vec<AnomalyResponse>, DomainError> {
        let flight_id = flight_id.trim();
        if flight_id.is_empty() {
            return Ok(Vec::new());
        }

        let Some(flight_repo) = self.flight_repo.as_ref() else {
            return Ok(Vec::new());
        };
        let Some(flight) = flight_repo.find_by_id(flight_id).await? else {
            return Ok(Vec::new());
        };

        self.seed_default_rules_if_empty().await?;
        let rules = self.repo.list_rules(true).await?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }

        let existing = self.repo.find_by_flight(flight_id).await?;
        let all_flights = flight_repo.find_all(2000, 0).await?;
        let mut created = Vec::new();

        for rule in rules.into_iter().filter(|rule| rule.enabled) {
            let Some(create_dto) = self.build_event_driven_anomaly(&flight, &all_flights, &existing, &rule)? else {
                continue;
            };
            created.push(self.create_anomaly(create_dto).await?);
        }

        Ok(created)
    }

    /// 统计
    pub async fn get_stats(
        &self,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<AnomalyStatsResponse, DomainError> {
        let anomalies = self
            .load_anomalies(None)
            .await?
            .into_iter()
            .filter(|anomaly| matches_date_range(anomaly, start_date, end_date))
            .collect::<Vec<_>>();

        let total = anomalies.len() as i64;
        let open = anomalies
            .iter()
            .filter(|anomaly| anomaly.status == AnomalyStatus::Open)
            .count() as i64;
        let acknowledged = anomalies
            .iter()
            .filter(|anomaly| anomaly.status == AnomalyStatus::Acknowledged)
            .count() as i64;
        let resolved = anomalies
            .iter()
            .filter(|anomaly| anomaly.status == AnomalyStatus::Resolved)
            .count() as i64;
        let critical = anomalies
            .iter()
            .filter(|anomaly| anomaly.severity == AnomalySeverity::Critical)
            .count() as i64;
        let escalated = anomalies
            .iter()
            .filter(|anomaly| anomaly.escalation_level > 0 || anomaly.last_escalated_at.is_some())
            .count() as i64;

        Ok(AnomalyStatsResponse {
            total,
            open,
            acknowledged,
            resolved,
            critical,
            escalated,
        })
    }

    async fn load_anomalies(&self, status: Option<&str>) -> Result<Vec<Anomaly>, DomainError> {
        let mut anomalies = match status.and_then(parse_status_filter) {
            Some(status) => self.repo.find_by_status(status).await?,
            None => {
                let mut combined = Vec::new();
                combined.extend(self.repo.find_by_status(AnomalyStatus::Open).await?);
                combined.extend(self.repo.find_by_status(AnomalyStatus::Acknowledged).await?);
                combined.extend(self.repo.find_by_status(AnomalyStatus::Resolved).await?);
                combined
            }
        };

        anomalies.sort_by(|left, right| right.detected_at.cmp(&left.detected_at));
        Ok(anomalies)
    }

    pub async fn list_rules(&self, enabled_only: bool) -> Result<Vec<AnomalyRuleResponse>, DomainError> {
        self.seed_default_rules_if_empty().await?;
        let items = self.repo.list_rules(enabled_only).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn get_rule(&self, rule_id: &str) -> Result<Option<AnomalyRuleResponse>, DomainError> {
        self.seed_default_rules_if_empty().await?;
        Ok(self.repo.get_rule(rule_id).await?.map(Into::into))
    }

    pub async fn create_rule(&self, input: AnomalyRuleCreate) -> Result<AnomalyRuleResponse, DomainError> {
        self.seed_default_rules_if_empty().await?;
        let mut rule = anomaly_rule_from_create(input)?;
        if let Some(existing) = self.repo.get_rule(&rule.rule_id).await? {
            rule.created_at = existing.created_at;
        }
        Ok(self.repo.upsert_rule(&rule).await?.into())
    }

    pub async fn update_rule(
        &self,
        rule_id: &str,
        input: AnomalyRuleUpdate,
    ) -> Result<Option<AnomalyRuleResponse>, DomainError> {
        self.seed_default_rules_if_empty().await?;
        let Some(mut rule) = self.repo.get_rule(rule_id).await? else {
            return Ok(None);
        };
        apply_rule_update(&mut rule, input)?;
        Ok(Some(self.repo.upsert_rule(&rule).await?.into()))
    }

    async fn seed_default_rules_if_empty(&self) -> Result<(), DomainError> {
        if !self.repo.list_rules(false).await?.is_empty() {
            return Ok(());
        }
        for rule in default_rules() {
            self.repo.upsert_rule(&rule).await?;
        }
        Ok(())
    }

    fn build_event_driven_anomaly(
        &self,
        flight: &fms_domain::models::flight::Flight,
        all_flights: &[fms_domain::models::flight::Flight],
        existing: &[Anomaly],
        rule: &AnomalyRule,
    ) -> Result<Option<AnomalyCreate>, DomainError> {
        match rule.rule_id.trim() {
            "gate_stand_conflict" => {
                if has_open_rule_signature(existing, AnomalyType::GateStandConflict, Some(rule.rule_id.as_str())) {
                    return Ok(None);
                }
                Ok(build_gate_stand_conflict_anomaly(flight, all_flights, rule))
            }
            _ => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnomalyCreate {
    #[serde(default)]
    pub flight_id: String,
    pub subject_type: Option<String>,
    pub subject_id: Option<String>,
    pub anomaly_type: String,
    pub severity: String,
    pub title: String,
    pub description: Option<String>,
    pub rule_id: Option<String>,
    pub context_data: Option<std::collections::HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnomalyResponse {
    pub anomaly_id: String,
    pub subject_type: String,
    pub subject_id: String,
    pub flight_id: String,
    pub anomaly_type: String,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub description: Option<String>,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub escalation_level: i32,
    pub last_escalated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub linked_todo_id: Option<String>,
    pub rule_id: Option<String>,
    pub context_data: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnomalyStatsResponse {
    pub total: i64,
    pub open: i64,
    pub acknowledged: i64,
    pub resolved: i64,
    pub critical: i64,
    pub escalated: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnomalyRuleCreate {
    pub rule_id: String,
    pub rule_type: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
    pub severity: String,
    #[serde(default)]
    pub auto_create_todo: bool,
    pub todo_priority: Option<String>,
    #[serde(default)]
    pub escalation_intervals: Vec<i64>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AnomalyRuleUpdate {
    pub enabled: Option<bool>,
    pub config: Option<HashMap<String, serde_json::Value>>,
    pub severity: Option<String>,
    pub auto_create_todo: Option<bool>,
    pub todo_priority: Option<String>,
    pub escalation_intervals: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnomalyRuleResponse {
    pub rule_id: String,
    pub rule_type: String,
    pub name: String,
    pub enabled: bool,
    pub config: HashMap<String, serde_json::Value>,
    pub severity: String,
    pub auto_create_todo: bool,
    pub todo_priority: Option<String>,
    pub escalation_intervals: Vec<i64>,
}

fn anomaly_to_response(a: &Anomaly) -> AnomalyResponse {
    AnomalyResponse {
        anomaly_id: a.anomaly_id.clone(),
        subject_type: a.subject_type.clone(),
        subject_id: a.subject_id.clone(),
        flight_id: a.flight_id.clone(),
        anomaly_type: anomaly_type_value(a.anomaly_type).to_string(),
        severity: a.severity.as_ref().to_string(),
        status: a.status.as_ref().to_string(),
        title: a.title.clone(),
        description: a.description.clone(),
        detected_at: a.detected_at,
        resolved_at: a.resolved_at,
        escalation_level: a.escalation_level,
        last_escalated_at: a.last_escalated_at,
        linked_todo_id: a.linked_todo_id.clone(),
        rule_id: a.rule_id.clone(),
        context_data: a.context_data.clone(),
        created_at: a.created_at,
        updated_at: a.updated_at,
    }
}

fn parse_status_filter(value: &str) -> Option<AnomalyStatus> {
    match value.trim().to_lowercase().as_str() {
        "open" => Some(AnomalyStatus::Open),
        "acknowledged" => Some(AnomalyStatus::Acknowledged),
        "resolved" => Some(AnomalyStatus::Resolved),
        _ => None,
    }
}

fn anomaly_type_value(value: AnomalyType) -> &'static str {
    match value {
        AnomalyType::ServiceNodeTimeout => "service_node_timeout",
        AnomalyType::GateStandConflict => "gate_stand_conflict",
        AnomalyType::KpiDegradation => "kpi_degradation",
        AnomalyType::AiRisk => "ai_risk",
        AnomalyType::DispatchIssue => "dispatch_issue",
    }
}

fn matches_anomaly_type(anomaly: &Anomaly, anomaly_type: Option<&str>) -> bool {
    let Some(anomaly_type) = anomaly_type else {
        return true;
    };
    anomaly_type_value(anomaly.anomaly_type).eq_ignore_ascii_case(anomaly_type.trim())
}

fn matches_date_range(anomaly: &Anomaly, start_date: Option<DateTime<Utc>>, end_date: Option<DateTime<Utc>>) -> bool {
    if let Some(start_date) = start_date {
        if anomaly.detected_at < start_date {
            return false;
        }
    }
    if let Some(end_date) = end_date {
        if anomaly.detected_at > end_date {
            return false;
        }
    }
    true
}

fn parse_type(s: &str) -> AnomalyType {
    match s {
        "gate_stand_conflict" => AnomalyType::GateStandConflict,
        "kpi_degradation" => AnomalyType::KpiDegradation,
        "ai_risk" => AnomalyType::AiRisk,
        "dispatch_issue" => AnomalyType::DispatchIssue,
        _ => AnomalyType::ServiceNodeTimeout,
    }
}

fn parse_sev(s: &str) -> AnomalySeverity {
    match s {
        "medium" => AnomalySeverity::Medium,
        "high" => AnomalySeverity::High,
        "critical" => AnomalySeverity::Critical,
        _ => AnomalySeverity::Low,
    }
}

impl From<AnomalyRule> for AnomalyRuleResponse {
    fn from(value: AnomalyRule) -> Self {
        let escalation_intervals = value.normalized_intervals();
        Self {
            rule_id: value.rule_id,
            rule_type: value.rule_type,
            name: value.name,
            enabled: value.enabled,
            config: value.config,
            severity: value.severity,
            auto_create_todo: value.auto_create_todo,
            todo_priority: Some(value.todo_priority),
            escalation_intervals,
        }
    }
}

fn anomaly_rule_from_create(input: AnomalyRuleCreate) -> Result<AnomalyRule, DomainError> {
    let rule_id = input.rule_id.trim().to_string();
    if rule_id.is_empty() {
        return Err(DomainError::ValidationError("rule_id 不能为空".into()));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(DomainError::ValidationError("name 不能为空".into()));
    }
    let rule_type = input.rule_type.trim().to_string();
    if rule_type.is_empty() {
        return Err(DomainError::ValidationError("rule_type 不能为空".into()));
    }
    let severity = normalize_rule_severity(&input.severity)?;
    let now = Utc::now();
    Ok(AnomalyRule {
        rule_id,
        rule_type,
        name,
        enabled: input.enabled,
        config: input.config,
        severity,
        auto_create_todo: input.auto_create_todo,
        todo_priority: normalize_todo_priority(input.todo_priority),
        escalation_intervals: normalize_intervals(input.escalation_intervals),
        created_at: now,
        updated_at: now,
    })
}

fn apply_rule_update(rule: &mut AnomalyRule, input: AnomalyRuleUpdate) -> Result<(), DomainError> {
    if let Some(enabled) = input.enabled {
        rule.enabled = enabled;
    }
    if let Some(config) = input.config {
        rule.config = config;
    }
    if let Some(severity) = input.severity {
        rule.severity = normalize_rule_severity(&severity)?;
    }
    if let Some(auto_create_todo) = input.auto_create_todo {
        rule.auto_create_todo = auto_create_todo;
    }
    if let Some(todo_priority) = input.todo_priority {
        rule.todo_priority = normalize_todo_priority(Some(todo_priority));
    }
    if let Some(intervals) = input.escalation_intervals {
        rule.escalation_intervals = normalize_intervals(intervals);
    }
    rule.updated_at = Utc::now();
    Ok(())
}

fn normalize_rule_severity(value: &str) -> Result<String, DomainError> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "low" | "medium" | "high" | "critical" => Ok(normalized),
        _ => Err(DomainError::ValidationError(format!("Invalid severity: {value}"))),
    }
}

fn normalize_intervals(values: Vec<i64>) -> Vec<i64> {
    let mut items = values.into_iter().filter(|value| *value >= 0).collect::<Vec<_>>();
    items.sort_unstable();
    items.dedup();
    if items.is_empty() {
        return vec![5, 15, 30];
    }
    items
}

fn normalize_todo_priority(value: Option<String>) -> String {
    let normalized = value.unwrap_or_default().trim().to_string();
    if normalized.is_empty() {
        return "HIGH".to_string();
    }
    normalized
}

fn default_true() -> bool {
    true
}

fn build_gate_stand_conflict_anomaly(
    flight: &fms_domain::models::flight::Flight,
    all_flights: &[fms_domain::models::flight::Flight],
    rule: &AnomalyRule,
) -> Option<AnomalyCreate> {
    let scheduled_departure = flight.scheduled_departure?;
    let gate = flight
        .gate
        .as_ref()
        .map(|value| value.0.trim())
        .filter(|value| !value.is_empty());
    let stand = flight
        .stand
        .as_ref()
        .map(|value| value.0.trim())
        .filter(|value| !value.is_empty());
    if gate.is_none() && stand.is_none() {
        return None;
    }

    let conflict_window_minutes = resolve_conflict_window_minutes(rule);
    let mut best_conflict: Option<(String, String, String, f64)> = None;

    for other in all_flights {
        if other.flight_id.0 == flight.flight_id.0 {
            continue;
        }
        let Some(other_departure) = other.scheduled_departure else {
            continue;
        };

        let shared_resource = match (
            gate,
            other
                .gate
                .as_ref()
                .map(|value| value.0.trim())
                .filter(|value| !value.is_empty()),
            stand,
            other
                .stand
                .as_ref()
                .map(|value| value.0.trim())
                .filter(|value| !value.is_empty()),
        ) {
            (Some(current_gate), Some(other_gate), _, _) if current_gate == other_gate => {
                Some(("gate".to_string(), current_gate.to_string()))
            }
            (_, _, Some(current_stand), Some(other_stand)) if current_stand == other_stand => {
                Some(("stand".to_string(), current_stand.to_string()))
            }
            _ => None,
        };

        let Some((resource_type, resource_value)) = shared_resource else {
            continue;
        };

        let window_minutes = (scheduled_departure - other_departure).num_seconds().unsigned_abs() as f64 / 60.0;
        if window_minutes > conflict_window_minutes as f64 {
            continue;
        }

        let candidate = (
            flight_number(other),
            resource_type,
            resource_value,
            (window_minutes * 100.0).round() / 100.0,
        );
        if best_conflict
            .as_ref()
            .map(|(_, _, _, existing_window)| candidate.3 < *existing_window)
            .unwrap_or(true)
        {
            best_conflict = Some(candidate);
        }
    }

    let Some((other_flight_number, resource_type, resource_value, window_minutes)) = best_conflict else {
        return None;
    };

    let flight_number = flight_number(flight);
    let mut context_data = HashMap::new();
    context_data.insert("flight_number".to_string(), json!(flight_number));
    context_data.insert("other_flight_number".to_string(), json!(other_flight_number));
    context_data.insert("resource_type".to_string(), json!(resource_type));
    context_data.insert("resource_value".to_string(), json!(resource_value));
    context_data.insert("window_minutes".to_string(), json!(window_minutes));
    context_data.insert("threshold_minutes".to_string(), json!(conflict_window_minutes));

    Some(AnomalyCreate {
        flight_id: flight.flight_id.0.clone(),
        subject_type: None,
        subject_id: None,
        anomaly_type: "gate_stand_conflict".to_string(),
        severity: rule.severity.clone(),
        title: format!("Gate/Stand conflict: {flight_number}"),
        description: Some(format!(
            "Flight {flight_number} conflicts with {other_flight_number} on shared {resource_type} {resource_value}. Departure window overlap is {window_minutes:.2} minutes."
        )),
        rule_id: Some(rule.rule_id.clone()),
        context_data: Some(context_data),
    })
}

fn resolve_conflict_window_minutes(rule: &AnomalyRule) -> i64 {
    rule.config
        .get("conflict_window_minutes")
        .and_then(|value| value.as_i64())
        .or_else(|| rule.config.get("threshold").and_then(|value| value.as_i64()))
        .unwrap_or(45)
        .max(1)
}

fn has_open_rule_signature(existing: &[Anomaly], anomaly_type: AnomalyType, rule_id: Option<&str>) -> bool {
    existing.iter().any(|anomaly| {
        anomaly.status != AnomalyStatus::Resolved
            && anomaly.anomaly_type == anomaly_type
            && anomaly.rule_id.as_deref() == rule_id
    })
}

fn flight_number(flight: &fms_domain::models::flight::Flight) -> String {
    flight
        .get_flight_numbers()
        .into_iter()
        .next()
        .or_else(|| flight.flight_number.as_ref().map(|value| value.0.clone()))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn default_rules() -> Vec<AnomalyRule> {
    let now = Utc::now();
    vec![
        AnomalyRule {
            rule_id: "kpi_degradation".to_string(),
            rule_type: "kpi".to_string(),
            name: "KPI 降级".to_string(),
            enabled: true,
            config: HashMap::from([("threshold".to_string(), serde_json::json!(0.85))]),
            severity: "high".to_string(),
            auto_create_todo: true,
            todo_priority: "high".to_string(),
            escalation_intervals: vec![15, 30, 60],
            created_at: now,
            updated_at: now,
        },
        AnomalyRule {
            rule_id: "gate_stand_conflict".to_string(),
            rule_type: "operation".to_string(),
            name: "机位冲突".to_string(),
            enabled: true,
            config: HashMap::from([("threshold".to_string(), serde_json::json!(0))]),
            severity: "critical".to_string(),
            auto_create_todo: true,
            todo_priority: "critical".to_string(),
            escalation_intervals: vec![5, 10, 20],
            created_at: now,
            updated_at: now,
        },
        AnomalyRule {
            rule_id: "service_node_timeout".to_string(),
            rule_type: "service".to_string(),
            name: "服务节点超时".to_string(),
            enabled: true,
            config: HashMap::from([("threshold".to_string(), serde_json::json!(300))]),
            severity: "medium".to_string(),
            auto_create_todo: false,
            todo_priority: "medium".to_string(),
            escalation_intervals: vec![30, 60],
            created_at: now,
            updated_at: now,
        },
    ]
}
