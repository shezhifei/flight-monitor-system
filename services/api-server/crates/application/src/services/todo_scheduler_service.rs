use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use futures_util::stream::{self, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use tracing::warn;

use fms_domain::error::DomainError;
use fms_domain::models::todo::{Todo, TodoPriority, TodoStatus};
use fms_domain::ports::todo_repository::TodoRepository;

use crate::services::notification_service::{NotificationCreate, NotificationService};

const TODO_SCHEDULER_ACTOR: &str = "TodoScheduler";
const DEFAULT_QUERY_LIMIT: i64 = 1000;
const DEFAULT_SAVE_CONCURRENCY: usize = 20;
const DEFAULT_SIDE_EFFECT_CONCURRENCY: usize = 20;

pub trait TodoSchedulerSsePublisher: Send + Sync {
    fn publish_system_alert<'a>(
        &'a self,
        payload: Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>>;
}

pub trait TodoSchedulerNotificationSender: Send + Sync {
    fn send_scheduler_notification<'a>(
        &'a self,
        notification: NotificationCreate,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>>;
}

impl<NR, PR, CR, DP, MR, RS> TodoSchedulerNotificationSender for NotificationService<NR, PR, CR, DP, MR, RS>
where
    NR: fms_domain::ports::notification_repository::NotificationRepository + Send + Sync + ?Sized,
    PR: fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync + ?Sized,
    CR: fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync + ?Sized,
    DP: crate::services::notification_service::NotificationDeliveryPublisher + Send + Sync + ?Sized,
    MR: crate::services::notification_service::NotificationMetricsRecorder + Send + Sync + ?Sized,
    RS: crate::services::notification_service::NotificationReceiptGroupSync + Send + Sync + ?Sized,
{
    fn send_scheduler_notification<'a>(
        &'a self,
        notification: NotificationCreate,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async move {
            self.send_notification(notification).await?;
            Ok(())
        })
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct TodoSchedulerRunSummary {
    pub overdue_ids: Vec<String>,
    pub unblocked_ids: Vec<String>,
    pub escalated_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct SchedulerSideEffect {
    todo: Todo,
    title: String,
    body: String,
    severity: String,
    event: String,
    event_payload: Value,
}

pub struct TodoSchedulerService {
    repo: Arc<dyn TodoRepository + Send + Sync>,
    notification_service: Option<Arc<dyn TodoSchedulerNotificationSender>>,
    sse_publisher: Option<Arc<dyn TodoSchedulerSsePublisher + Send + Sync>>,
    save_concurrency: usize,
    side_effect_concurrency: usize,
}

impl TodoSchedulerService {
    pub fn new(repo: Arc<dyn TodoRepository + Send + Sync>) -> Self {
        Self {
            repo,
            notification_service: None,
            sse_publisher: None,
            save_concurrency: DEFAULT_SAVE_CONCURRENCY,
            side_effect_concurrency: DEFAULT_SIDE_EFFECT_CONCURRENCY,
        }
    }

    pub fn with_notification_service(mut self, notification_service: Arc<dyn TodoSchedulerNotificationSender>) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub fn with_sse_publisher(mut self, sse_publisher: Arc<dyn TodoSchedulerSsePublisher + Send + Sync>) -> Self {
        self.sse_publisher = Some(sse_publisher);
        self
    }

    pub async fn run_once(&self) -> Result<TodoSchedulerRunSummary, DomainError> {
        Ok(TodoSchedulerRunSummary {
            overdue_ids: self.check_overdue_todos().await?,
            unblocked_ids: self.check_blocked_todos().await?,
            escalated_ids: self.auto_escalate_priority().await?,
        })
    }

    pub async fn check_overdue_todos(&self) -> Result<Vec<String>, DomainError> {
        let now = Utc::now();
        let todos = self.load_by_status(TodoStatus::InProgress).await?;

        let mut updated_todos = Vec::new();
        let mut side_effects = Vec::new();
        let mut escalated_ids = Vec::new();

        for mut todo in todos {
            let Some(due_date) = todo.due_date else {
                continue;
            };
            if due_date >= now {
                continue;
            }

            let target_priority = TodoPriority::Critical;
            if !should_escalate(todo.priority, target_priority) {
                continue;
            }

            apply_priority_update(&mut todo, target_priority);
            escalated_ids.push(todo.todo_id.clone());
            side_effects.push(SchedulerSideEffect {
                todo: todo.clone(),
                title: "Todo overdue".to_string(),
                body: format!(
                    "Todo '{}' is overdue and its priority was raised to {}.",
                    todo.title,
                    priority_code(target_priority)
                ),
                severity: "critical".to_string(),
                event: "todo_overdue_escalated".to_string(),
                event_payload: json!({
                    "priority": priority_code(target_priority),
                }),
            });
            updated_todos.push(todo);
        }

        self.save_todos(updated_todos).await;
        self.dispatch_side_effects(side_effects).await;
        Ok(escalated_ids)
    }

    pub async fn check_blocked_todos(&self) -> Result<Vec<String>, DomainError> {
        let blocked_todos = self.load_by_status(TodoStatus::Blocked).await?;

        let mut updated_todos = Vec::new();
        let mut side_effects = Vec::new();
        let mut unlocked_ids = Vec::new();

        for mut todo in blocked_todos {
            if !self.dependencies_completed(&todo.depends_on).await {
                continue;
            }

            match todo.update_status(TodoStatus::Pending, TODO_SCHEDULER_ACTOR) {
                Ok(()) => {
                    unlocked_ids.push(todo.todo_id.clone());
                    side_effects.push(SchedulerSideEffect {
                        todo: todo.clone(),
                        title: "Todo unblocked".to_string(),
                        body: format!(
                            "Todo '{}' was unblocked because dependencies are completed.",
                            todo.title
                        ),
                        severity: "info".to_string(),
                        event: "todo_unblocked".to_string(),
                        event_payload: json!({
                            "status": status_code(TodoStatus::Pending),
                        }),
                    });
                    updated_todos.push(todo);
                }
                Err(error) => {
                    warn!(todo_id = %todo.todo_id, error = %error, "failed to unlock blocked todo");
                }
            }
        }

        self.save_todos(updated_todos).await;
        self.dispatch_side_effects(side_effects).await;
        Ok(unlocked_ids)
    }

    pub async fn auto_escalate_priority(&self) -> Result<Vec<String>, DomainError> {
        let candidates = self.load_active_todos().await?;
        let now = Utc::now();

        let mut updated_todos = Vec::new();
        let mut side_effects = Vec::new();
        let mut escalated_ids = Vec::new();

        for mut todo in candidates {
            let Some(due_date) = todo.due_date else {
                continue;
            };

            let seconds_to_due = (due_date - now).num_seconds();
            if seconds_to_due < 0 {
                continue;
            }

            let (target_priority, severity) = if seconds_to_due <= 30 * 60 {
                (TodoPriority::Critical, "critical")
            } else if seconds_to_due <= 120 * 60 {
                (TodoPriority::High, "warning")
            } else {
                continue;
            };

            if !should_escalate(todo.priority, target_priority) {
                continue;
            }

            let due_in_minutes = (seconds_to_due / 60).max(0);
            apply_priority_update(&mut todo, target_priority);
            escalated_ids.push(todo.todo_id.clone());
            side_effects.push(SchedulerSideEffect {
                todo: todo.clone(),
                title: "Todo priority auto-escalated".to_string(),
                body: format!(
                    "Todo '{}' is due in about {} minutes. Priority raised to {}.",
                    todo.title,
                    due_in_minutes,
                    priority_code(target_priority)
                ),
                severity: severity.to_string(),
                event: "todo_priority_auto_escalated".to_string(),
                event_payload: json!({
                    "priority": priority_code(target_priority),
                    "due_in_minutes": due_in_minutes,
                }),
            });
            updated_todos.push(todo);
        }

        self.save_todos(updated_todos).await;
        self.dispatch_side_effects(side_effects).await;
        Ok(escalated_ids)
    }

    async fn load_by_status(&self, status: TodoStatus) -> Result<Vec<Todo>, DomainError> {
        self.repo
            .find_all(Some(status), None, None, None, None, None, DEFAULT_QUERY_LIMIT, 0)
            .await
    }

    async fn load_active_todos(&self) -> Result<Vec<Todo>, DomainError> {
        Ok(self
            .repo
            .find_all(None, None, None, None, None, None, DEFAULT_QUERY_LIMIT, 0)
            .await?
            .into_iter()
            .filter(|todo| !todo.status.is_terminal())
            .collect())
    }

    async fn dependencies_completed(&self, dependency_ids: &[String]) -> bool {
        if dependency_ids.is_empty() {
            return true;
        }

        let mut normalized_ids = Vec::new();
        let mut seen_ids = HashSet::new();
        for dependency_id in dependency_ids {
            let trimmed = dependency_id.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen_ids.insert(trimmed.to_string()) {
                normalized_ids.push(trimmed.to_string());
            }
        }

        if normalized_ids.is_empty() {
            return true;
        }

        let batch = match self.repo.find_by_ids(&normalized_ids).await {
            Ok(batch) => batch,
            Err(error) => {
                warn!(error = %error, "batch dependency lookup failed in todo scheduler");
                return false;
            }
        };

        let dependency_map: HashMap<String, Todo> =
            batch.into_iter().map(|todo| (todo.todo_id.clone(), todo)).collect();

        for dependency_id in normalized_ids {
            let Some(todo) = dependency_map.get(&dependency_id) else {
                return false;
            };
            if todo.status != TodoStatus::Completed {
                return false;
            }
        }

        true
    }

    async fn save_todos(&self, todos: Vec<Todo>) {
        if todos.is_empty() {
            return;
        }

        let repo = self.repo.clone();
        stream::iter(todos.into_iter())
            .for_each_concurrent(self.save_concurrency, move |todo| {
                let repo = repo.clone();
                async move {
                    if let Err(error) = repo.save(&todo).await {
                        warn!(todo_id = %todo.todo_id, error = %error, "todo scheduler save failed");
                    }
                }
            })
            .await;
    }

    async fn dispatch_side_effects(&self, side_effects: Vec<SchedulerSideEffect>) {
        if side_effects.is_empty() {
            return;
        }

        let notification_service = self.notification_service.clone();
        let sse_publisher = self
            .sse_publisher
            .clone()
            .map(|p| p as Arc<dyn TodoSchedulerSsePublisher>);
        stream::iter(side_effects.into_iter())
            .for_each_concurrent(self.side_effect_concurrency, move |effect| {
                let notification_service = notification_service.clone();
                let sse_publisher = sse_publisher.clone();
                async move {
                    notify_assignee(notification_service, &effect).await;
                    emit_scheduler_event(sse_publisher, &effect).await;
                }
            })
            .await;
    }
}

async fn notify_assignee(
    notification_service: Option<Arc<dyn TodoSchedulerNotificationSender>>,
    effect: &SchedulerSideEffect,
) {
    let Some(notification_service) = notification_service else {
        return;
    };

    let assignee = effect
        .todo
        .assigned_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let Some(assignee) = assignee else {
        return;
    };

    if let Err(error) = notification_service
        .send_scheduler_notification(NotificationCreate {
            user_id: assignee,
            title: effect.title.clone(),
            body: effect.body.clone(),
            category: Some("escalation".to_string()),
            severity: Some(effect.severity.clone()),
            flight_id: None,
            related_entity_type: Some("todo".to_string()),
            related_entity_id: Some(effect.todo.todo_id.clone()),
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: None,
            sender_username_snapshot: None,
            origin_type: None,
            receipt_required: false,
            receipt_group_id: None,
        })
        .await
    {
        warn!(
            todo_id = %effect.todo.todo_id,
            error = %error,
            "failed to send todo scheduler notification"
        );
    }
}

async fn emit_scheduler_event(sse_publisher: Option<Arc<dyn TodoSchedulerSsePublisher>>, effect: &SchedulerSideEffect) {
    let Some(sse_publisher) = sse_publisher else {
        return;
    };

    let payload = scheduler_event_payload(&effect.event, &effect.todo.todo_id, effect.event_payload.as_object());
    if let Err(error) = sse_publisher.publish_system_alert(payload).await {
        warn!(
            todo_id = %effect.todo.todo_id,
            error = %error,
            "failed to broadcast todo scheduler SSE event"
        );
    }
}

fn scheduler_event_payload(
    event: &str,
    todo_id: &str,
    extra_payload: Option<&serde_json::Map<String, Value>>,
) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("type".to_string(), Value::String(event.to_string()));
    payload.insert("todo_id".to_string(), Value::String(todo_id.to_string()));
    payload.insert("timestamp".to_string(), Value::String(Utc::now().to_rfc3339()));
    if let Some(extra_payload) = extra_payload {
        for (key, value) in extra_payload {
            payload.insert(key.clone(), value.clone());
        }
    }
    Value::Object(payload)
}

fn should_escalate(current: TodoPriority, target: TodoPriority) -> bool {
    target.level() < current.level()
}

fn apply_priority_update(todo: &mut Todo, priority: TodoPriority) {
    todo.priority = priority;
    todo.updated_at = Utc::now();
    todo.updated_by = TODO_SCHEDULER_ACTOR.to_string();
    todo.version += 1;
}

fn priority_code(priority: TodoPriority) -> &'static str {
    match priority {
        TodoPriority::Critical => "critical",
        TodoPriority::High => "high",
        TodoPriority::Medium => "medium",
        TodoPriority::Low => "low",
        TodoPriority::Background => "background",
    }
}

fn status_code(status: TodoStatus) -> &'static str {
    match status {
        TodoStatus::Pending => "pending",
        TodoStatus::InProgress => "in_progress",
        TodoStatus::Completed => "completed",
        TodoStatus::Cancelled => "cancelled",
        TodoStatus::Blocked => "blocked",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    use chrono::{Duration, Utc};

    use fms_domain::models::notification::{Notification, NotificationPreference};
    use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};

    use super::*;

    #[derive(Default)]
    struct FakeTodoRepo {
        todos: Mutex<HashMap<String, Todo>>,
    }

    impl FakeTodoRepo {
        fn new(items: Vec<Todo>) -> Self {
            Self {
                todos: Mutex::new(items.into_iter().map(|todo| (todo.todo_id.clone(), todo)).collect()),
            }
        }

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
            status: Option<TodoStatus>,
            priority: Option<TodoPriority>,
            category: Option<&str>,
            assigned_to: Option<&str>,
            source_type: Option<&str>,
            source_id: Option<&str>,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<Todo>, DomainError> {
            let mut items = self
                .todos
                .lock()
                .expect("lock todos")
                .values()
                .cloned()
                .collect::<Vec<_>>();
            items.retain(|todo| !todo.is_deleted);
            if let Some(status) = status {
                items.retain(|todo| todo.status == status);
            }
            if let Some(priority) = priority {
                items.retain(|todo| todo.priority == priority);
            }
            if let Some(category) = category {
                let normalized = category.trim().to_ascii_lowercase();
                items.retain(|todo| {
                    todo.category
                        .map(|value| format!("{:?}", value).to_ascii_lowercase())
                        .as_deref()
                        == Some(normalized.as_str())
                });
            }
            if let Some(assigned_to) = assigned_to {
                items.retain(|todo| todo.assigned_to.as_deref() == Some(assigned_to));
            }
            if let Some(source_type) = source_type {
                items.retain(|todo| todo.source_type == source_type);
            }
            if let Some(source_id) = source_id {
                items.retain(|todo| todo.source_id.as_deref() == Some(source_id));
            }

            items.sort_by(|left, right| left.todo_id.cmp(&right.todo_id));
            Ok(items
                .into_iter()
                .skip(offset.max(0) as usize)
                .take(limit.max(0) as usize)
                .collect())
        }

        async fn find_by_ids(&self, todo_ids: &[String]) -> Result<Vec<Todo>, DomainError> {
            let todos = self.todos.lock().expect("lock todos");
            Ok(todo_ids
                .iter()
                .filter_map(|todo_id| todos.get(todo_id).cloned())
                .collect())
        }

        async fn find_by_source(&self, source_type: &str, source_id: &str) -> Result<Vec<Todo>, DomainError> {
            self.find_all(None, None, None, None, Some(source_type), Some(source_id), 1000, 0)
                .await
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
            let mut todos = self.todos.lock().expect("lock todos");
            let Some(todo) = todos.get_mut(todo_id) else {
                return Ok(false);
            };
            todo.is_deleted = true;
            Ok(true)
        }

        async fn count_by_status(&self, status: TodoStatus) -> Result<i64, DomainError> {
            Ok(self
                .todos
                .lock()
                .expect("lock todos")
                .values()
                .filter(|todo| !todo.is_deleted && todo.status == status)
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
    }

    #[derive(Default)]
    struct FakeNotificationRepo {
        notifications: Mutex<HashMap<String, Notification>>,
    }

    impl FakeNotificationRepo {
        fn items(&self) -> Vec<Notification> {
            self.notifications
                .lock()
                .expect("lock notifications")
                .values()
                .cloned()
                .collect()
        }
    }

    #[async_trait::async_trait]
    impl NotificationRepository for FakeNotificationRepo {
        async fn save(&self, notification: &Notification) -> Result<(), DomainError> {
            self.notifications
                .lock()
                .expect("lock notifications")
                .insert(notification.notification_id.clone(), notification.clone());
            Ok(())
        }

        async fn find_by_id(&self, notification_id: &str) -> Result<Option<Notification>, DomainError> {
            Ok(self
                .notifications
                .lock()
                .expect("lock notifications")
                .get(notification_id)
                .cloned())
        }

        async fn find_by_id_for_user(
            &self,
            notification_id: &str,
            user_id: &str,
        ) -> Result<Option<Notification>, DomainError> {
            Ok(self
                .notifications
                .lock()
                .expect("lock notifications")
                .get(notification_id)
                .filter(|notification| notification.user_id == user_id)
                .cloned())
        }

        async fn find_by_user(
            &self,
            user_id: &str,
            unread_only: bool,
            limit: i64,
            offset: i64,
        ) -> Result<Vec<Notification>, DomainError> {
            let mut items = self
                .notifications
                .lock()
                .expect("lock notifications")
                .values()
                .filter(|notification| notification.user_id == user_id)
                .filter(|notification| !unread_only || !notification.is_read)
                .cloned()
                .collect::<Vec<_>>();
            items.sort_by(|left, right| left.created_at.cmp(&right.created_at));
            Ok(items
                .into_iter()
                .skip(offset.max(0) as usize)
                .take(limit.max(0) as usize)
                .collect())
        }

        async fn mark_read(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError> {
            let mut notifications = self.notifications.lock().expect("lock notifications");
            let Some(notification) = notifications.get_mut(notification_id) else {
                return Ok(false);
            };
            if notification.user_id != user_id {
                return Ok(false);
            }
            notification.is_read = true;
            notification.read_at = Some(Utc::now());
            Ok(true)
        }

        async fn mark_delivered(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError> {
            let mut notifications = self.notifications.lock().expect("lock notifications");
            let Some(notification) = notifications.get_mut(notification_id) else {
                return Ok(false);
            };
            if notification.user_id != user_id {
                return Ok(false);
            }
            notification.delivered_at = Some(Utc::now());
            Ok(true)
        }

        async fn mark_all_read(&self, user_id: &str) -> Result<i64, DomainError> {
            let mut updated = 0;
            let mut notifications = self.notifications.lock().expect("lock notifications");
            for notification in notifications.values_mut() {
                if notification.user_id == user_id && !notification.is_read {
                    notification.is_read = true;
                    notification.read_at = Some(Utc::now());
                    updated += 1;
                }
            }
            Ok(updated)
        }

        async fn count_unread(&self, user_id: &str) -> Result<i64, DomainError> {
            Ok(self
                .notifications
                .lock()
                .expect("lock notifications")
                .values()
                .filter(|notification| notification.user_id == user_id && !notification.is_read)
                .count() as i64)
        }

        async fn acknowledge(
            &self,
            notification_id: &str,
            user_id: &str,
            action: &str,
            note: Option<&str>,
        ) -> Result<Option<Notification>, DomainError> {
            let mut notifications = self.notifications.lock().expect("lock notifications");
            let Some(notification) = notifications.get_mut(notification_id) else {
                return Ok(None);
            };
            if notification.user_id != user_id {
                return Ok(None);
            }
            notification.ack_status = action.to_string();
            notification.ack_note = note.map(ToOwned::to_owned);
            notification.ack_at = Some(Utc::now());
            Ok(Some(notification.clone()))
        }

        async fn find_by_receipt_group(&self, receipt_group_id: &str) -> Result<Vec<Notification>, DomainError> {
            Ok(self
                .notifications
                .lock()
                .expect("lock notifications")
                .values()
                .filter(|notification| notification.receipt_group_id.as_deref() == Some(receipt_group_id))
                .cloned()
                .collect())
        }

        async fn summarize_receipt_group(&self, _receipt_group_id: &str) -> Result<Option<Value>, DomainError> {
            Ok(None)
        }

        async fn list_sent_receipt_groups(
            &self,
            _sender_user_id: &str,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<Value>, DomainError> {
            Ok(Vec::new())
        }
    }

    struct FakePreferenceRepo;

    #[async_trait::async_trait]
    impl NotificationPreferenceRepository for FakePreferenceRepo {
        async fn find_by_user(&self, _user_id: &str) -> Result<Option<NotificationPreference>, DomainError> {
            Ok(None)
        }

        async fn save(&self, _pref: &NotificationPreference) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeTodoSchedulerPublisher {
        payloads: Mutex<Vec<Value>>,
    }

    impl FakeTodoSchedulerPublisher {
        fn payloads(&self) -> Vec<Value> {
            self.payloads.lock().expect("lock payloads").clone()
        }
    }

    impl TodoSchedulerSsePublisher for FakeTodoSchedulerPublisher {
        fn publish_system_alert<'a>(
            &'a self,
            payload: Value,
        ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
            Box::pin(async move {
                self.payloads.lock().expect("lock payloads").push(payload);
                Ok(1)
            })
        }
    }

    fn build_service(
        todo_repo: Arc<FakeTodoRepo>,
        notification_repo: Arc<FakeNotificationRepo>,
        publisher: Arc<FakeTodoSchedulerPublisher>,
    ) -> TodoSchedulerService {
        let notification_service = Arc::new(NotificationService::new(
            notification_repo,
            Arc::new(FakePreferenceRepo),
        ));
        TodoSchedulerService::new(todo_repo)
            .with_notification_service(notification_service)
            .with_sse_publisher(publisher)
    }

    fn sample_todo(
        todo_id: &str,
        title: &str,
        status: TodoStatus,
        priority: TodoPriority,
        due_date: Option<chrono::DateTime<Utc>>,
        depends_on: Vec<String>,
        assigned_to: Option<&str>,
    ) -> Todo {
        let now = Utc::now();
        Todo {
            todo_id: todo_id.to_string(),
            title: title.to_string(),
            description: None,
            priority,
            status,
            category: None,
            due_date,
            assigned_to: assigned_to.map(ToOwned::to_owned),
            tags: vec![],
            estimated_duration: None,
            actual_duration: None,
            progress: 25,
            is_recurring: false,
            recurring_pattern: None,
            parent_todo_id: None,
            execution_order: 0,
            depends_on,
            source_type: "manual".to_string(),
            source_id: None,
            is_deleted: false,
            deleted_at: None,
            created_at: now,
            updated_at: now,
            created_by: "tester".to_string(),
            updated_by: "tester".to_string(),
            version: 1,
        }
    }

    #[tokio::test]
    async fn check_overdue_todos_escalates_priority_and_dispatches_side_effects() {
        let todo_repo = Arc::new(FakeTodoRepo::new(vec![
            sample_todo(
                "todo-overdue",
                "Follow up flight ops",
                TodoStatus::InProgress,
                TodoPriority::High,
                Some(Utc::now() - Duration::minutes(10)),
                vec![],
                Some("alice"),
            ),
            sample_todo(
                "todo-on-time",
                "Still on time",
                TodoStatus::InProgress,
                TodoPriority::Medium,
                Some(Utc::now() + Duration::minutes(30)),
                vec![],
                Some("bob"),
            ),
        ]));
        let notification_repo = Arc::new(FakeNotificationRepo::default());
        let publisher = Arc::new(FakeTodoSchedulerPublisher::default());
        let service = build_service(todo_repo.clone(), notification_repo.clone(), publisher.clone());

        let escalated_ids = service.check_overdue_todos().await.expect("scheduler succeeds");

        assert_eq!(escalated_ids, vec!["todo-overdue".to_string()]);
        let updated = todo_repo.todo("todo-overdue");
        assert_eq!(updated.priority, TodoPriority::Critical);
        assert_eq!(updated.updated_by, TODO_SCHEDULER_ACTOR);

        let notifications = notification_repo.items();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].user_id, "alice");
        assert_eq!(notifications[0].title, "Todo overdue");
        assert_eq!(notifications[0].severity, "critical");
        assert_eq!(notifications[0].category, "escalation");
        assert_eq!(notifications[0].related_entity_type.as_deref(), Some("todo"));
        assert_eq!(notifications[0].related_entity_id.as_deref(), Some("todo-overdue"));

        let payloads = publisher.payloads();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["type"], json!("todo_overdue_escalated"));
        assert_eq!(payloads[0]["todo_id"], json!("todo-overdue"));
        assert_eq!(payloads[0]["priority"], json!("critical"));
    }

    #[tokio::test]
    async fn check_blocked_todos_unblocks_only_when_dependencies_are_completed() {
        let todo_repo = Arc::new(FakeTodoRepo::new(vec![
            sample_todo(
                "blocked-ready",
                "Ready to unblock",
                TodoStatus::Blocked,
                TodoPriority::Medium,
                None,
                vec!["dep-1".to_string(), "dep-2".to_string()],
                Some("carol"),
            ),
            sample_todo(
                "blocked-waiting",
                "Still waiting",
                TodoStatus::Blocked,
                TodoPriority::Medium,
                None,
                vec!["dep-3".to_string()],
                Some("dave"),
            ),
            sample_todo(
                "dep-1",
                "Done 1",
                TodoStatus::Completed,
                TodoPriority::Low,
                None,
                vec![],
                None,
            ),
            sample_todo(
                "dep-2",
                "Done 2",
                TodoStatus::Completed,
                TodoPriority::Low,
                None,
                vec![],
                None,
            ),
            sample_todo(
                "dep-3",
                "Not done",
                TodoStatus::InProgress,
                TodoPriority::Low,
                None,
                vec![],
                None,
            ),
        ]));
        let notification_repo = Arc::new(FakeNotificationRepo::default());
        let publisher = Arc::new(FakeTodoSchedulerPublisher::default());
        let service = build_service(todo_repo.clone(), notification_repo.clone(), publisher.clone());

        let unlocked_ids = service.check_blocked_todos().await.expect("scheduler succeeds");

        assert_eq!(unlocked_ids, vec!["blocked-ready".to_string()]);
        let ready = todo_repo.todo("blocked-ready");
        assert_eq!(ready.status, TodoStatus::Pending);
        assert_eq!(ready.progress, 0);
        assert_eq!(ready.updated_by, TODO_SCHEDULER_ACTOR);

        let waiting = todo_repo.todo("blocked-waiting");
        assert_eq!(waiting.status, TodoStatus::Blocked);

        let notifications = notification_repo.items();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].title, "Todo unblocked");
        assert_eq!(notifications[0].user_id, "carol");

        let payloads = publisher.payloads();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["type"], json!("todo_unblocked"));
        assert_eq!(payloads[0]["status"], json!("pending"));
    }

    #[tokio::test]
    async fn auto_escalate_priority_promotes_due_soon_todos() {
        let todo_repo = Arc::new(FakeTodoRepo::new(vec![
            sample_todo(
                "due-critical",
                "Critical soon",
                TodoStatus::Pending,
                TodoPriority::High,
                Some(Utc::now() + Duration::minutes(20)),
                vec![],
                Some("eve"),
            ),
            sample_todo(
                "due-high",
                "High soon",
                TodoStatus::InProgress,
                TodoPriority::Medium,
                Some(Utc::now() + Duration::minutes(90)),
                vec![],
                Some("frank"),
            ),
            sample_todo(
                "already-critical",
                "Already critical",
                TodoStatus::Pending,
                TodoPriority::Critical,
                Some(Utc::now() + Duration::minutes(10)),
                vec![],
                Some("grace"),
            ),
            sample_todo(
                "terminal",
                "Completed item",
                TodoStatus::Completed,
                TodoPriority::Low,
                Some(Utc::now() + Duration::minutes(10)),
                vec![],
                Some("heidi"),
            ),
            sample_todo(
                "already-overdue",
                "Past due",
                TodoStatus::Pending,
                TodoPriority::Low,
                Some(Utc::now() - Duration::minutes(5)),
                vec![],
                Some("ivan"),
            ),
        ]));
        let notification_repo = Arc::new(FakeNotificationRepo::default());
        let publisher = Arc::new(FakeTodoSchedulerPublisher::default());
        let service = build_service(todo_repo.clone(), notification_repo.clone(), publisher.clone());

        let escalated_ids = service.auto_escalate_priority().await.expect("scheduler succeeds");

        assert_eq!(escalated_ids.len(), 2);
        assert!(escalated_ids.contains(&"due-critical".to_string()));
        assert!(escalated_ids.contains(&"due-high".to_string()));

        assert_eq!(todo_repo.todo("due-critical").priority, TodoPriority::Critical);
        assert_eq!(todo_repo.todo("due-high").priority, TodoPriority::High);
        assert_eq!(todo_repo.todo("already-critical").priority, TodoPriority::Critical);
        assert_eq!(todo_repo.todo("terminal").priority, TodoPriority::Low);

        let notifications = notification_repo.items();
        assert_eq!(notifications.len(), 2);
        assert!(notifications
            .iter()
            .any(|item| item.user_id == "eve" && item.severity == "critical"));
        assert!(notifications
            .iter()
            .any(|item| item.user_id == "frank" && item.severity == "warning"));

        let payloads = publisher.payloads();
        assert_eq!(payloads.len(), 2);
        assert!(payloads
            .iter()
            .any(|item| { item["todo_id"] == json!("due-critical") && item["priority"] == json!("critical") }));
        assert!(payloads
            .iter()
            .any(|item| { item["todo_id"] == json!("due-high") && item["priority"] == json!("high") }));
    }
}
