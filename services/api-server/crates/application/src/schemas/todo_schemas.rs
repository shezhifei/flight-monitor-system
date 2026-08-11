//! 待办事项 DTO 模式
//!
//! 对应 Python `src/application/schemas/todo_schemas.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 创建 / 更新
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TodoCreate {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub category: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub estimated_duration: Option<i32>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    pub created_by: Option<String>,
    pub assigned_to: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TodoCreateCommand {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub category: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub estimated_duration: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub agent_entity_id: Option<String>,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub created_by: Option<String>,
    pub assigned_to: Option<String>,
}

impl From<TodoCreate> for TodoCreateCommand {
    fn from(value: TodoCreate) -> Self {
        Self {
            title: value.title,
            description: value.description,
            priority: value.priority,
            category: value.category,
            due_date: value.due_date,
            estimated_duration: value.estimated_duration,
            tags: value.tags,
            agent_entity_id: None,
            source_type: None,
            source_id: None,
            created_by: value.created_by,
            assigned_to: value.assigned_to,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TodoUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub category: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub estimated_duration: Option<i32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TodoAssign {
    pub assignee: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TodoComplete {
    pub actual_duration: Option<i32>,
    pub completed_by: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TodoCancel {
    pub reason: Option<String>,
    pub cancelled_by: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TodoProgress {
    pub progress: i32, // 0-100
    pub updated_by: Option<String>,
}

// ---------------------------------------------------------------------------
// 响应
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TodoResponse {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: String,
    pub category: Option<String>,
    pub status: String,
    pub assigned_to: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub estimated_duration: Option<i32>,
    pub actual_duration: Option<i32>,
    pub progress: i32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub is_recurring: bool,
    pub recurring_pattern: Option<String>,
    pub is_overdue: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: String,
    pub updated_by: String,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodoStatsResponse {
    pub total: i64,
    pub pending: i64,
    pub in_progress: i64,
    pub completed: i64,
    pub cancelled: i64,
    pub overdue: i64,
    pub avg_completion_time: Option<f64>,
    pub completion_rate: f64,
    #[serde(default)]
    pub priority_distribution: HashMap<String, i64>,
    #[serde(default)]
    pub category_distribution: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodoListResponse {
    pub items: Vec<TodoResponse>,
    pub total: i64,
    pub page: i64,
    pub size: i64,
    pub pages: i64,
}

#[cfg(test)]
mod tests {
    use super::TodoCreate;
    use serde_json::json;

    #[test]
    fn public_todo_create_ignores_internal_only_fields() {
        let parsed: TodoCreate = serde_json::from_value(json!({
            "title": "Check stand assignment",
            "priority": "高",
            "agent_entity_id": "agent-42",
            "source_type": "agent_run",
            "source_id": "run-123",
            "created_by": "operator-a"
        }))
        .expect("public todo create should ignore internal-only fields");

        assert_eq!(parsed.title, "Check stand assignment");
        assert_eq!(parsed.priority.as_deref(), Some("高"));
        assert_eq!(parsed.created_by.as_deref(), Some("operator-a"));
    }
}
