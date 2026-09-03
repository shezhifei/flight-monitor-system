//! 待办事项领域模型
//!
//! 对应 Python `src/domain/models/todo.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

// ---------------------------------------------------------------------------
// 枚举
// ---------------------------------------------------------------------------

/// 待办事项优先级
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoPriority {
    Critical,
    High,
    #[default]
    Medium,
    Low,
    Background,
}

impl TodoPriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "关键",
            Self::High => "高",
            Self::Medium => "中",
            Self::Low => "低",
            Self::Background => "后台",
        }
    }

    /// 优先级数值 (越小越高)
    pub fn level(self) -> u8 {
        match self {
            Self::Critical => 1,
            Self::High => 2,
            Self::Medium => 3,
            Self::Low => 4,
            Self::Background => 5,
        }
    }

    pub fn is_high(self) -> bool {
        self.level() <= 2
    }
}

impl AsRef<str> for TodoPriority {
    fn as_ref(&self) -> &str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Background => "background",
        }
    }
}

/// 待办事项状态
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Cancelled,
    Blocked,
}

impl TodoStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "待办",
            Self::InProgress => "进行中",
            Self::Completed => "已完成",
            Self::Cancelled => "已取消",
            Self::Blocked => "阻塞中",
        }
    }

    /// 检查是否可以转换到目标状态
    pub fn can_transition_to(self, target: Self) -> bool {
        use TodoStatus::*;
        matches!(
            (self, target),
            (Pending, InProgress | Cancelled | Blocked)
                | (InProgress, Completed | Pending | Cancelled | Blocked)
                | (Blocked, Pending | InProgress | Cancelled)
        )
    }

    /// 是否为终态
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

impl AsRef<str> for TodoStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }
}

/// 待办事项分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoCategory {
    Work,
    Personal,
    Meeting,
    Deadline,
    Recurring,
}

impl AsRef<str> for TodoCategory {
    fn as_ref(&self) -> &str {
        match self {
            Self::Work => "work",
            Self::Personal => "personal",
            Self::Meeting => "meeting",
            Self::Deadline => "deadline",
            Self::Recurring => "recurring",
        }
    }
}

// ---------------------------------------------------------------------------
// Todo 实体
// ---------------------------------------------------------------------------

/// 待办事项实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub todo_id: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub priority: TodoPriority,
    #[serde(default)]
    pub status: TodoStatus,
    pub category: Option<TodoCategory>,
    pub due_date: Option<DateTime<Utc>>,
    pub assigned_to: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 预计完成时间（分钟）
    pub estimated_duration: Option<i32>,
    /// 实际完成时间（分钟）
    pub actual_duration: Option<i32>,
    /// 完成进度百分比 (0–100)
    #[serde(default)]
    pub progress: i32,
    #[serde(default)]
    pub is_recurring: bool,
    pub recurring_pattern: Option<String>,

    // 层级结构
    pub parent_todo_id: Option<String>,
    #[serde(default)]
    pub execution_order: i32,
    #[serde(default)]
    pub depends_on: Vec<String>,

    // 来源
    #[serde(default = "default_source_type")]
    pub source_type: String,
    pub source_id: Option<String>,

    // 软删除
    #[serde(default)]
    pub is_deleted: bool,
    pub deleted_at: Option<DateTime<Utc>>,

    // 审计
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_system")]
    pub created_by: String,
    #[serde(default = "default_system")]
    pub updated_by: String,
    #[serde(default)]
    pub version: i32,
}

fn default_source_type() -> String {
    "manual".to_string()
}
fn default_system() -> String {
    "System".to_string()
}

impl Todo {
    /// 更新状态（含状态转换校验）
    pub fn update_status(&mut self, new_status: TodoStatus, updated_by: &str) -> Result<(), DomainError> {
        if !self.status.can_transition_to(new_status) {
            return Err(DomainError::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", new_status),
            });
        }
        self.status = new_status;
        if new_status == TodoStatus::Completed {
            self.progress = 100;
        } else if new_status == TodoStatus::Pending {
            self.progress = 0;
        }
        self.touch(updated_by);
        Ok(())
    }

    /// 更新进度（自动调整状态）
    pub fn update_progress(&mut self, new_progress: i32, updated_by: &str) -> Result<(), DomainError> {
        if !(0..=100).contains(&new_progress) {
            return Err(DomainError::ValidationError("进度必须在 0-100 之间".into()));
        }
        if new_progress == 100 && self.status != TodoStatus::Completed {
            self.status = TodoStatus::Completed;
        } else if new_progress > 0 && self.status == TodoStatus::Pending {
            self.status = TodoStatus::InProgress;
        }
        self.progress = new_progress;
        self.touch(updated_by);
        Ok(())
    }

    /// 标记完成
    pub fn mark_completed(&mut self, completed_by: &str) -> Result<(), DomainError> {
        self.update_status(TodoStatus::Completed, completed_by)
    }

    /// 标记取消
    pub fn mark_cancelled(&mut self, cancelled_by: &str) -> Result<(), DomainError> {
        self.update_status(TodoStatus::Cancelled, cancelled_by)
    }

    /// 是否逾期
    pub fn is_overdue(&self) -> bool {
        if self.status.is_terminal() {
            return false;
        }
        self.due_date.map(|d| Utc::now() > d).unwrap_or(false)
    }

    /// 是否可编辑
    pub fn can_be_edited(&self) -> bool {
        !self.status.is_terminal()
    }

    fn touch(&mut self, updated_by: &str) {
        self.updated_at = Utc::now();
        self.updated_by = updated_by.to_string();
        self.version += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_todo() -> Todo {
        serde_json::from_value(json!({
            "todo_id": "t-1",
            "title": "test",
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z",
        }))
        .unwrap()
    }

    // --- TodoPriority ---

    #[test]
    fn priority_level_ordering() {
        assert!(TodoPriority::Critical.level() < TodoPriority::High.level());
        assert!(TodoPriority::High.level() < TodoPriority::Medium.level());
        assert!(TodoPriority::Medium.level() < TodoPriority::Low.level());
        assert!(TodoPriority::Low.level() < TodoPriority::Background.level());
    }

    #[test]
    fn priority_is_high() {
        assert!(TodoPriority::Critical.is_high());
        assert!(TodoPriority::High.is_high());
        assert!(!TodoPriority::Medium.is_high());
        assert!(!TodoPriority::Low.is_high());
    }

    // --- TodoStatus transitions ---

    #[test]
    fn valid_transitions() {
        assert!(TodoStatus::Pending.can_transition_to(TodoStatus::InProgress));
        assert!(TodoStatus::Pending.can_transition_to(TodoStatus::Cancelled));
        assert!(TodoStatus::Pending.can_transition_to(TodoStatus::Blocked));
        assert!(TodoStatus::InProgress.can_transition_to(TodoStatus::Completed));
        assert!(TodoStatus::InProgress.can_transition_to(TodoStatus::Pending));
        assert!(TodoStatus::Blocked.can_transition_to(TodoStatus::Pending));
        assert!(TodoStatus::Blocked.can_transition_to(TodoStatus::InProgress));
    }

    #[test]
    fn invalid_transitions() {
        assert!(!TodoStatus::Pending.can_transition_to(TodoStatus::Completed));
        assert!(!TodoStatus::Completed.can_transition_to(TodoStatus::Pending));
        assert!(!TodoStatus::Cancelled.can_transition_to(TodoStatus::Pending));
    }

    #[test]
    fn terminal_status() {
        assert!(TodoStatus::Completed.is_terminal());
        assert!(TodoStatus::Cancelled.is_terminal());
        assert!(!TodoStatus::Pending.is_terminal());
        assert!(!TodoStatus::InProgress.is_terminal());
        assert!(!TodoStatus::Blocked.is_terminal());
    }

    // --- Todo entity ---

    #[test]
    fn update_status_valid_transition() {
        let mut todo = sample_todo();
        todo.update_status(TodoStatus::InProgress, "user").unwrap();
        assert_eq!(todo.status, TodoStatus::InProgress);
    }

    #[test]
    fn update_status_invalid_transition_errors() {
        let mut todo = sample_todo();
        let err = todo.update_status(TodoStatus::Completed, "user").unwrap_err();
        assert!(matches!(err, DomainError::InvalidStateTransition { .. }));
    }

    #[test]
    fn update_status_completed_sets_progress_100() {
        let mut todo = sample_todo();
        todo.status = TodoStatus::InProgress;
        todo.update_status(TodoStatus::Completed, "user").unwrap();
        assert_eq!(todo.progress, 100);
    }

    #[test]
    fn update_progress_valid_range() {
        let mut todo = sample_todo();
        todo.update_progress(50, "user").unwrap();
        assert_eq!(todo.progress, 50);
        assert_eq!(todo.status, TodoStatus::InProgress);
    }

    #[test]
    fn update_progress_100_completes() {
        let mut todo = sample_todo();
        todo.update_progress(100, "user").unwrap();
        assert_eq!(todo.status, TodoStatus::Completed);
    }

    #[test]
    fn update_progress_out_of_range_errors() {
        let mut todo = sample_todo();
        assert!(todo.update_progress(-1, "user").is_err());
        assert!(todo.update_progress(101, "user").is_err());
    }

    #[test]
    fn can_be_edited_reflects_terminal() {
        let mut todo = sample_todo();
        assert!(todo.can_be_edited());
        todo.status = TodoStatus::Completed;
        assert!(!todo.can_be_edited());
    }

    // --- Property-based tests ---

    proptest::proptest! {
        #[test]
        fn todo_priority_json_roundtrip(level in 0u8..5) {
            let priorities = [
                TodoPriority::Critical,
                TodoPriority::High,
                TodoPriority::Medium,
                TodoPriority::Low,
                TodoPriority::Background,
            ];
            let original = priorities[level as usize];
            let json = serde_json::to_string(&original).unwrap();
            let restored: TodoPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(original, restored);
        }

        #[test]
        fn todo_status_json_roundtrip(idx in 0u8..5) {
            let statuses = [
                TodoStatus::Pending,
                TodoStatus::InProgress,
                TodoStatus::Completed,
                TodoStatus::Cancelled,
                TodoStatus::Blocked,
            ];
            let original = statuses[idx as usize];
            let json = serde_json::to_string(&original).unwrap();
            let restored: TodoStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(original, restored);
        }

        #[test]
        fn terminal_states_never_accept_transitions(idx in 0u8..2) {
            let terminal = [TodoStatus::Completed, TodoStatus::Cancelled][idx as usize];
            let all = [
                TodoStatus::Pending,
                TodoStatus::InProgress,
                TodoStatus::Completed,
                TodoStatus::Cancelled,
                TodoStatus::Blocked,
            ];
            for target in all {
                assert!(!terminal.can_transition_to(target),
                    "terminal state {:?} should not transition to {:?}", terminal, target);
            }
        }
    }
}
