use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStructuredOutput {
    pub contract_version: String,
    pub run_id: String,
    pub status: String,
    pub answer: String,
    pub reasoning_steps: Vec<ReasoningStep>,
    pub evidence: Vec<OutputEvidence>,
    pub proposals: Vec<OutputProposal>,
    pub limitations: Vec<String>,
    pub metrics: Option<OutputMetrics>,
    /// Token accounting reported by the Python sidecar. Optional because older
    /// producers and heuristic/degraded paths may omit it; when present the
    /// Rust side preserves it rather than discarding the field.
    #[serde(default)]
    pub token_usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub step: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEvidence {
    pub object_type: String,
    pub object_id: String,
    pub field: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputProposal {
    pub proposal_id: Option<String>,
    pub object_type: String,
    pub object_id: String,
    pub action_name: String,
    pub arguments: Value,
    pub risk_level: String,
    pub confidence: f64,
    pub reasoning: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputMetrics {
    pub model: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}
