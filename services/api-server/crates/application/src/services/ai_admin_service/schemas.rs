use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiBatchRequestItem {
    pub request_id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiBatchResultItem {
    pub request_id: String,
    pub success: bool,
    pub response: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}
