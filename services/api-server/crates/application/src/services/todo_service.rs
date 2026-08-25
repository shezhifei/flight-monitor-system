//! 待办事项应用服务。
//!
//! 对齐 Python `AsyncTodoApplicationService` 与 `todo_routes.py`。

use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use fms_domain::error::DomainError;
use fms_domain::models::todo::{Todo, TodoCategory, TodoPriority, TodoStatus};
use fms_domain::ports::todo_agent_context_repository::TodoAgentContextRepository;
use fms_domain::ports::todo_repository::{TodoRepository, TodoTransactionalRepository};

use crate::schemas::todo_schemas::{
    TodoAssign, TodoCancel, TodoComplete, TodoCreateCommand, TodoListResponse, TodoProgress, TodoResponse,
    TodoStatsResponse, TodoUpdate,
};

#[derive(Default)]
struct AgentContextQueryMetrics {
    dedicated_query_calls: f64,
    dedicated_query_context_repo_path_calls: f64,
    dedicated_query_compat_fallback_calls: f64,
    dedicated_query_duration_ms_total: f64,
    dedicated_query_empty_results: f64,
}

/// 待办事项应用服务
pub struct TodoService {
    repo: Arc<dyn TodoRepository + Send + Sync>,
    context_repo: Option<Arc<dyn TodoAgentContextRepository + Send + Sync>>,
    agent_context_query_metrics: Mutex<AgentContextQueryMetrics>,
}

impl TodoService {
    pub fn new(repo: Arc<dyn TodoRepository + Send + Sync>) -> Self {
        Self {
            repo,
            context_repo: None,
            agent_context_query_metrics: Mutex::new(AgentContextQueryMetrics::default()),
        }
    }

}

impl TodoService {
    pub fn with_agent_context_repository(
        mut self,
        context_repo: Arc<dyn TodoAgentContextRepository + Send + Sync>,
    ) -> Self {
        self.context_repo = Some(context_repo);
        self
    }

    /// 创建待办事项
    pub async fn create_todo(&self, dto: TodoCreateCommand, actor: &str) -> Result<TodoResponse, DomainError> {
        let TodoCreateCommand {
            title,
            description,
            priority,
            category,
            due_date,
            estimated_duration,
            tags,
            agent_entity_id,
            source_type,
            source_id,
            created_by,
            assigned_to,
        } = dto;
        let now = Utc::now();
        let todo_id = ulid::Ulid::new().to_string();
        let created_by = created_by
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(actor)
            .to_string();
        let normalized_source_type = normalize_source_type(source_type.as_deref());
        let normalized_source_id = normalize_optional_string(source_id.as_deref());
        let normalized_agent_entity_id = normalize_optional_string(agent_entity_id.as_deref());

        let todo = Todo {
            todo_id: todo_id.clone(),
            title,
            description,
            priority: parse_priority(priority.as_deref().unwrap_or("中")),
            status: TodoStatus::Pending,
            category: category.as_deref().and_then(parse_category),
            due_date,
            assigned_to,
            tags: tags.unwrap_or_default(),
            estimated_duration,
            actual_duration: None,
            progress: 0,
            is_recurring: false,
            recurring_pattern: None,
            parent_todo_id: None,
            execution_order: 0,
            depends_on: vec![],
            source_type: normalized_source_type,
            source_id: normalized_source_id,
            is_deleted: false,
            deleted_at: None,
            created_at: now,
            updated_at: now,
            created_by: created_by.clone(),
            updated_by: created_by.clone(),
            version: 1,
        };

        self.repo.save(&todo).await?;
        if let (Some(context_repo), Some(agent_entity_id)) = (&self.context_repo, normalized_agent_entity_id.as_deref())
        {
            context_repo
                .upsert_partial(&todo_id, Some(agent_entity_id), None, None, &created_by)
                .await?;
        }
        Ok(todo_to_response(&todo))
    }

    /// 创建待办事项（使用外部事务）
    /// 查询单个待办
    pub async fn get_todo(&self, todo_id: &str) -> Result<Option<TodoResponse>, DomainError> {
        Ok(self.repo.find_by_id(todo_id).await?.map(|todo| todo_to_response(&todo)))
    }

    /// 查询列表
    #[allow(clippy::too_many_arguments)]
    pub async fn list_todos(
        &self,
        status: Option<&str>,
        priority: Option<&str>,
        category: Option<&str>,
        assignee: Option<&str>,
        source_type: Option<&str>,
        source_id: Option<&str>,
        agent_status: Option<&str>,
        agent_entity_id: Option<&str>,
        agent_run_id: Option<&str>,
        page: i64,
        size: i64,
    ) -> Result<TodoListResponse, DomainError> {
        let page = page.max(1);
        let size = size.clamp(1, 100);
        let offset = (page - 1) * size;

        let items = if has_agent_context_filter(agent_status, agent_entity_id, agent_run_id) {
            self.list_todos_by_agent_context(agent_status, agent_entity_id, agent_run_id, size, offset)
                .await?
        } else {
            self.repo
                .find_all(
                    status.map(parse_todo_status),
                    priority.map(parse_priority),
                    category,
                    assignee,
                    source_type,
                    source_id,
                    size,
                    offset,
                )
                .await?
        };

        let responses: Vec<TodoResponse> = items.iter().map(todo_to_response).collect();
        let total = responses.len() as i64;

        Ok(TodoListResponse {
            items: responses,
            total,
            page,
            size,
            pages: if total > 0 { (total + size - 1) / size } else { 1 },
        })
    }

    pub async fn list_open_todos_by_source(&self, source_type: &str, limit: i64) -> Result<Vec<Todo>, DomainError> {
        let normalized_source_type = source_type.trim();
        if normalized_source_type.is_empty() {
            return Ok(Vec::new());
        }

        Ok(self
            .repo
            .find_all(
                None,
                None,
                None,
                None,
                Some(normalized_source_type),
                None,
                limit.max(1),
                0,
            )
            .await?
            .into_iter()
            .filter(|todo| !todo.status.is_terminal())
            .collect())
    }

    pub async fn list_open_todos_by_source_for_assignee(
        &self,
        source_type: &str,
        assignee: &str,
        limit: i64,
    ) -> Result<Vec<Todo>, DomainError> {
        let normalized_source_type = source_type.trim();
        let normalized_assignee = assignee.trim();
        if normalized_source_type.is_empty() || normalized_assignee.is_empty() {
            return Ok(Vec::new());
        }

        Ok(self
            .repo
            .find_all(
                None,
                None,
                None,
                Some(normalized_assignee),
                Some(normalized_source_type),
                None,
                limit.max(1),
                0,
            )
            .await?
            .into_iter()
            .filter(|todo| !todo.status.is_terminal())
            .collect())
    }

    async fn list_todos_by_agent_context(
        &self,
        agent_status: Option<&str>,
        agent_entity_id: Option<&str>,
        agent_run_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Todo>, DomainError> {
        let started = Instant::now();
        self.inc_agent_metric(|metrics| &mut metrics.dedicated_query_calls, 1.0);

        if let Some(context_repo) = &self.context_repo {
            self.inc_agent_metric(|metrics| &mut metrics.dedicated_query_context_repo_path_calls, 1.0);
            let todo_ids = context_repo
                .find_todo_ids(agent_status, agent_entity_id, agent_run_id, limit, offset)
                .await?;
            if todo_ids.is_empty() {
                self.inc_agent_metric(|metrics| &mut metrics.dedicated_query_empty_results, 1.0);
                self.inc_agent_metric(
                    |metrics| &mut metrics.dedicated_query_duration_ms_total,
                    duration_ms(started),
                );
                return Ok(Vec::new());
            }

            let todos = self.repo.find_by_ids(&todo_ids).await?;
            let by_id: HashMap<String, Todo> = todos.into_iter().map(|todo| (todo.todo_id.clone(), todo)).collect();
            let ordered = todo_ids
                .into_iter()
                .filter_map(|todo_id| by_id.get(&todo_id).cloned())
                .collect();
            self.inc_agent_metric(
                |metrics| &mut metrics.dedicated_query_duration_ms_total,
                duration_ms(started),
            );
            return Ok(ordered);
        }

        self.inc_agent_metric(|metrics| &mut metrics.dedicated_query_compat_fallback_calls, 1.0);
        let fallback = self
            .repo
            .find_all(None, None, None, None, None, None, (limit + offset).max(200), 0)
            .await?;
        self.inc_agent_metric(
            |metrics| &mut metrics.dedicated_query_duration_ms_total,
            duration_ms(started),
        );
        Ok(fallback
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    /// 更新待办
    pub async fn update_todo(
        &self,
        todo_id: &str,
        dto: TodoUpdate,
        actor: &str,
    ) -> Result<Option<TodoResponse>, DomainError> {
        let mut todo = match self.repo.find_by_id(todo_id).await? {
            Some(todo) => todo,
            None => return Err(DomainError::Internal(format!("Todo not found: {todo_id}"))),
        };

        if let Some(title) = dto.title {
            todo.title = title;
        }
        if let Some(description) = dto.description {
            todo.description = Some(description);
        }
        if let Some(priority) = dto.priority {
            todo.priority = parse_priority(&priority);
        }
        if dto.category.is_some() {
            todo.category = dto.category.as_deref().and_then(parse_category);
        }
        if let Some(due_date) = dto.due_date {
            todo.due_date = Some(due_date);
        }
        if let Some(tags) = dto.tags {
            todo.tags = tags;
        }
        if let Some(estimated_duration) = dto.estimated_duration {
            todo.estimated_duration = Some(estimated_duration);
        }
        todo.updated_at = Utc::now();
        todo.updated_by = actor.to_string();
        todo.version += 1;

        self.repo.update(&todo).await?;
        Ok(Some(todo_to_response(&todo)))
    }

    /// 完成待办
    pub async fn complete_todo(
        &self,
        todo_id: &str,
        mut dto: TodoComplete,
        actor: &str,
    ) -> Result<Option<TodoResponse>, DomainError> {
        let mut todo = match self.repo.find_by_id(todo_id).await? {
            Some(todo) => todo,
            None => return Err(DomainError::Internal(format!("Todo not found: {todo_id}"))),
        };

        dto.completed_by = Some(actor.to_string());
        todo.mark_completed(dto.completed_by.as_deref().unwrap_or(actor))?;
        if let Some(actual_duration) = dto.actual_duration {
            todo.actual_duration = Some(actual_duration);
        }
        self.repo.update(&todo).await?;
        Ok(Some(todo_to_response(&todo)))
    }

    /// 完成待办（使用外部事务）
    /// 取消待办
    pub async fn cancel_todo(
        &self,
        todo_id: &str,
        mut dto: TodoCancel,
        actor: &str,
    ) -> Result<Option<TodoResponse>, DomainError> {
        let mut todo = match self.repo.find_by_id(todo_id).await? {
            Some(todo) => todo,
            None => return Err(DomainError::Internal(format!("Todo not found: {todo_id}"))),
        };

        dto.cancelled_by = Some(actor.to_string());
        todo.mark_cancelled(dto.cancelled_by.as_deref().unwrap_or(actor))?;
        self.repo.update(&todo).await?;
        Ok(Some(todo_to_response(&todo)))
    }

    /// 指派待办
    pub async fn assign_todo(
        &self,
        todo_id: &str,
        dto: TodoAssign,
        actor: &str,
    ) -> Result<Option<TodoResponse>, DomainError> {
        let mut todo = match self.repo.find_by_id(todo_id).await? {
            Some(todo) => todo,
            None => return Err(DomainError::Internal(format!("Todo not found: {todo_id}"))),
        };

        todo.assigned_to = Some(dto.assignee);
        if todo.status == TodoStatus::Pending {
            todo.status = TodoStatus::InProgress;
        }
        todo.updated_at = Utc::now();
        todo.updated_by = actor.to_string();
        todo.version += 1;
        self.repo.update(&todo).await?;
        Ok(Some(todo_to_response(&todo)))
    }

    /// 更新进度
    pub async fn update_progress(
        &self,
        todo_id: &str,
        mut dto: TodoProgress,
        actor: &str,
    ) -> Result<Option<TodoResponse>, DomainError> {
        let mut todo = match self.repo.find_by_id(todo_id).await? {
            Some(todo) => todo,
            None => return Err(DomainError::Internal(format!("Todo not found: {todo_id}"))),
        };

        dto.updated_by = Some(actor.to_string());
        todo.update_progress(dto.progress, dto.updated_by.as_deref().unwrap_or(actor))?;
        self.repo.update(&todo).await?;
        Ok(Some(todo_to_response(&todo)))
    }

    /// 统计
    pub async fn get_stats(&self) -> Result<TodoStatsResponse, DomainError> {
        let pending = self.repo.count_by_status(TodoStatus::Pending).await?;
        let in_progress = self.repo.count_by_status(TodoStatus::InProgress).await?;
        let completed = self.repo.count_by_status(TodoStatus::Completed).await?;
        let cancelled = self.repo.count_by_status(TodoStatus::Cancelled).await?;
        let total = pending + in_progress + completed + cancelled;

        // Limit sample size for aggregation to prevent unbounded memory growth.
        // Under 5000 items the approximation is accurate; above that the cost
        // outweighs the benefit.
        const MAX_STATS_SAMPLE: i64 = 5000;
        let all_todos = self
            .repo
            .find_all(None, None, None, None, None, None, MAX_STATS_SAMPLE, 0)
            .await?;
        let overdue = all_todos.iter().filter(|todo| todo.is_overdue()).count() as i64;
        let avg_completion_time = average_completion_time(&all_todos);
        let priority_distribution = priority_distribution(&all_todos);
        let category_distribution = category_distribution(&all_todos);

        Ok(TodoStatsResponse {
            total,
            pending,
            in_progress,
            completed,
            cancelled,
            overdue,
            avg_completion_time,
            completion_rate: if total > 0 {
                completed as f64 / total as f64
            } else {
                0.0
            },
            priority_distribution,
            category_distribution,
        })
    }

    /// 软删除
    pub async fn delete_todo(&self, todo_id: &str, deleted_by: &str) -> Result<bool, DomainError> {
        self.repo.soft_delete(todo_id, deleted_by).await
    }

    /// 导出 agent context 查询指标
    pub fn get_agent_context_query_metrics(&self) -> HashMap<String, Value> {
        let local_snapshot = self
            .agent_context_query_metrics
            .lock()
            .ok()
            .map(|metrics| {
                let calls = metrics.dedicated_query_calls;
                HashMap::from([
                    ("dedicated_query_calls".into(), json!(metrics.dedicated_query_calls)),
                    (
                        "dedicated_query_context_repo_path_calls".into(),
                        json!(metrics.dedicated_query_context_repo_path_calls),
                    ),
                    (
                        "dedicated_query_compat_fallback_calls".into(),
                        json!(metrics.dedicated_query_compat_fallback_calls),
                    ),
                    (
                        "dedicated_query_duration_ms_total".into(),
                        json!(metrics.dedicated_query_duration_ms_total),
                    ),
                    (
                        "dedicated_query_empty_results".into(),
                        json!(metrics.dedicated_query_empty_results),
                    ),
                    (
                        "dedicated_query_avg_duration_ms".into(),
                        json!(if calls > 0.0 {
                            metrics.dedicated_query_duration_ms_total / calls
                        } else {
                            0.0
                        }),
                    ),
                    (
                        "dedicated_query_context_repo_path_ratio".into(),
                        json!(if calls > 0.0 {
                            metrics.dedicated_query_context_repo_path_calls / calls
                        } else {
                            0.0
                        }),
                    ),
                    (
                        "dedicated_query_compat_fallback_ratio".into(),
                        json!(if calls > 0.0 {
                            metrics.dedicated_query_compat_fallback_calls / calls
                        } else {
                            0.0
                        }),
                    ),
                ])
            })
            .unwrap_or_default();

        let mut merged = local_snapshot;
        if let Some(context_repo) = &self.context_repo {
            for (key, value) in context_repo.get_metrics_snapshot() {
                merged.insert(format!("repo_{key}"), value);
            }
        }
        merged
    }

    fn inc_agent_metric(&self, selector: fn(&mut AgentContextQueryMetrics) -> &mut f64, value: f64) {
        if let Ok(mut metrics) = self.agent_context_query_metrics.lock() {
            *selector(&mut metrics) += value;
        }
    }
}

fn duration_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_source_type(value: Option<&str>) -> String {
    normalize_optional_string(value).unwrap_or_else(|| "manual".to_string())
}

fn has_agent_context_filter(
    agent_status: Option<&str>,
    agent_entity_id: Option<&str>,
    agent_run_id: Option<&str>,
) -> bool {
    [agent_status, agent_entity_id, agent_run_id]
        .into_iter()
        .flatten()
        .any(|value| !value.trim().is_empty())
}

fn parse_priority(value: &str) -> TodoPriority {
    match value.trim().to_lowercase().as_str() {
        "critical" | "关键" => TodoPriority::Critical,
        "high" | "高" => TodoPriority::High,
        "medium" | "中" => TodoPriority::Medium,
        "low" | "低" => TodoPriority::Low,
        "background" | "后台" => TodoPriority::Background,
        _ => TodoPriority::Medium,
    }
}

fn parse_todo_status(value: &str) -> TodoStatus {
    match value.trim().to_lowercase().as_str() {
        "pending" | "待办" => TodoStatus::Pending,
        "in_progress" | "进行中" => TodoStatus::InProgress,
        "completed" | "已完成" => TodoStatus::Completed,
        "cancelled" | "已取消" => TodoStatus::Cancelled,
        "blocked" | "阻塞中" => TodoStatus::Blocked,
        _ => TodoStatus::Pending,
    }
}

fn parse_category(value: &str) -> Option<TodoCategory> {
    match value.trim().to_lowercase().as_str() {
        "work" | "工作" => Some(TodoCategory::Work),
        "personal" | "个人" => Some(TodoCategory::Personal),
        "meeting" | "会议" => Some(TodoCategory::Meeting),
        "deadline" | "截止日期" => Some(TodoCategory::Deadline),
        "recurring" | "重复任务" => Some(TodoCategory::Recurring),
        _ => None,
    }
}

fn priority_value(priority: TodoPriority) -> String {
    priority.label().to_string()
}

fn status_value(status: TodoStatus) -> String {
    status.label().to_string()
}

fn category_value(category: Option<TodoCategory>) -> Option<String> {
    category.map(|value| match value {
        TodoCategory::Work => "工作".to_string(),
        TodoCategory::Personal => "个人".to_string(),
        TodoCategory::Meeting => "会议".to_string(),
        TodoCategory::Deadline => "截止日期".to_string(),
        TodoCategory::Recurring => "重复任务".to_string(),
    })
}

fn todo_to_response(todo: &Todo) -> TodoResponse {
    TodoResponse {
        id: todo.todo_id.clone(),
        title: todo.title.clone(),
        description: todo.description.clone(),
        priority: priority_value(todo.priority),
        category: category_value(todo.category),
        status: status_value(todo.status),
        assigned_to: todo.assigned_to.clone(),
        due_date: todo.due_date,
        estimated_duration: todo.estimated_duration,
        actual_duration: todo.actual_duration,
        progress: todo.progress,
        tags: todo.tags.clone(),
        is_recurring: todo.is_recurring,
        recurring_pattern: todo.recurring_pattern.clone(),
        is_overdue: todo.is_overdue(),
        created_at: todo.created_at,
        updated_at: todo.updated_at,
        created_by: todo.created_by.clone(),
        updated_by: todo.updated_by.clone(),
        version: todo.version,
    }
}

fn average_completion_time(todos: &[Todo]) -> Option<f64> {
    let durations: Vec<i32> = todos
        .iter()
        .filter(|todo| todo.status == TodoStatus::Completed)
        .filter_map(|todo| todo.actual_duration)
        .collect();
    if durations.is_empty() {
        None
    } else {
        Some(durations.iter().map(|value| *value as f64).sum::<f64>() / durations.len() as f64)
    }
}

fn priority_distribution(todos: &[Todo]) -> HashMap<String, i64> {
    let mut distribution = HashMap::new();
    for todo in todos {
        *distribution.entry(priority_value(todo.priority)).or_insert(0) += 1;
    }
    distribution
}

fn category_distribution(todos: &[Todo]) -> HashMap<String, i64> {
    let mut distribution = HashMap::new();
    for todo in todos {
        if let Some(category) = category_value(todo.category) {
            *distribution.entry(category).or_insert(0) += 1;
        }
    }
    distribution
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::{Duration, Utc};
    use fms_domain::ports::todo_agent_context_repository::{TodoAgentContext, TodoAgentContextRepository};
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeTodoRepo {
        todos: Mutex<HashMap<String, Todo>>,
    }

    impl FakeTodoRepo {
        fn todo(&self, todo_id: &str) -> Todo {
            self.todos
                .lock()
                .expect("lock todos")
                .get(todo_id)
                .cloned()
                .expect("todo exists")
        }
    }

    #[async_trait::async_trait]
    impl TodoRepository for FakeTodoRepo {
        async fn find_by_id(&self, todo_id: &str) -> Result<Option<Todo>, DomainError> {
            Ok(self.todos.lock().expect("lock todos").get(todo_id).cloned())
        }

        async fn find_all(
            &self,
            _status: Option<TodoStatus>,
            _priority: Option<TodoPriority>,
            _category: Option<&str>,
            _assigned_to: Option<&str>,
            _source_type: Option<&str>,
            _source_id: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<Todo>, DomainError> {
            Ok(self.todos.lock().expect("lock todos").values().cloned().collect())
        }

        async fn find_by_ids(&self, todo_ids: &[String]) -> Result<Vec<Todo>, DomainError> {
            let todos = self.todos.lock().expect("lock todos");
            Ok(todo_ids
                .iter()
                .filter_map(|todo_id| todos.get(todo_id).cloned())
                .collect())
        }

        async fn find_by_source(&self, source_type: &str, source_id: &str) -> Result<Vec<Todo>, DomainError> {
            Ok(self
                .todos
                .lock()
                .expect("lock todos")
                .values()
                .filter(|todo| todo.source_type == source_type && todo.source_id.as_deref() == Some(source_id))
                .cloned()
                .collect())
        }

        async fn find_overdue(&self) -> Result<Vec<Todo>, DomainError> {
            Ok(self
                .todos
                .lock()
                .expect("lock todos")
                .values()
                .filter(|todo| todo.is_overdue())
                .cloned()
                .collect())
        }

        async fn find_children(&self, parent_todo_id: &str) -> Result<Vec<Todo>, DomainError> {
            Ok(self
                .todos
                .lock()
                .expect("lock todos")
                .values()
                .filter(|todo| todo.parent_todo_id.as_deref() == Some(parent_todo_id))
                .cloned()
                .collect())
        }

        async fn save(&self, todo: &Todo) -> Result<(), DomainError> {
            self.todos
                .lock()
                .expect("lock todos")
                .insert(todo.todo_id.clone(), todo.clone());
            Ok(())
        }

        async fn update(&self, todo: &Todo) -> Result<bool, DomainError> {
            self.save(todo).await?;
            Ok(true)
        }

        async fn soft_delete(&self, todo_id: &str, _deleted_by: &str) -> Result<bool, DomainError> {
            Ok(self.todos.lock().expect("lock todos").remove(todo_id).is_some())
        }

        async fn count_by_status(&self, status: TodoStatus) -> Result<i64, DomainError> {
            Ok(self
                .todos
                .lock()
                .expect("lock todos")
                .values()
                .filter(|todo| todo.status == status)
                .count() as i64)
        }

        async fn count_by_source_ids(
            &self,
            _source_type: &str,
            _source_ids: &[String],
            _cutoff: chrono::DateTime<chrono::Utc>,
        ) -> Result<i64, DomainError> {
            Ok(0)
        }

        // 本文件的测试都不走冒烟清理。返回 `Ok(0)` 会让将来误用这个端口的测试静默通过，
        // 所以这里响一声。
        async fn soft_delete_by_source_ids(
            &self,
            _source_type: &str,
            _source_ids: &[String],
            _cutoff: chrono::DateTime<chrono::Utc>,
        ) -> Result<u64, DomainError> {
            unimplemented!("soft_delete_by_source_ids is not exercised by these tests")
        }
    }

    #[derive(Default)]
    struct FakeTodoAgentContextRepo {
        contexts: Mutex<HashMap<String, TodoAgentContext>>,
        upsert_calls: Mutex<Vec<(String, Option<String>, Option<String>, Option<String>, String)>>,
    }

    impl FakeTodoAgentContextRepo {
        fn context(&self, todo_id: &str) -> TodoAgentContext {
            self.contexts
                .lock()
                .expect("lock contexts")
                .get(todo_id)
                .cloned()
                .expect("context exists")
        }

        fn upsert_calls(&self) -> Vec<(String, Option<String>, Option<String>, Option<String>, String)> {
            self.upsert_calls.lock().expect("lock upsert calls").clone()
        }
    }

    #[async_trait::async_trait]
    impl TodoAgentContextRepository for FakeTodoAgentContextRepo {
        async fn get(&self, todo_id: &str) -> Result<Option<TodoAgentContext>, DomainError> {
            Ok(self.contexts.lock().expect("lock contexts").get(todo_id).cloned())
        }

        async fn batch_get(&self, todo_ids: &[String]) -> Result<HashMap<String, TodoAgentContext>, DomainError> {
            let contexts = self.contexts.lock().expect("lock contexts");
            Ok(todo_ids
                .iter()
                .filter_map(|todo_id| contexts.get(todo_id).cloned().map(|context| (todo_id.clone(), context)))
                .collect())
        }

        async fn upsert_partial(
            &self,
            todo_id: &str,
            agent_entity_id: Option<&str>,
            agent_run_id: Option<&str>,
            agent_status: Option<&str>,
            updated_by: &str,
        ) -> Result<TodoAgentContext, DomainError> {
            let existing = self.contexts.lock().expect("lock contexts").get(todo_id).cloned();
            let context = TodoAgentContext {
                todo_id: todo_id.to_string(),
                agent_entity_id: agent_entity_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(
                        existing
                            .as_ref()
                            .map(|item| item.agent_entity_id.as_str())
                            .unwrap_or("default"),
                    )
                    .to_string(),
                agent_run_id: agent_run_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .or_else(|| existing.as_ref().and_then(|item| item.agent_run_id.clone())),
                agent_status: agent_status
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(
                        existing
                            .as_ref()
                            .map(|item| item.agent_status.as_str())
                            .unwrap_or("pending"),
                    )
                    .to_string(),
                updated_by: updated_by.trim().to_string(),
                updated_at: Some(Utc::now()),
                version: existing.map(|item| item.version + 1).unwrap_or(1),
            };
            self.contexts
                .lock()
                .expect("lock contexts")
                .insert(todo_id.to_string(), context.clone());
            self.upsert_calls.lock().expect("lock upsert calls").push((
                todo_id.to_string(),
                agent_entity_id.map(ToString::to_string),
                agent_run_id.map(ToString::to_string),
                agent_status.map(ToString::to_string),
                updated_by.to_string(),
            ));
            Ok(context)
        }

        async fn find_todo_ids(
            &self,
            agent_status: Option<&str>,
            agent_entity_id: Option<&str>,
            agent_run_id: Option<&str>,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<String>, DomainError> {
            let mut todo_ids = self
                .contexts
                .lock()
                .expect("lock contexts")
                .values()
                .filter(|context| {
                    agent_status.is_none_or(|value| context.agent_status == value)
                        && agent_entity_id.is_none_or(|value| context.agent_entity_id == value)
                        && agent_run_id.is_none_or(|value| context.agent_run_id.as_deref() == Some(value))
                })
                .map(|context| context.todo_id.clone())
                .collect::<Vec<_>>();
            todo_ids.sort();
            Ok(todo_ids
                .into_iter()
                .skip(offset.max(0) as usize)
                .take(limit.max(0) as usize)
                .collect())
        }

        fn get_metrics_snapshot(&self) -> HashMap<String, serde_json::Value> {
            HashMap::new()
        }
    }

    fn sample_create_command() -> TodoCreateCommand {
        TodoCreateCommand {
            title: "Follow flight status".to_string(),
            description: Some("sync from upstream".to_string()),
            priority: Some("高".to_string()),
            category: Some("工作".to_string()),
            due_date: Some(Utc::now() + Duration::hours(2)),
            estimated_duration: Some(20),
            tags: Some(vec!["ops".to_string()]),
            agent_entity_id: None,
            source_type: None,
            source_id: None,
            created_by: Some("creator".to_string()),
            assigned_to: Some("operator-a".to_string()),
        }
    }

    #[tokio::test]
    async fn create_todo_persists_python_compatible_source_fields() {
        let repo = Arc::new(FakeTodoRepo::default());
        let service = TodoService::new(repo.clone());

        let response = service
            .create_todo(
                TodoCreateCommand {
                    source_type: Some("agent_run".to_string()),
                    source_id: Some(" run-123 ".to_string()),
                    ..sample_create_command()
                },
                "api-user",
            )
            .await
            .expect("create todo succeeds");

        let saved = repo.todo(&response.id);
        assert_eq!(saved.source_type, "agent_run");
        assert_eq!(saved.source_id.as_deref(), Some("run-123"));
        assert_eq!(saved.created_by, "creator");
    }

    #[tokio::test]
    async fn create_todo_upserts_agent_context_when_repository_is_configured() {
        let repo = Arc::new(FakeTodoRepo::default());
        let context_repo = Arc::new(FakeTodoAgentContextRepo::default());
        let service = TodoService::new(repo.clone()).with_agent_context_repository(context_repo.clone());

        let response = service
            .create_todo(
                TodoCreateCommand {
                    agent_entity_id: Some(" agent-42 ".to_string()),
                    ..sample_create_command()
                },
                "api-user",
            )
            .await
            .expect("create todo succeeds");

        let saved = repo.todo(&response.id);
        let context = context_repo.context(&response.id);
        assert_eq!(saved.source_type, "manual");
        assert_eq!(context.todo_id, response.id);
        assert_eq!(context.agent_entity_id, "agent-42");
        assert_eq!(context.agent_status, "pending");
        assert_eq!(context.updated_by, "creator");
        assert_eq!(
            context_repo.upsert_calls(),
            vec![(
                response.id,
                Some("agent-42".to_string()),
                None,
                None,
                "creator".to_string(),
            )]
        );
    }
}

/// 待办的两个「在别人已经开好的事务里写」的单元。
///
/// `TodoService` 有一大票 `web::Data` 注入、api 层要调它十来个方法，所以服务本身保持
/// 非泛型；只有这两个持有事务句柄的方法搬到对 `Tx` 泛型的写入方上。它们的唯一调用方是
/// `DomainActionExecutor`，api 层零调用，所以这里不需要接缝端口——也不需要 trait：
/// 只有一个实现、一个调用方的 trait 正是上一步在 anomaly 那里删掉的那种转发层。
///
/// 这里的 `tx_repo` 不是 `Option`：写入方没有仓储就不该存在。原来那两处
/// 「transactional repository is not configured」的运行期错误因此不必存在。
pub struct TodoWriter<Tx> {
    repo: Arc<dyn TodoRepository + Send + Sync>,
    tx_repo: Arc<dyn TodoTransactionalRepository<Tx> + Send + Sync>,
    context_repo: Option<Arc<dyn TodoAgentContextRepository + Send + Sync>>,
}

impl<Tx> TodoWriter<Tx> {
    pub fn new(
        repo: Arc<dyn TodoRepository + Send + Sync>,
        tx_repo: Arc<dyn TodoTransactionalRepository<Tx> + Send + Sync>,
    ) -> Self {
        Self {
            repo,
            tx_repo,
            context_repo: None,
        }
    }

    pub fn with_agent_context_repository(
        mut self,
        context_repo: Arc<dyn TodoAgentContextRepository + Send + Sync>,
    ) -> Self {
        self.context_repo = Some(context_repo);
        self
    }
}

impl<Tx: Send> TodoWriter<Tx> {
    pub async fn create_todo_in_tx(
        &self,
        tx: &mut Tx,
        dto: TodoCreateCommand,
        actor: &str,
    ) -> Result<TodoResponse, DomainError> {
        let TodoCreateCommand {
            title,
            description,
            priority,
            category,
            due_date,
            estimated_duration,
            tags,
            agent_entity_id,
            source_type,
            source_id,
            created_by,
            assigned_to,
        } = dto;
        let now = Utc::now();
        let todo_id = ulid::Ulid::new().to_string();
        let created_by = created_by
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(actor)
            .to_string();
        let normalized_source_type = normalize_source_type(source_type.as_deref());
        let normalized_source_id = normalize_optional_string(source_id.as_deref());
        let normalized_agent_entity_id = normalize_optional_string(agent_entity_id.as_deref());

        let todo = Todo {
            todo_id: todo_id.clone(),
            title,
            description,
            priority: parse_priority(priority.as_deref().unwrap_or("中")),
            status: TodoStatus::Pending,
            category: category.as_deref().and_then(parse_category),
            due_date,
            assigned_to,
            tags: tags.unwrap_or_default(),
            estimated_duration,
            actual_duration: None,
            progress: 0,
            is_recurring: false,
            recurring_pattern: None,
            parent_todo_id: None,
            execution_order: 0,
            depends_on: vec![],
            source_type: normalized_source_type,
            source_id: normalized_source_id,
            is_deleted: false,
            deleted_at: None,
            created_at: now,
            updated_at: now,
            created_by: created_by.clone(),
            updated_by: created_by.clone(),
            version: 1,
        };

        self.tx_repo.save_in_tx(tx, &todo).await?;
        if let (Some(context_repo), Some(agent_entity_id)) = (&self.context_repo, normalized_agent_entity_id.as_deref())
        {
            context_repo
                .upsert_partial(&todo_id, Some(agent_entity_id), None, None, &created_by)
                .await?;
        }
        Ok(todo_to_response(&todo))
    }

    pub async fn complete_todo_in_tx(
        &self,
        tx: &mut Tx,
        todo_id: &str,
        mut dto: TodoComplete,
        actor: &str,
    ) -> Result<Option<TodoResponse>, DomainError> {
        let mut todo = match self.repo.find_by_id(todo_id).await? {
            Some(todo) => todo,
            None => return Err(DomainError::Internal(format!("Todo not found: {todo_id}"))),
        };
        dto.completed_by = Some(actor.to_string());
        todo.mark_completed(dto.completed_by.as_deref().unwrap_or(actor))?;
        if let Some(actual_duration) = dto.actual_duration {
            todo.actual_duration = Some(actual_duration);
        }
        self.tx_repo.update_in_tx(tx, &todo).await?;
        Ok(Some(todo_to_response(&todo)))
    }
}
