use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use dashmap::DashMap;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::task::JoinHandle;

use crate::schemas::llm_eval_schemas::EvalRunOptionsRequest;

#[derive(Debug, Clone)]
pub(crate) struct ArgExpectation {
    pub(crate) key: String,
    pub(crate) required: bool,
    pub(crate) expected: Option<Value>,
    pub(crate) contains: Option<String>,
    pub(crate) one_of: Option<Vec<Value>>,
    pub(crate) min_value: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct EvalCaseDefinition {
    pub(crate) case_id: String,
    pub(crate) prompt: String,
    pub(crate) expected_tools: Vec<String>,
    pub(crate) expectations: Vec<ArgExpectation>,
    pub(crate) tags: Vec<String>,
    pub(crate) suites: Vec<String>,
    pub(crate) eval_type: String,
    pub(crate) expected_behavior: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeProfile {
    pub(crate) profile_id: String,
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) timeout: f64,
    pub(crate) max_retries: i32,
    pub(crate) retry_delay: f64,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) max_completion_tokens: Option<i32>,
    pub(crate) api_mode: String,
    pub(crate) instructions: Option<String>,
    pub(crate) reasoning_summary: Option<String>,
    pub(crate) store: Option<bool>,
    pub(crate) include: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct EvalProgress {
    pub(crate) completed_attempts: i32,
    pub(crate) total_attempts: i32,
    pub(crate) percentage: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct EvalProfileSnapshot {
    pub(crate) profile_id: String,
    pub(crate) name: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
    pub(crate) timeout: f64,
    pub(crate) max_retries: i32,
    pub(crate) retry_delay: f64,
    pub(crate) api_key_masked: String,
    pub(crate) status: String,
    pub(crate) progress: EvalProgress,
    pub(crate) metrics: Option<Value>,
    pub(crate) cases: Vec<Value>,
    pub(crate) error_message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct EvalJob {
    pub(crate) job_id: String,
    pub(crate) status: String,
    pub(crate) created_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) owner: Value,
    pub(crate) options: EvalRunOptionsRequest,
    pub(crate) suite: Value,
    pub(crate) progress: EvalProgress,
    pub(crate) profiles: Vec<EvalProfileSnapshot>,
    pub(crate) ranking: Vec<Value>,
    pub(crate) error_message: Option<String>,
}

#[derive(Default)]
pub(crate) struct LLMEvalState {
    pub(crate) jobs: DashMap<String, EvalJob>,
    pub(crate) runtime_profiles: DashMap<String, Vec<RuntimeProfile>>,
    pub(crate) runtime_cases: DashMap<String, Vec<EvalCaseDefinition>>,
    pub(crate) cancel_flags: DashMap<String, Arc<AtomicBool>>,
    pub(crate) tasks: DashMap<String, JoinHandle<()>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCaseFile {
    #[serde(default)]
    pub(crate) cases: Vec<RawCaseDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCaseDefinition {
    pub(crate) case_id: String,
    pub(crate) prompt: String,
    #[serde(default)]
    pub(crate) expected_tools: Vec<String>,
    #[serde(default)]
    pub(crate) expectations: Vec<RawArgExpectation>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) suites: Vec<String>,
    pub(crate) eval_type: Option<String>,
    pub(crate) expected_behavior: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawArgExpectation {
    pub(crate) key: String,
    pub(crate) required: Option<bool>,
    pub(crate) expected: Option<Value>,
    pub(crate) contains: Option<String>,
    pub(crate) one_of: Option<Value>,
    pub(crate) min_value: Option<f64>,
}
