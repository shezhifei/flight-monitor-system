//! 自然语言查询 DTO

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NLQueryContextSchema {
    pub source_page: Option<String>,
    pub selected_flight_id: Option<String>,
    pub selected_flight_no: Option<String>,
    pub scope_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLQueryRequest {
    pub question: String,
    pub request_id: Option<String>,
    pub context: Option<NLQueryContextSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLFollowupRequest {
    pub question: String,
    pub conversation_id: String,
    pub request_id: Option<String>,
    pub context: Option<NLQueryContextSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLQueryResultSchema {
    pub query: String,
    pub interpretation: String,
    pub structured_data: serde_json::Value,
    pub visualization_hint: Option<String>,
    pub summary: String,
    pub conversation_id: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLSuggestionsResponse {
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationSchema {
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLConversationListItemSchema {
    pub conversation_id: String,
    pub title: Option<String>,
    pub status: String,
    pub model: Option<String>,
    pub message_count: usize,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub tags: Vec<String>,
    pub created_at: Option<f64>,
    pub updated_at: Option<f64>,
    pub last_activity_at: Option<f64>,
    pub ended_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLConversationListDataSchema {
    pub items: Vec<NLConversationListItemSchema>,
    pub total: usize,
    pub total_count: usize,
    pub pagination: PaginationSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLConversationMessageItemSchema {
    pub message_index: usize,
    pub role: String,
    pub content_raw: serde_json::Value,
    pub content_text: String,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub tool_call_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLConversationMessagesDataSchema {
    pub items: Vec<NLConversationMessageItemSchema>,
    pub total: usize,
    pub total_count: usize,
    pub pagination: PaginationSchema,
}
