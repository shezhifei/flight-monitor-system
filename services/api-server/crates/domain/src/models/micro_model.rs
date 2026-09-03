//! 微模型元信息与规格定义
//!
//! 围绕高价值运行任务的微模型规格。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// MicroModelCategory — 微模型分类
// ---------------------------------------------------------------------------

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MicroModelCategory {
    #[default]
    RiskAnalysis = 0,
    Optimization = 1,
    Triage = 2,
    Explanation = 3,
    Generation = 4,
    Prediction = 5,
}

impl MicroModelCategory {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::RiskAnalysis => "risk_analysis",
            Self::Optimization => "optimization",
            Self::Triage => "triage",
            Self::Explanation => "explanation",
            Self::Generation => "generation",
            Self::Prediction => "prediction",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "risk_analysis" | "risk" => Some(Self::RiskAnalysis),
            "optimization" | "optimize" | "opt" => Some(Self::Optimization),
            "triage" | "classify" => Some(Self::Triage),
            "explanation" | "explain" => Some(Self::Explanation),
            "generation" | "generate" => Some(Self::Generation),
            "prediction" | "predict" => Some(Self::Prediction),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ExecutionMode — 执行模式
// ---------------------------------------------------------------------------

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionMode {
    #[default]
    RustDeterministic = 0,
    PythonLLM = 1,
    HybridSolver = 2,
}

impl ExecutionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::RustDeterministic => "rust_deterministic",
            Self::PythonLLM => "python_llm",
            Self::HybridSolver => "hybrid_solver",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "rust_deterministic" | "rust" | "deterministic" => Some(Self::RustDeterministic),
            "python_llm" | "llm" | "python" => Some(Self::PythonLLM),
            "hybrid_solver" | "hybrid" | "solver" => Some(Self::HybridSolver),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// MicroModelSpec — 微模型规格
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModelSpec {
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub category: MicroModelCategory,
    pub execution_mode: ExecutionMode,
    pub ontology_objects: Vec<String>,
    pub input_schema: Value,
    pub output_schema: Value,
    pub advisory_output: bool,
    pub proposal_capable: bool,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub version: String,
    pub evaluation_dataset_id: Option<String>,
    pub allowed_actions: Vec<String>,
    pub feature_flag: Option<String>,
}

impl MicroModelSpec {
    pub fn new(model_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            name: name.into(),
            description: String::new(),
            category: MicroModelCategory::default(),
            execution_mode: ExecutionMode::default(),
            ontology_objects: Vec::new(),
            input_schema: Value::Null,
            output_schema: Value::Null,
            advisory_output: true,
            proposal_capable: false,
            timeout_ms: 30_000,
            max_retries: 2,
            version: "1.0.0".to_string(),
            evaluation_dataset_id: None,
            allowed_actions: Vec::new(),
            feature_flag: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_category(mut self, category: MicroModelCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }

    pub fn with_ontology_objects(mut self, objects: Vec<String>) -> Self {
        self.ontology_objects = objects;
        self
    }

    pub fn with_schemas(mut self, input: Value, output: Value) -> Self {
        self.input_schema = input;
        self.output_schema = output;
        self
    }

    pub fn with_proposal_capable(mut self, capable: bool) -> Self {
        self.proposal_capable = capable;
        self
    }

    pub fn with_allowed_actions(mut self, actions: Vec<String>) -> Self {
        self.allowed_actions = actions;
        self
    }

    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_feature_flag(mut self, flag: impl Into<String>) -> Self {
        self.feature_flag = Some(flag.into());
        self
    }

    pub fn with_evaluation_dataset(mut self, dataset_id: impl Into<String>) -> Self {
        self.evaluation_dataset_id = Some(dataset_id.into());
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

// ---------------------------------------------------------------------------
// MicroModelRegistry — 微模型注册表
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct MicroModelRegistry {
    models: HashMap<String, MicroModelSpec>,
}

impl MicroModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default_models() -> Self {
        let mut registry = Self::new();
        registry.register_flight_risk_model();
        registry.register_dispatch_replan_model();
        registry.register_stand_conflict_model();
        registry.register_anomaly_triage_model();
        registry.register_ops_briefing_model();
        registry
    }

    fn register_flight_risk_model(&mut self) {
        let spec = MicroModelSpec::new("flight_risk_v1", "航班风险摘要")
            .with_description("分析航班的风险因素，生成风险评分、证据追踪和处置建议")
            .with_category(MicroModelCategory::RiskAnalysis)
            .with_execution_mode(ExecutionMode::RustDeterministic)
            .with_ontology_objects(vec![
                "Flight".to_string(),
                "Anomaly".to_string(),
                "DispatchOrder".to_string(),
                "BusinessCase".to_string(),
            ])
            .with_proposal_capable(true)
            .with_allowed_actions(vec![
                "Anomaly.acknowledge".to_string(),
                "Anomaly.escalate".to_string(),
                "Notification.send".to_string(),
                "Flight.add_note".to_string(),
                "Flight.update_estimated_time".to_string(),
            ])
            .with_timeout_ms(5_000)
            .with_version("1.0.0")
            .with_feature_flag("FMS_AI_MICROMODEL_FLIGHT_RISK_ENABLED")
            .with_evaluation_dataset("eval_flight_risk_v1_baseline")
            .with_schemas(
                serde_json::json!({
                    "type": "object",
                    "required": ["flight_id"],
                    "properties": {
                        "flight_id": {"type": "string"},
                        "context_window_minutes": {"type": "integer"},
                        "include_weather": {"type": "boolean"},
                        "include_manual_context": {"type": "boolean"},
                        "risk_ceiling": {"type": "string"}
                    }
                }),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string"},
                        "model_version": {"type": "string"},
                        "flight_id": {"type": "string"},
                        "risk_score": {"type": "integer"},
                        "risk_level": {"type": "string"},
                        "evidence": {"type": "array"},
                        "confidence": {"type": "object"},
                        "proposals": {"type": "array"},
                        "limitations": {"type": "array"},
                        "execution_time_ms": {"type": "integer"}
                    }
                }),
            );
        self.models.insert(spec.model_id.clone(), spec);
    }

    fn register_dispatch_replan_model(&mut self) {
        let spec = MicroModelSpec::new("dispatch_replan_v1", "智能调配重规划")
            .with_description("智能调配重规划微模型，对因大面积延误或备降导致的保障任务缺口进行重算和推荐")
            .with_category(MicroModelCategory::Optimization)
            .with_execution_mode(ExecutionMode::RustDeterministic)
            .with_ontology_objects(vec!["DispatchOrder".to_string(), "Flight".to_string()])
            .with_proposal_capable(true)
            .with_allowed_actions(vec![
                "DispatchOrder.recommend_replan".to_string(),
                "DispatchOrder.assign_slot".to_string(),
                "DispatchOrder.unassign_slot".to_string(),
                "DispatchOrder.add_slot".to_string(),
                "DispatchOrder.remove_slot".to_string(),
            ])
            .with_timeout_ms(10_000)
            .with_version("1.0.0")
            .with_feature_flag("FMS_AI_MICROMODEL_DISPATCH_REPLAN_ENABLED")
            .with_evaluation_dataset("eval_dispatch_replan_v1_baseline")
            .with_schemas(
                serde_json::json!({
                    "type": "object",
                    "required": ["shift_id", "target_time_window", "optimization_objective"],
                    "properties": {
                        "shift_id": {"type": "string"},
                        "target_time_window": {"type": "object", "properties": {
                            "start": {"type": "string", "format": "date-time"},
                            "end": {"type": "string", "format": "date-time"}
                        }},
                        "dispatch_order_ids": {"type": "array", "items": {"type": "string"}},
                        "include_locked": {"type": "boolean"},
                        "optimization_objective": {"type": "string"},
                        "max_proposals": {"type": "integer"}
                    }
                }),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string"},
                        "model_version": {"type": "string"},
                        "shift_id": {"type": "string"},
                        "proposals": {"type": "array"},
                        "optimization_score": {"type": "number"},
                        "execution_time_ms": {"type": "integer"}
                    }
                }),
            );
        self.models.insert(spec.model_id.clone(), spec);
    }

    fn register_stand_conflict_model(&mut self) {
        let spec = MicroModelSpec::new("stand_conflict_v1", "停机位冲突治理")
            .with_description(
                "停机位冲突治理微模型，针对进港提前、出港延误引发的机位分配冲突进行快速诊断并推荐备选机位",
            )
            .with_category(MicroModelCategory::Optimization)
            .with_execution_mode(ExecutionMode::RustDeterministic)
            .with_ontology_objects(vec!["Flight".to_string(), "Stand".to_string()])
            .with_proposal_capable(true)
            .with_allowed_actions(vec!["Flight.change_stand".to_string(), "Stand.reserve".to_string()])
            .with_timeout_ms(5_000)
            .with_version("1.0.0")
            .with_feature_flag("FMS_AI_MICROMODEL_STAND_CONFLICT_ENABLED")
            .with_evaluation_dataset("eval_stand_conflict_v1_baseline")
            .with_schemas(
                serde_json::json!({
                    "type": "object",
                    "required": ["flight_id", "current_stand_id"],
                    "properties": {
                        "flight_id": {"type": "string"},
                        "current_stand_id": {"type": "string"},
                        "conflict_flight_id": {"type": "string"},
                        "conflict_window_minutes": {"type": "integer"}
                    }
                }),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string"},
                        "model_version": {"type": "string"},
                        "conflict_detected": {"type": "boolean"},
                        "recommended_stand": {"type": "string"},
                        "conflict_details": {"type": "string"},
                        "confidence": {"type": "object"},
                        "execution_time_ms": {"type": "integer"}
                    }
                }),
            );
        self.models.insert(spec.model_id.clone(), spec);
    }

    fn register_anomaly_triage_model(&mut self) {
        let spec = MicroModelSpec::new("anomaly_triage_v1", "保障异常处置分流")
            .with_description(
                "保障异常处置分流微模型，当发生超时或KPI异常时，根据历史经验和影响度等级自动决策通报和升级路径",
            )
            .with_category(MicroModelCategory::Triage)
            .with_execution_mode(ExecutionMode::RustDeterministic)
            .with_ontology_objects(vec!["Anomaly".to_string(), "Notification".to_string()])
            .with_proposal_capable(true)
            .with_allowed_actions(vec![
                "Anomaly.acknowledge".to_string(),
                "Anomaly.escalate".to_string(),
                "Notification.send".to_string(),
            ])
            .with_timeout_ms(5_000)
            .with_version("1.0.0")
            .with_feature_flag("FMS_AI_MICROMODEL_ANOMALY_TRIAGE_ENABLED")
            .with_evaluation_dataset("eval_anomaly_triage_v1_baseline")
            .with_schemas(
                serde_json::json!({
                    "type": "object",
                    "required": ["anomaly_id", "severity"],
                    "properties": {
                        "anomaly_id": {"type": "string"},
                        "severity": {"type": "string"},
                        "duration_minutes": {"type": "integer"},
                        "affected_flight_id": {"type": "string"},
                        "anomaly_type": {"type": "string"}
                    }
                }),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string"},
                        "model_version": {"type": "string"},
                        "should_escalate": {"type": "boolean"},
                        "assigned_tier": {"type": "string"},
                        "recommended_action": {"type": "string"},
                        "confidence": {"type": "object"},
                        "execution_time_ms": {"type": "integer"}
                    }
                }),
            );
        self.models.insert(spec.model_id.clone(), spec);
    }

    fn register_ops_briefing_model(&mut self) {
        let spec = MicroModelSpec::new("ops_briefing_v1", "运行复盘摘要生成")
            .with_description("运行复盘摘要生成微模型，用于在班组交接班或特定保障任务结束时，对保障时序 and 关键冲突生成概括性复盘建议")
            .with_category(MicroModelCategory::Generation)
            .with_execution_mode(ExecutionMode::RustDeterministic)
            .with_ontology_objects(vec![
                "Flight".to_string(),
                "Todo".to_string(),
            ])
            .with_proposal_capable(true)
            .with_allowed_actions(vec![
                "Todo.create".to_string(),
                "Flight.add_note".to_string(),
            ])
            .with_timeout_ms(15_000)
            .with_version("1.0.0")
            .with_feature_flag("FMS_AI_MICROMODEL_OPS_BRIEFING_ENABLED")
            .with_evaluation_dataset("eval_ops_briefing_v1_baseline")
            .with_schemas(
                serde_json::json!({
                    "type": "object",
                    "required": ["shift_id"],
                    "properties": {
                        "shift_id": {"type": "string"},
                        "time_range_start": {"type": "string", "format": "date-time"},
                        "time_range_end": {"type": "string", "format": "date-time"},
                        "include_flight_ids": {"type": "array", "items": {"type": "string"}},
                        "focus_areas": {"type": "array", "items": {"type": "string"}}
                    }
                }),
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model_id": {"type": "string"},
                        "model_version": {"type": "string"},
                        "briefing": {"type": "string"},
                        "key_events": {"type": "array"},
                        "recommendations": {"type": "array"},
                        "execution_time_ms": {"type": "integer"}
                    }
                }),
            );
        self.models.insert(spec.model_id.clone(), spec);
    }

    pub fn register(&mut self, spec: MicroModelSpec) {
        self.models.insert(spec.model_id.clone(), spec);
    }

    pub fn get(&self, model_id: &str) -> Option<&MicroModelSpec> {
        self.models.get(model_id)
    }

    pub fn list_all(&self) -> Vec<&MicroModelSpec> {
        self.models.values().collect()
    }

    pub fn list_by_category(&self, category: MicroModelCategory) -> Vec<&MicroModelSpec> {
        self.models.values().filter(|m| m.category == category).collect()
    }

    pub fn list_proposal_capable(&self) -> Vec<&MicroModelSpec> {
        self.models.values().filter(|m| m.proposal_capable).collect()
    }

    pub fn is_registered(&self, model_id: &str) -> bool {
        self.models.contains_key(model_id)
    }

    /// Check if a model's feature flag is enabled via environment variable.
    /// Returns false if the model is not registered or its feature flag is not set/enabled.
    pub fn is_enabled(&self, model_id: &str) -> bool {
        self.models
            .get(model_id)
            .and_then(|spec| spec.feature_flag.as_deref())
            .map(|flag| {
                std::env::var(flag)
                    .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// MicroModelExecutionResult — 执行结果
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModelExecutionResult {
    pub model_id: String,
    pub model_version: String,
    pub execution_id: String,
    pub job_id: String,
    pub run_id: String,
    pub input: Value,
    pub output: Value,
    pub execution_time_ms: u64,
    pub status: MicroModelExecutionStatus,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicroModelExecutionStatus {
    Success = 0,
    Failed = 1,
    Timeout = 2,
    ValidationError = 3,
}

impl MicroModelExecutionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::ValidationError => "validation_error",
        }
    }
}

// ---------------------------------------------------------------------------
// MicroModelEvalRecord — 评测记录
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModelEvalRecord {
    pub eval_id: String,
    pub model_id: String,
    pub model_version: String,
    pub job_id: String,
    pub run_id: String,
    pub input_snapshot: Value,
    pub output: Value,
    pub execution_time_ms: u64,
    pub proposals_generated: usize,
    pub proposals_approved: usize,
    pub proposals_rejected: usize,
    pub user_rating: Option<i32>,
    pub feedback: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl MicroModelEvalRecord {
    pub fn new(eval_id: impl Into<String>, result: &MicroModelExecutionResult) -> Self {
        let now = chrono::Utc::now();
        Self {
            eval_id: eval_id.into(),
            model_id: result.model_id.clone(),
            model_version: result.model_version.clone(),
            job_id: result.job_id.clone(),
            run_id: result.run_id.clone(),
            input_snapshot: result.input.clone(),
            output: result.output.clone(),
            execution_time_ms: result.execution_time_ms,
            proposals_generated: 0,
            proposals_approved: 0,
            proposals_rejected: 0,
            user_rating: None,
            feedback: None,
            created_at: now,
        }
    }

    pub fn proposal_outcome(mut self, generated: usize, approved: usize, rejected: usize) -> Self {
        self.proposals_generated = generated;
        self.proposals_approved = approved;
        self.proposals_rejected = rejected;
        self
    }

    pub fn with_feedback(mut self, rating: i32, feedback: impl Into<String>) -> Self {
        self.user_rating = Some(rating);
        self.feedback = Some(feedback.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec(id: &str) -> MicroModelSpec {
        MicroModelSpec::new(id, format!("Model {id}"))
            .with_category(MicroModelCategory::Triage)
            .with_proposal_capable(id.ends_with("-capable"))
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = MicroModelRegistry::new();
        reg.register(sample_spec("m1"));
        assert!(reg.is_registered("m1"));
        assert!(reg.get("m1").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[test]
    fn registry_list_all() {
        let mut reg = MicroModelRegistry::new();
        reg.register(sample_spec("m1"));
        reg.register(sample_spec("m2"));
        assert_eq!(reg.list_all().len(), 2);
    }

    #[test]
    fn registry_list_by_category() {
        let mut reg = MicroModelRegistry::new();
        reg.register(sample_spec("m1"));
        reg.register(MicroModelSpec::new("m2", "M2").with_category(MicroModelCategory::Generation));
        let triage = reg.list_by_category(MicroModelCategory::Triage);
        assert_eq!(triage.len(), 1);
        assert_eq!(triage[0].model_id, "m1");
    }

    #[test]
    fn registry_list_proposal_capable() {
        let mut reg = MicroModelRegistry::new();
        reg.register(sample_spec("m1"));
        reg.register(sample_spec("m2-capable"));
        assert_eq!(reg.list_proposal_capable().len(), 1);
    }

    #[test]
    fn registry_is_enabled_reads_env() {
        let mut reg = MicroModelRegistry::new();
        reg.register(MicroModelSpec::new("m1", "M1").with_feature_flag("TEST_MODEL_FLAG"));
        assert!(!reg.is_enabled("m1"));
        assert!(!reg.is_enabled("unknown"));

        std::env::set_var("TEST_MODEL_FLAG", "true");
        assert!(reg.is_enabled("m1"));
        std::env::remove_var("TEST_MODEL_FLAG");
    }

    #[test]
    fn default_registry_has_builtin_models() {
        let reg = MicroModelRegistry::with_default_models();
        assert!(!reg.list_all().is_empty());
    }

    #[test]
    fn micro_model_category_from_str_loose() {
        assert_eq!(
            MicroModelCategory::from_str_loose("triage"),
            Some(MicroModelCategory::Triage)
        );
        assert_eq!(
            MicroModelCategory::from_str_loose("generation"),
            Some(MicroModelCategory::Generation)
        );
        assert_eq!(MicroModelCategory::from_str_loose("bogus"), None);
    }

    #[test]
    fn execution_mode_from_str_loose() {
        assert_eq!(
            ExecutionMode::from_str_loose("rust"),
            Some(ExecutionMode::RustDeterministic)
        );
        assert_eq!(
            ExecutionMode::from_str_loose("python_llm"),
            Some(ExecutionMode::PythonLLM)
        );
    }
}
