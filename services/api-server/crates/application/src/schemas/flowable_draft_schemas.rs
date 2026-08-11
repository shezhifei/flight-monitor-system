use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDraftSourceMeta {
    pub filename: String,
    pub extension: String,
    pub parsed_characters: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowableProcessDraftData {
    pub draft_bpmn_xml: String,
    pub draft_summary_markdown: String,
    #[serde(default)]
    pub extracted_requirements: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub source_meta: ProcessDraftSourceMeta,
    pub generated_at: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowableDraftAssistantContext {
    pub process_key: Option<String>,
    pub process_name: Option<String>,
    pub case_type_code: Option<String>,
    pub locale: Option<String>,
    pub source_meta: Option<serde_json::Value>,
    pub draft_summary_markdown: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub draft_bpmn_xml: Option<String>,
    pub source_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowableDraftAssistantChatRequest {
    pub message: String,
    #[serde(default = "default_contextual")]
    pub mode: String,
    pub request_id: Option<String>,
    pub context: Option<FlowableDraftAssistantContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowableDraftAssistantChatData {
    pub answer_markdown: String,
    pub mode: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub generated_at: String,
    pub model: String,
}

fn default_contextual() -> String {
    "contextual".to_string()
}
