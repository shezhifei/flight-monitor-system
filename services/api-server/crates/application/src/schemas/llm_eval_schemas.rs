//! LLM 评测实验室 DTO。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct EvalProfileRequest {
    pub profile_id: Option<String>,
    pub name: Option<String>,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,
    #[serde(default = "default_retry_delay")]
    pub retry_delay: f64,
    pub reasoning_effort: Option<String>,
    pub max_completion_tokens: Option<i32>,
    pub api_mode: Option<String>,
    pub instructions: Option<String>,
    pub reasoning_summary: Option<String>,
    pub store: Option<bool>,
    pub include: Option<Vec<String>>,
}

fn default_timeout() -> f64 {
    30.0
}

fn default_max_retries() -> i32 {
    2
}

fn default_retry_delay() -> f64 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunOptionsRequest {
    #[serde(default = "default_suite")]
    pub suite: String,
    #[serde(default = "default_repeat_count")]
    pub repeat_count: i32,
    #[serde(default = "default_profile_concurrency")]
    pub profile_concurrency: i32,
    #[serde(default = "default_case_concurrency")]
    pub case_concurrency: i32,
    #[serde(default = "default_enable_tool_routing")]
    pub enable_tool_routing: bool,
}

fn default_suite() -> String {
    "quick".to_string()
}

fn default_repeat_count() -> i32 {
    2
}

fn default_profile_concurrency() -> i32 {
    1
}

fn default_case_concurrency() -> i32 {
    1
}

fn default_enable_tool_routing() -> bool {
    true
}

impl Default for EvalRunOptionsRequest {
    fn default() -> Self {
        Self {
            suite: default_suite(),
            repeat_count: default_repeat_count(),
            profile_concurrency: default_profile_concurrency(),
            case_concurrency: default_case_concurrency(),
            enable_tool_routing: default_enable_tool_routing(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LLMEvalJobCreateRequest {
    pub profiles: Vec<EvalProfileRequest>,
    #[serde(default)]
    pub options: EvalRunOptionsRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMEvalJobCreateResponse {
    pub job_id: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LLMEvalCompareResponse {
    pub left: Value,
    pub right: Value,
    pub metric_deltas: HashMap<String, Value>,
    pub case_deltas: Vec<Value>,
    pub regression_cases: Vec<String>,
    pub improvement_cases: Vec<String>,
}
