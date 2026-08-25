use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::test_support::notification_service_without_side_channels;

use super::service::*;
use fms_domain::error::DomainError;
use fms_domain::models::notification::{Notification, NotificationPreference};
use fms_domain::models::todo::{Todo, TodoPriority, TodoStatus};
use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};
use fms_domain::ports::todo_agent_context_repository::{TodoAgentContext, TodoAgentContextRepository};
use fms_domain::ports::todo_repository::TodoRepository;
use std::sync::Mutex;

async fn wait_for_prune_to_finish(service: &AiRuntimeService) {
    for _ in 0..50 {
        if !service.is_prune_scheduled() {
            return;
        }
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("prune task did not finish");
}

#[derive(Default)]
struct FakeNotificationRepo {
    items: Mutex<Vec<Notification>>,
}

#[async_trait::async_trait]
impl NotificationRepository for FakeNotificationRepo {
    async fn save(&self, notification: &Notification) -> Result<(), DomainError> {
        self.items
            .lock()
            .expect("lock notifications")
            .push(notification.clone());
        Ok(())
    }

    async fn find_by_id(&self, notification_id: &str) -> Result<Option<Notification>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock notifications")
            .iter()
            .find(|item| item.notification_id == notification_id)
            .cloned())
    }

    async fn find_by_id_for_user(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<Option<Notification>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock notifications")
            .iter()
            .find(|item| item.notification_id == notification_id && item.user_id == user_id)
            .cloned())
    }

    async fn find_by_user(
        &self,
        user_id: &str,
        unread_only: bool,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock notifications")
            .iter()
            .filter(|item| item.user_id == user_id && (!unread_only || !item.is_read))
            .cloned()
            .collect())
    }

    async fn mark_read(&self, _notification_id: &str, _user_id: &str) -> Result<bool, DomainError> {
        Ok(false)
    }

    async fn mark_delivered(&self, _notification_id: &str, _user_id: &str) -> Result<bool, DomainError> {
        Ok(false)
    }

    async fn mark_all_read(&self, _user_id: &str) -> Result<i64, DomainError> {
        Ok(0)
    }

    async fn count_unread(&self, user_id: &str) -> Result<i64, DomainError> {
        Ok(self
            .items
            .lock()
            .expect("lock notifications")
            .iter()
            .filter(|item| item.user_id == user_id && !item.is_read)
            .count() as i64)
    }

    async fn acknowledge(
        &self,
        _notification_id: &str,
        _user_id: &str,
        _action: &str,
        _note: Option<&str>,
    ) -> Result<Option<Notification>, DomainError> {
        Ok(None)
    }

    async fn find_by_receipt_group(&self, _receipt_group_id: &str) -> Result<Vec<Notification>, DomainError> {
        Ok(vec![])
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
        Ok(vec![])
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

struct FakeTodoRepo {
    items: Vec<Todo>,
}

#[async_trait::async_trait]
impl TodoRepository for FakeTodoRepo {
    async fn find_by_id(&self, todo_id: &str) -> Result<Option<Todo>, DomainError> {
        Ok(self.items.iter().find(|item| item.todo_id == todo_id).cloned())
    }

    async fn find_all(
        &self,
        _status: Option<TodoStatus>,
        _priority: Option<TodoPriority>,
        _category: Option<&str>,
        _assigned_to: Option<&str>,
        _source_type: Option<&str>,
        _source_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Todo>, DomainError> {
        Ok(self
            .items
            .iter()
            .skip(offset.max(0) as usize)
            .take(limit.max(0) as usize)
            .cloned()
            .collect())
    }

    async fn find_by_ids(&self, todo_ids: &[String]) -> Result<Vec<Todo>, DomainError> {
        let todo_ids = todo_ids.iter().cloned().collect::<HashSet<_>>();
        Ok(self
            .items
            .iter()
            .filter(|item| todo_ids.contains(&item.todo_id))
            .cloned()
            .collect())
    }

    async fn find_by_source(&self, _source_type: &str, _source_id: &str) -> Result<Vec<Todo>, DomainError> {
        Ok(Vec::new())
    }

    async fn find_overdue(&self) -> Result<Vec<Todo>, DomainError> {
        Ok(Vec::new())
    }

    async fn find_children(&self, _parent_todo_id: &str) -> Result<Vec<Todo>, DomainError> {
        Ok(Vec::new())
    }

    async fn save(&self, _todo: &Todo) -> Result<(), DomainError> {
        Ok(())
    }

    async fn update(&self, _todo: &Todo) -> Result<bool, DomainError> {
        Ok(true)
    }

    async fn soft_delete(&self, _todo_id: &str, _deleted_by: &str) -> Result<bool, DomainError> {
        Ok(false)
    }

    async fn count_by_status(&self, status: TodoStatus) -> Result<i64, DomainError> {
        Ok(self.items.iter().filter(|item| item.status == status).count() as i64)
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

struct FakeTodoAgentContextRepo {
    items: HashMap<String, TodoAgentContext>,
}

#[async_trait::async_trait]
impl TodoAgentContextRepository for FakeTodoAgentContextRepo {
    async fn get(&self, todo_id: &str) -> Result<Option<TodoAgentContext>, DomainError> {
        Ok(self.items.get(todo_id).cloned())
    }

    async fn batch_get(&self, todo_ids: &[String]) -> Result<HashMap<String, TodoAgentContext>, DomainError> {
        Ok(todo_ids
            .iter()
            .filter_map(|todo_id| self.items.get(todo_id).cloned().map(|ctx| (todo_id.clone(), ctx)))
            .collect())
    }

    async fn upsert_partial(
        &self,
        _todo_id: &str,
        _agent_entity_id: Option<&str>,
        _agent_run_id: Option<&str>,
        _agent_status: Option<&str>,
        _updated_by: &str,
    ) -> Result<TodoAgentContext, DomainError> {
        Err(DomainError::Internal("not implemented".to_string()))
    }

    async fn find_todo_ids(
        &self,
        _agent_status: Option<&str>,
        agent_entity_id: Option<&str>,
        _agent_run_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<String>, DomainError> {
        let normalized_entity = agent_entity_id.unwrap_or_default();
        Ok(self
            .items
            .values()
            .filter(|item| item.agent_entity_id == normalized_entity)
            .skip(offset.max(0) as usize)
            .take(limit.max(0) as usize)
            .map(|item| item.todo_id.clone())
            .collect())
    }

    fn get_metrics_snapshot(&self) -> HashMap<String, serde_json::Value> {
        HashMap::new()
    }
}

fn test_todo(todo_id: &str, created_at: DateTime<Utc>) -> Todo {
    Todo {
        todo_id: todo_id.to_string(),
        title: format!("Todo {todo_id}"),
        description: None,
        priority: TodoPriority::Medium,
        status: TodoStatus::Pending,
        category: None,
        due_date: None,
        assigned_to: None,
        tags: vec![],
        estimated_duration: None,
        actual_duration: None,
        progress: 0,
        is_recurring: false,
        recurring_pattern: None,
        parent_todo_id: None,
        execution_order: 0,
        depends_on: vec![],
        source_type: "ai".to_string(),
        source_id: None,
        is_deleted: false,
        deleted_at: None,
        created_at,
        updated_at: created_at,
        created_by: "tester".to_string(),
        updated_by: "tester".to_string(),
        version: 1,
    }
}

#[test]
fn execution_record_exposes_normalized_runtime_fields() {
    let now = Utc::now();
    let payload = ExecutionRecord::pending_approval(
        "run_graph_001".to_string(),
        "change_stand".to_string(),
        json!({"stand_id": "S1"}),
        Some("user_001".to_string()),
        vec!["dispatcher".to_string()],
        Some("pending_001".to_string()),
        now,
    )
    .to_value();

    assert_eq!(payload["runtime_path"], "legacy");
    assert_eq!(payload["runtime_status"], "pending_approval");
    assert_eq!(payload["runtime"]["path"], "legacy");
    assert_eq!(payload["runtime"]["requested_path"], "legacy");
    assert_eq!(payload["runtime"]["resumable"], true);
    assert_eq!(payload["runtime"]["tool_names"][0], "change_stand");
    assert!(payload["runtime"]["fallback_reason"].is_null());
    println!("ai runtime metadata payload: {}", payload);
}

#[tokio::test]
async fn get_execution_falls_back_to_pending_action_correlation() {
    let service = AiRuntimeService::new();
    let now = Utc::now();

    {
        let mut state = service.state.write().await;
        state.pending_actions.insert(
            "pending_001".to_string(),
            PendingActionRecord::new(
                "pending_001".to_string(),
                AiToolExecutionSpec {
                    tool_name: "update_todo".to_string(),
                    category: "todo".to_string(),
                    operation_level: "l1_write".to_string(),
                    side_effect: true,
                    query_intent: None,
                    query_dataset: None,
                },
                json!({"todo_id": "todo_1"}),
                "call_001".to_string(),
                Some("user_001".to_string()),
                vec!["dispatcher".to_string()],
                "req_nl_123".to_string(),
                now,
            ),
        );
    }

    let payload = service
        .get_execution("req_nl_123")
        .await
        .expect("pending action fallback should exist");

    assert_eq!(payload["execution_id"], "req_nl_123");
    assert_eq!(payload["run_id"], "req_nl_123");
    assert_eq!(payload["status"], "pending");
    assert_eq!(payload["runtime"]["status"], "pending");
    assert_eq!(payload["pending_action"]["action_id"], "pending_001");
    println!("ai execution fallback payload: {}", payload);
}

#[tokio::test]
async fn list_pending_actions_uses_python_page_total_semantics() {
    let service = AiRuntimeService::new();
    let now = Utc::now();

    {
        let mut state = service.state.write().await;
        for idx in 0..3 {
            let action_id = format!("pending_{idx}");
            state.pending_order.push(action_id.clone());
            state.pending_actions.insert(
                action_id.clone(),
                PendingActionRecord::new(
                    action_id,
                    AiToolExecutionSpec {
                        tool_name: "update_todo".to_string(),
                        category: "todo".to_string(),
                        operation_level: "l1_write".to_string(),
                        side_effect: true,
                        query_intent: None,
                        query_dataset: None,
                    },
                    json!({"todo_id": idx}),
                    format!("call_{idx}"),
                    Some("requester_001".to_string()),
                    vec!["dispatcher".to_string()],
                    format!("exec_{idx}"),
                    now,
                ),
            );
        }
    }

    let payload = service.list_pending_actions(None, None, 2, 1).await;
    assert_eq!(payload["total"], 2);
    assert_eq!(payload["total_count"], 3);
    assert_eq!(payload["pagination"]["offset"], 1);
    assert_eq!(payload["pagination"]["has_more"], false);
    println!("ai pending-actions pagination payload: {}", payload);
}

#[tokio::test]
async fn approve_pending_action_notifies_requester() {
    let repo = Arc::new(FakeNotificationRepo::default());
    let notification_service = Arc::new(notification_service_without_side_channels(
        repo.clone(),
        Arc::new(FakePreferenceRepo),
    ));
    let service = AiRuntimeService::new().with_notification_service(notification_service);

    let pending = service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "update_todo".to_string(),
                category: "todo".to_string(),
                operation_level: "l1_write".to_string(),
                side_effect: true,
                query_intent: None,
                query_dataset: None,
            },
            json!({"todo_id": "todo_1"}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;
    let action_id = pending["approval_id"].as_str().expect("approval id").to_string();

    {
        let notifications = repo.items.lock().expect("lock notifications");
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].user_id, "requester_001");
        assert_eq!(notifications[0].title, "AI 工具 'update_todo' 已进入审批队列");
        assert_eq!(
            notifications[0].body,
            format!("动作 {} 状态: pending；原因: todo requires human approval", action_id)
        );
        println!("ai pending notification title: {}", notifications[0].title);
        println!("ai pending notification body: {}", notifications[0].body);
    }

    let result = service
        .approve_pending_action(&action_id, "approver_001", None)
        .await
        .expect("approve pending action");

    let notifications = repo.items.lock().expect("lock notifications");
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[1].user_id, "requester_001");
    assert_eq!(notifications[1].title, "AI 工具 'update_todo' 审批已通过");
    assert_eq!(notifications[1].body, format!("动作 {} 状态: approved", action_id));
    println!("ai approval notification payload: {}", result);
}

#[tokio::test]
async fn execute_tool_contract_matches_python_payload_shape() {
    let service = AiRuntimeService::new();

    let pending = service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "update_todo".to_string(),
                category: "todo".to_string(),
                operation_level: "l1_write".to_string(),
                side_effect: true,
                query_intent: None,
                query_dataset: None,
            },
            json!({"todo_id": "todo_1"}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;

    assert_eq!(pending["status"], "pending_approval");
    assert_eq!(pending["code"], "TOOL_PENDING_APPROVAL");
    assert_eq!(pending["severity"], "warning");
    assert_eq!(pending["data"]["action_id"], pending["approval_id"]);
    assert_eq!(pending["meta"]["duration_ms"], 0);
    assert!(pending["execution_id"].as_str().is_some());
    println!("ai execute pending payload: {}", pending);

    let success = service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "list_flights".to_string(),
                category: "query".to_string(),
                operation_level: "l0_read".to_string(),
                side_effect: false,
                query_intent: None,
                query_dataset: None,
            },
            json!({"limit": 10}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;

    assert_eq!(success["status"], "success");
    assert_eq!(success["code"], "TOOL_SUCCESS");
    assert_eq!(success["severity"], "success");
    assert_eq!(success["meta"]["duration_ms"], 28);
    assert!(success["data"].is_object());
    println!("ai execute success payload: {}", success);
}

#[tokio::test]
async fn query_routing_metrics_aggregate_live_state() {
    let service = AiRuntimeService::new();

    service
        .record_query_route("search", "flights", "get_flight_overview", "success", false, "none")
        .await;
    service
        .record_query_route(
            "aggregate",
            "flights",
            "get_flight_status_summary",
            "success",
            false,
            "none",
        )
        .await;
    service
        .record_query_route(
            "search",
            "flights",
            "none",
            "validation_error",
            true,
            "unsupported_intent",
        )
        .await;
    service
        .record_query_tool_selection("success", false, "get_flight_overview", "none")
        .await;
    service
        .record_query_tool_selection(
            "validation_error",
            true,
            "get_flight_status_summary",
            "missing_runtime_metadata",
        )
        .await;

    let payload = service.query_routing_metrics().await;

    assert_eq!(payload["query_route_total"], 3);
    assert_eq!(payload["query_misroute_total"], 1);
    assert_eq!(payload["query_selection_total"], 2);
    assert_eq!(payload["query_misselection_total"], 1);
    assert_eq!(payload["top_reasons"][0]["reason"], "none");
    assert_eq!(payload["top_reasons"][0]["count"], 2);
    println!("ai query routing metrics payload: {}", payload);
}

#[tokio::test]
async fn execute_tool_records_query_route_metrics_from_execution_labels() {
    let service = AiRuntimeService::new();

    service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "get_flight_overview".to_string(),
                category: "query".to_string(),
                operation_level: "l0_read".to_string(),
                side_effect: false,
                query_intent: Some("search".to_string()),
                query_dataset: Some("flights".to_string()),
            },
            json!({"limit": 10}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;

    let state = service.state.read().await;
    let (labels, count) = state
        .query_route_totals
        .iter()
        .next()
        .expect("query route metric recorded");

    assert_eq!(*count, 1);
    assert_eq!(labels.intent, "search");
    assert_eq!(labels.dataset, "flights");
    assert_eq!(labels.adapter, "get_flight_overview");
    assert_eq!(labels.status, "success");
    assert_eq!(labels.misroute, "false");
    assert_eq!(labels.reason, "none");
}

#[tokio::test]
async fn execute_tool_records_query_route_metric_for_unsupported_dataset() {
    let service = AiRuntimeService::new();

    service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "get_flight_overview".to_string(),
                category: "query".to_string(),
                operation_level: "l0_read".to_string(),
                side_effect: false,
                query_intent: Some("search".to_string()),
                query_dataset: Some("crews".to_string()),
            },
            json!({"limit": 10}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;

    let state = service.state.read().await;
    let (labels, count) = state
        .query_route_totals
        .iter()
        .next()
        .expect("unsupported dataset metric recorded");
    println!("unsupported dataset labels: {:?}", labels);

    assert_eq!(*count, 1);
    assert_eq!(labels.intent, "search");
    assert_eq!(labels.dataset, "crews");
    assert_eq!(labels.adapter, "none");
    assert_eq!(labels.status, "validation_error");
    assert_eq!(labels.misroute, "true");
    assert_eq!(labels.reason, "unsupported_dataset");
}

#[tokio::test]
async fn prune_scheduling_does_not_wait_for_state_write_lock() {
    let service = AiRuntimeService::new();
    let state_guard = service.state.write().await;

    service.schedule_prune();

    assert!(service.is_prune_scheduled());
    drop(state_guard);
    wait_for_prune_to_finish(&service).await;
    assert!(!service.is_prune_scheduled());
}

#[tokio::test]
async fn execute_tool_records_query_route_metric_for_unsupported_intent() {
    let service = AiRuntimeService::new();

    service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "get_flight_overview".to_string(),
                category: "query".to_string(),
                operation_level: "l0_read".to_string(),
                side_effect: false,
                query_intent: Some("summarize".to_string()),
                query_dataset: Some("flights".to_string()),
            },
            json!({"limit": 10}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;

    let state = service.state.read().await;
    let (labels, count) = state
        .query_route_totals
        .iter()
        .next()
        .expect("unsupported intent metric recorded");
    println!("unsupported intent labels: {:?}", labels);

    assert_eq!(*count, 1);
    assert_eq!(labels.intent, "summarize");
    assert_eq!(labels.dataset, "flights");
    assert_eq!(labels.adapter, "none");
    assert_eq!(labels.status, "validation_error");
    assert_eq!(labels.misroute, "true");
    assert_eq!(labels.reason, "unsupported_intent");
}

#[tokio::test]
async fn execute_tool_records_query_route_metric_for_dataset_specific_routing_failure() {
    let service = AiRuntimeService::new();

    service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "search_todos".to_string(),
                category: "query".to_string(),
                operation_level: "l0_read".to_string(),
                side_effect: false,
                query_intent: Some("aggregate".to_string()),
                query_dataset: Some("alerts".to_string()),
            },
            json!({"limit": 10}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;

    let state = service.state.read().await;
    let (labels, count) = state
        .query_route_totals
        .iter()
        .next()
        .expect("dataset-specific routing failure metric recorded");
    println!("dataset-specific routing failure labels: {:?}", labels);

    assert_eq!(*count, 1);
    assert_eq!(labels.intent, "aggregate");
    assert_eq!(labels.dataset, "alerts");
    assert_eq!(labels.adapter, "search_todos");
    assert_eq!(labels.status, "validation_error");
    assert_eq!(labels.misroute, "true");
    assert_eq!(labels.reason, "dataset_specific_routing_failure");
}

#[tokio::test]
async fn report_schema_metrics_aggregate_live_state() {
    let service = AiRuntimeService::new();

    service
        .record_report_schema_validation(true, "legacy", "flight_history", 0)
        .await;
    service
        .record_report_schema_validation(false, "legacy", "flight_event_journey", 3)
        .await;
    service
        .record_report_schema_validation(false, "jsonschema", "ops_incident", 1)
        .await;

    let payload = service.report_schema_metrics().await;

    assert_eq!(payload["schema_validation_total"], 3);
    assert_eq!(payload["schema_validation_invalid_total"], 2);
    assert_eq!(payload["mode_breakdown"][0]["mode"], "legacy");
    assert_eq!(payload["mode_breakdown"][0]["count"], 2);
    println!("ai report schema metrics payload: {}", payload);
}

#[tokio::test]
async fn execution_visibility_metrics_match_python_shape_and_semantics() {
    let service = AiRuntimeService::new();

    {
        let mut state = service.state.write().await;
        state.record_visibility_sample(1500.0, 2800.0);
        state.record_visibility_sample(1200.0, 3200.0);
    }

    let payload = service.execution_visibility_metrics().await;

    assert_eq!(payload["execution_event_total"], 2);
    assert_eq!(payload["coverage"]["first_progress_samples"], 2);
    assert_eq!(payload["coverage"]["event_interval_samples"], 2);

    assert_eq!(payload["first_progress_latency_ms"]["count"], 2);
    assert_eq!(payload["first_progress_latency_ms"]["violation_total"], 0);
    assert_eq!(payload["first_progress_latency_ms"]["target_p95_lt_ms"], 1500.0);
    assert!(payload["first_progress_latency_ms"].get("target_lte_ms").is_none());
    assert_eq!(payload["first_progress_latency_ms"]["target_met"], false);

    assert_eq!(payload["event_interval_ms"]["count"], 2);
    assert_eq!(payload["event_interval_ms"]["violation_total"], 1);
    assert_eq!(payload["event_interval_ms"]["target_lte_ms"], 3000.0);
    assert!(payload["event_interval_ms"].get("target_p95_lt_ms").is_none());
    assert_eq!(payload["event_interval_ms"]["target_met"], false);
    println!("ai execution visibility metrics payload: {}", payload);
}

#[tokio::test]
async fn todo_graph_pilot_metrics_returns_python_shape_with_live_counts() {
    let now = Utc::now();
    let todo_repo = Arc::new(FakeTodoRepo {
        items: vec![test_todo("todo_1", now), test_todo("todo_2", now)],
    });
    let context_repo = Arc::new(FakeTodoAgentContextRepo {
        items: HashMap::from([
            (
                "todo_1".to_string(),
                TodoAgentContext {
                    todo_id: "todo_1".to_string(),
                    agent_entity_id: "todo_graph_pilot".to_string(),
                    agent_run_id: Some("run_001".to_string()),
                    agent_status: "completed".to_string(),
                    updated_by: "tester".to_string(),
                    updated_at: Some(now),
                    version: 1,
                },
            ),
            (
                "todo_2".to_string(),
                TodoAgentContext {
                    todo_id: "todo_2".to_string(),
                    agent_entity_id: "todo_graph_pilot".to_string(),
                    agent_run_id: Some("run_002".to_string()),
                    agent_status: "pending".to_string(),
                    updated_by: "tester".to_string(),
                    updated_at: Some(now),
                    version: 1,
                },
            ),
        ]),
    });
    let service = AiRuntimeService::new()
        .with_todo_repository(todo_repo)
        .with_todo_agent_context_repository(context_repo);

    {
        let mut state = service.state.write().await;
        let mut run_001 = ExecutionRecord::success(
            "run_001".to_string(),
            "todo_agent".to_string(),
            json!({
                "metadata": {
                    "graph_runtime_guardrails": {
                        "duplicate_tool_execution_total": 0,
                        "duplicate_tool_execution_blocked_total": 0
                    }
                },
                "execution": { "duration_ms": 300000 },
            }),
            Some("user_001".to_string()),
            vec!["dispatcher".to_string()],
            now - chrono::Duration::minutes(20),
        );
        run_001.todo_id = Some("todo_1".to_string());
        run_001.entity_id = Some("todo_graph_pilot".to_string());
        run_001.runtime_path = "graph".to_string();
        run_001.runtime_path_requested = "graph".to_string();
        run_001.finished_at = Some(now - chrono::Duration::minutes(15));
        run_001.updated_at = now - chrono::Duration::minutes(15);

        let mut run_002 = ExecutionRecord::success(
            "run_002".to_string(),
            "todo_agent".to_string(),
            json!({
                "metadata": {
                    "graph_runtime_guardrails": {
                        "duplicate_tool_execution_total": 0,
                        "duplicate_tool_execution_blocked_total": 0
                    }
                },
                "execution": { "duration_ms": 600000 },
            }),
            Some("user_001".to_string()),
            vec!["dispatcher".to_string()],
            now - chrono::Duration::minutes(12),
        );
        run_002.todo_id = Some("todo_2".to_string());
        run_002.entity_id = Some("todo_graph_pilot".to_string());
        run_002.runtime_path = "legacy".to_string();
        run_002.runtime_path_requested = "graph".to_string();
        run_002.runtime_fallback_reason = Some("guardrail_timeout".to_string());
        run_002.finished_at = Some(now - chrono::Duration::minutes(2));
        run_002.updated_at = now - chrono::Duration::minutes(2);

        state.execution_order = vec!["run_001".to_string(), "run_002".to_string()];
        state.executions.insert("run_001".to_string(), run_001);
        state.executions.insert("run_002".to_string(), run_002);

        let mut executed_action = PendingActionRecord::new(
            "pending_001".to_string(),
            AiToolExecutionSpec {
                tool_name: "change_stand".to_string(),
                category: "todo".to_string(),
                operation_level: "l1_write".to_string(),
                side_effect: true,
                query_intent: None,
                query_dataset: None,
            },
            json!({"todo_id": "todo_1", "entity_id": "todo_graph_pilot"}),
            "call_001".to_string(),
            Some("user_001".to_string()),
            vec!["dispatcher".to_string()],
            "run_001".to_string(),
            now - chrono::Duration::minutes(18),
        );
        executed_action.status = "executed".to_string();
        executed_action.updated_at = now - chrono::Duration::minutes(12);
        executed_action.approved_at = Some(now - chrono::Duration::minutes(12));
        executed_action.execution_receipt = Some(json!({
            "resume_mode": "graph",
            "status": "applied"
        }));

        let stale_action = PendingActionRecord::new(
            "pending_002".to_string(),
            AiToolExecutionSpec {
                tool_name: "change_stand".to_string(),
                category: "todo".to_string(),
                operation_level: "l1_write".to_string(),
                side_effect: true,
                query_intent: None,
                query_dataset: None,
            },
            json!({"todo_id": "todo_2", "entity_id": "todo_graph_pilot"}),
            "call_002".to_string(),
            Some("user_001".to_string()),
            vec!["dispatcher".to_string()],
            "run_002".to_string(),
            now - chrono::Duration::minutes(90),
        );
        let mut stale_action = stale_action;
        stale_action.expires_at = Some(now + chrono::Duration::minutes(90));
        stale_action.updated_at = now - chrono::Duration::minutes(45);

        state.pending_order = vec!["pending_001".to_string(), "pending_002".to_string()];
        state.pending_actions.insert("pending_001".to_string(), executed_action);
        state.pending_actions.insert("pending_002".to_string(), stale_action);
    }

    let payload = service
        .todo_graph_pilot_metrics(Some("todo_graph_pilot".to_string()), 168, 200, 30)
        .await;

    assert_eq!(payload["scope"]["entity_id"], "todo_graph_pilot");
    assert_eq!(payload["scope"]["cohort_mode"], "entity");
    assert_eq!(payload["thresholds"]["ready_graph_requested_total_min"], 30);
    assert_eq!(payload["executions"]["total"], 2);
    assert_eq!(payload["executions"]["graph_requested_total"], 2);
    assert_eq!(payload["executions"]["graph_actual_total"], 1);
    assert_eq!(payload["executions"]["graph_fallback_total"], 1);
    assert_eq!(
        payload["executions"]["top_fallback_reasons"][0]["reason"],
        "guardrail_timeout"
    );
    assert_eq!(payload["approvals"]["pending_total"], 1);
    assert_eq!(payload["approvals"]["stale_pending_total"], 1);
    assert_eq!(payload["approvals"]["graph_resume_total"], 1);
    assert_eq!(payload["approvals"]["graph_resume_success_total"], 1);
    assert_eq!(payload["value_metrics"]["execution_duration_ms"]["p50"], 450000.0);
    assert_eq!(payload["value_metrics"]["execution_duration_ms"]["p95"], 585000.0);
    assert_eq!(payload["value_metrics"]["approval_response_time_ms"]["sample_size"], 2);
    assert_eq!(payload["value_metrics"]["approval_response_time_ms"]["p50"], 1530000.0);
    assert_eq!(payload["value_metrics"]["human_approval_rate"], 0.5);
    assert_eq!(payload["verdict"]["status"], "insufficient_data");
    assert!(payload["verdict"]["reasons"]
        .as_array()
        .expect("reasons array")
        .iter()
        .any(|item| item == "graph requested sample size below readiness threshold"));
}

#[tokio::test]
async fn todo_graph_pilot_metrics_returns_hold_when_duplicate_attempts_blocked() {
    let now = Utc::now();
    let todo_repo = Arc::new(FakeTodoRepo {
        items: (0..35)
            .map(|index| test_todo(&format!("todo_{index:03}"), now))
            .collect(),
    });
    let context_repo = Arc::new(FakeTodoAgentContextRepo {
        items: (0..35)
            .map(|index| {
                (
                    format!("todo_{index:03}"),
                    TodoAgentContext {
                        todo_id: format!("todo_{index:03}"),
                        agent_entity_id: "todo_graph_pilot".to_string(),
                        agent_run_id: Some(format!("run_{index:03}")),
                        agent_status: "completed".to_string(),
                        updated_by: "tester".to_string(),
                        updated_at: Some(now),
                        version: 1,
                    },
                )
            })
            .collect(),
    });
    let service = AiRuntimeService::new()
        .with_todo_repository(todo_repo)
        .with_todo_agent_context_repository(context_repo);

    {
        let mut state = service.state.write().await;
        for index in 0..35 {
            let run_id = format!("run_{index:03}");
            let mut execution = ExecutionRecord::success(
                run_id.clone(),
                "todo_agent".to_string(),
                json!({
                    "metadata": {
                        "graph_runtime_guardrails": {
                            "duplicate_tool_execution_total": 0,
                            "duplicate_tool_execution_blocked_total": if index == 0 { 1 } else { 0 }
                        }
                    },
                    "execution": { "duration_ms": 60000 }
                }),
                None,
                vec![],
                now - chrono::Duration::minutes(60 - index as i64),
            );
            execution.todo_id = Some(format!("todo_{index:03}"));
            execution.entity_id = Some("todo_graph_pilot".to_string());
            execution.runtime_path = "graph".to_string();
            execution.runtime_path_requested = "graph".to_string();
            execution.finished_at = Some(now - chrono::Duration::minutes(59 - index as i64));
            execution.updated_at = execution.finished_at.expect("finished_at");
            state.execution_order.push(run_id.clone());
            state.executions.insert(run_id, execution);
        }

        for index in 0..5 {
            let mut action = PendingActionRecord::new(
                format!("pending_{index:03}"),
                AiToolExecutionSpec {
                    tool_name: "change_stand".to_string(),
                    category: "todo".to_string(),
                    operation_level: "l1_write".to_string(),
                    side_effect: true,
                    query_intent: None,
                    query_dataset: None,
                },
                json!({"todo_id": format!("todo_{index:03}"), "entity_id": "todo_graph_pilot"}),
                format!("call_{index:03}"),
                Some("user_001".to_string()),
                vec!["dispatcher".to_string()],
                format!("run_{index:03}"),
                now - chrono::Duration::minutes(30 - index as i64),
            );
            action.status = "executed".to_string();
            action.updated_at = now - chrono::Duration::minutes(25 - index as i64);
            action.approved_at = Some(now - chrono::Duration::minutes(25 - index as i64));
            action.execution_receipt = Some(json!({
                "resume_mode": "graph",
                "status": "applied"
            }));
            state.pending_order.push(format!("pending_{index:03}"));
            state.pending_actions.insert(format!("pending_{index:03}"), action);
        }
    }

    let payload = service
        .todo_graph_pilot_metrics(Some("todo_graph_pilot".to_string()), 168, 200, 30)
        .await;

    assert_eq!(payload["guardrails"]["duplicate_tool_execution_blocked_total"], 1);
    assert_eq!(payload["verdict"]["status"], "hold");
    assert!(payload["verdict"]["reasons"]
        .as_array()
        .expect("reasons array")
        .iter()
        .any(|item| item == "duplicate execution attempts were blocked"));
}

#[tokio::test]
async fn reject_pending_action_notifies_requester_with_reason() {
    let repo = Arc::new(FakeNotificationRepo::default());
    let notification_service = Arc::new(notification_service_without_side_channels(
        repo.clone(),
        Arc::new(FakePreferenceRepo),
    ));
    let service = AiRuntimeService::new().with_notification_service(notification_service);

    let pending = service
        .execute_tool(
            AiToolExecutionSpec {
                tool_name: "update_todo".to_string(),
                category: "todo".to_string(),
                operation_level: "l1_write".to_string(),
                side_effect: true,
                query_intent: None,
                query_dataset: None,
            },
            json!({"todo_id": "todo_1"}),
            Some("requester_001".to_string()),
            vec!["dispatcher".to_string()],
        )
        .await;
    let action_id = pending["approval_id"].as_str().expect("approval id").to_string();

    service
        .reject_pending_action(&action_id, "approver_001", Some("manual review failed"))
        .await
        .expect("reject pending action");

    let notifications = repo.items.lock().expect("lock notifications");
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[0].title, "AI 工具 'update_todo' 已进入审批队列");
    assert_eq!(
        notifications[0].body,
        format!("动作 {} 状态: pending；原因: todo requires human approval", action_id)
    );
    assert_eq!(notifications[1].title, "AI 工具 'update_todo' 审批已被拒绝");
    assert_eq!(
        notifications[1].body,
        format!("动作 {} 状态: rejected；原因: todo requires human approval", action_id)
    );
    println!("ai rejection notification title: {}", notifications[1].title);
    println!("ai rejection notification body: {}", notifications[1].body);
}
