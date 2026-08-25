use super::*;
use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch_collaboration::{
    DispatchChatDispatcherCandidate, DispatchChatGroupList, DispatchChatGroupSummary, DispatchChatMember,
    DispatchChatMemberUnread, DispatchChatMemberUpsert, DispatchChatMessage, DispatchChatMessageCursor,
    DispatchChatMessageList, DispatchChatReadCursorUpdate, DispatchChatUserProfile, DispatchCollaborationEvent,
    NewDispatchChatMessage, NotificationReceiptSummary,
};
use fms_domain::models::notification::{Notification, NotificationPreference};
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};

use crate::test_support::notification_service_without_side_channels;
use crate::types::{
    NoopNotificationDeliveryPublisher, NoopNotificationMetricsRecorder, NoopNotificationReceiptGroupSync,
};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

struct FakeNotificationRepo {
    saved: Mutex<Vec<Notification>>,
    ack_result: Mutex<Option<Notification>>,
    summary: Mutex<Option<Value>>,
    sent_groups: Mutex<Vec<Value>>,
}

impl FakeNotificationRepo {
    fn new() -> Self {
        Self {
            saved: Mutex::new(Vec::new()),
            ack_result: Mutex::new(None),
            summary: Mutex::new(None),
            sent_groups: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl NotificationRepository for FakeNotificationRepo {
    async fn save(&self, notification: &Notification) -> Result<(), DomainError> {
        let mut saved = self.saved.lock().expect("lock saved");
        if let Some(existing) = saved
            .iter_mut()
            .find(|item| item.notification_id == notification.notification_id)
        {
            *existing = notification.clone();
        } else {
            saved.push(notification.clone());
        }
        Ok(())
    }

    async fn find_by_id(&self, notification_id: &str) -> Result<Option<Notification>, DomainError> {
        Ok(self
            .saved
            .lock()
            .expect("lock saved")
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
            .saved
            .lock()
            .expect("lock saved")
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
            .saved
            .lock()
            .expect("lock saved")
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
            .saved
            .lock()
            .expect("lock saved")
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
        Ok(self.ack_result.lock().expect("lock ack").clone())
    }

    async fn find_by_receipt_group(&self, receipt_group_id: &str) -> Result<Vec<Notification>, DomainError> {
        Ok(self
            .saved
            .lock()
            .expect("lock saved")
            .iter()
            .filter(|item| item.receipt_group_id.as_deref() == Some(receipt_group_id))
            .cloned()
            .collect())
    }

    async fn summarize_receipt_group(&self, _receipt_group_id: &str) -> Result<Option<Value>, DomainError> {
        Ok(self.summary.lock().expect("lock summary").clone())
    }

    async fn list_sent_receipt_groups(
        &self,
        _sender_user_id: &str,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Value>, DomainError> {
        Ok(self.sent_groups.lock().expect("lock sent groups").clone())
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
struct FakeDeliveryPublisher {
    sender_updates: Mutex<Vec<(String, Value)>>,
}

impl NotificationDeliveryPublisher for FakeDeliveryPublisher {
    fn publish_user_notification<'a>(
        &'a self,
        _notification: &'a NotificationResponse,
        _unread_count: i64,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }

    fn publish_sender_receipt_update<'a>(
        &'a self,
        sender_user_id: &'a str,
        payload: Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async move {
            self.sender_updates
                .lock()
                .expect("lock sender updates")
                .push((sender_user_id.to_string(), payload));
            Ok(1)
        })
    }
}

#[derive(Default)]
struct FakeCollaborationRepo {
    events: Mutex<Vec<DispatchCollaborationEvent>>,
}

#[async_trait::async_trait]
impl DispatchCollaborationRepository for FakeCollaborationRepo {
    async fn get_group_by_id(&self, _group_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        Ok(None)
    }

    async fn get_group_for_user(
        &self,
        _group_id: &str,
        _user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        Ok(None)
    }

    async fn get_group_for_user_by_flight(
        &self,
        _flight_id: &str,
        _user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        Ok(None)
    }

    async fn get_group_by_flight(&self, _flight_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        Ok(None)
    }

    async fn list_user_groups(
        &self,
        _user_id: &str,
        _status: &str,
        _limit: i64,
        _offset: i64,
    ) -> Result<DispatchChatGroupList, DomainError> {
        Ok(DispatchChatGroupList {
            items: vec![],
            total: 0,
            limit: 0,
            offset: 0,
            unread_total: 0,
        })
    }

    async fn list_group_messages(
        &self,
        _group_id: &str,
        _limit: i64,
        _cursor: DispatchChatMessageCursor,
    ) -> Result<DispatchChatMessageList, DomainError> {
        Ok(DispatchChatMessageList {
            items: vec![],
            total: 0,
            limit: 0,
            before_seq: None,
            after_seq: None,
            has_more: false,
            next_before_seq: None,
            next_after_seq: None,
        })
    }

    async fn insert_message(&self, _message: &NewDispatchChatMessage) -> Result<DispatchChatMessage, DomainError> {
        Err(DomainError::ValidationError("unused in test".into()))
    }

    async fn update_message_event_id(
        &self,
        _message_id: &str,
        _event_id: &str,
    ) -> Result<Option<DispatchChatMessage>, DomainError> {
        Ok(None)
    }

    async fn find_message_by_client_id(
        &self,
        _group_id: &str,
        _client_msg_id: &str,
    ) -> Result<Option<DispatchChatMessage>, DomainError> {
        Ok(None)
    }

    async fn mark_group_read(
        &self,
        _group_id: &str,
        _user_id: &str,
        _read_seq: i64,
    ) -> Result<Option<DispatchChatReadCursorUpdate>, DomainError> {
        Ok(None)
    }

    async fn get_group_latest_seq(&self, _group_id: &str) -> Result<i64, DomainError> {
        Ok(0)
    }

    async fn count_group_unread(&self, _group_id: &str, _user_id: &str) -> Result<i64, DomainError> {
        Ok(0)
    }

    async fn count_total_unread(&self, _user_id: &str) -> Result<i64, DomainError> {
        Ok(0)
    }

    async fn count_unread_for_group_members(
        &self,
        _group_id: &str,
    ) -> Result<Vec<DispatchChatMemberUnread>, DomainError> {
        Ok(vec![])
    }

    async fn find_active_members(&self, _group_id: &str) -> Result<Vec<DispatchChatMember>, DomainError> {
        Ok(vec![])
    }

    async fn find_group_members(&self, _group_id: &str) -> Result<Vec<DispatchChatMember>, DomainError> {
        Ok(vec![])
    }

    async fn find_users_by_ids(&self, _user_ids: &[String]) -> Result<Vec<DispatchChatUserProfile>, DomainError> {
        Ok(vec![])
    }

    async fn find_dispatchers_by_departments(
        &self,
        _departments: &[String],
    ) -> Result<Vec<DispatchChatDispatcherCandidate>, DomainError> {
        Ok(vec![])
    }

    async fn upsert_group_for_flight(
        &self,
        _flight_id: &str,
        _group_name: &str,
        _archive_at: Option<chrono::DateTime<chrono::Utc>>,
        _metadata: &serde_json::Value,
    ) -> Result<DispatchChatGroupSummary, DomainError> {
        Err(DomainError::ValidationError("unused in test".into()))
    }

    async fn upsert_group_memberships(
        &self,
        _group_id: &str,
        _memberships: &[DispatchChatMemberUpsert],
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn deactivate_members_except(
        &self,
        _group_id: &str,
        _active_user_ids: &[String],
    ) -> Result<Vec<DispatchChatMember>, DomainError> {
        Ok(vec![])
    }

    async fn clear_group_deprecation(
        &self,
        _group_id: &str,
        _reason: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        Ok(None)
    }

    async fn mark_group_deprecated(
        &self,
        _group_id: &str,
        _reason: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        Ok(None)
    }

    async fn find_groups_pending_deprecation(&self, _limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
        Ok(vec![])
    }

    async fn find_due_archive_groups(&self, _limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
        Ok(vec![])
    }

    async fn archive_groups_batch(&self, _group_ids: &[String]) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
        Ok(vec![])
    }

    async fn create_event(
        &self,
        event: &DispatchCollaborationEvent,
    ) -> Result<DispatchCollaborationEvent, DomainError> {
        self.events.lock().expect("lock events").push(event.clone());
        Ok(event.clone())
    }

    async fn list_events_by_flight(
        &self,
        _flight_id: &str,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchCollaborationEvent>, DomainError> {
        Ok(vec![])
    }

    async fn list_events_by_order(
        &self,
        _order_id: &str,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DispatchCollaborationEvent>, DomainError> {
        Ok(vec![])
    }

    async fn find_recent_notifications_by_flight(
        &self,
        _flight_id: &str,
        _limit: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        Ok(vec![])
    }

    async fn find_recent_notifications_by_order(
        &self,
        _order_id: &str,
        _limit: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        Ok(vec![])
    }

    async fn summarize_receipts_for_flight(&self, _flight_id: &str) -> Result<NotificationReceiptSummary, DomainError> {
        Ok(NotificationReceiptSummary {
            total_count: 0,
            pending_count: 0,
            acknowledged_count: 0,
            rejected_count: 0,
            latest_updated_at: None,
            receipt_group_ids: vec![],
        })
    }

    async fn summarize_receipts_for_order(&self, _order_id: &str) -> Result<NotificationReceiptSummary, DomainError> {
        Ok(NotificationReceiptSummary {
            total_count: 0,
            pending_count: 0,
            acknowledged_count: 0,
            rejected_count: 0,
            latest_updated_at: None,
            receipt_group_ids: vec![],
        })
    }
}

#[tokio::test]
async fn send_batch_preserves_non_critical_receipts_and_sender_metadata() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let service = notification_service_without_side_channels(repo.clone(), Arc::new(FakePreferenceRepo));

    let result = service
        .send_batch(DispatchBatchNotificationCreate {
            user_ids: vec!["user_001".to_string(), "user_002".to_string()],
            title: "普通提醒".to_string(),
            body: "需要回执".to_string(),
            category: "dispatch".to_string(),
            severity: "warning".to_string(),
            flight_id: None,
            related_entity_type: None,
            related_entity_id: None,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: Some("sender_001".to_string()),
            sender_username_snapshot: Some("发送者甲".to_string()),
            origin_type: "manual".to_string(),
            receipt_required: true,
        })
        .await
        .expect("send batch succeeds");

    let receipt_group_id = result
        .get("receipt_group_id")
        .and_then(Value::as_str)
        .expect("receipt group id present")
        .to_string();
    let saved = repo.saved.lock().expect("lock saved").clone();
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(|item| item.receipt_required));
    assert!(saved
        .iter()
        .all(|item| item.receipt_group_id.as_deref() == Some(receipt_group_id.as_str())));
    assert!(saved
        .iter()
        .all(|item| item.sender_user_id.as_deref() == Some("sender_001")));
    assert!(saved
        .iter()
        .all(|item| { item.sender_username_snapshot.as_deref() == Some("发送者甲") }));
}

#[tokio::test]
async fn send_batch_with_idempotency_reuses_receipt_group_and_notification_ids() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let service = notification_service_without_side_channels(repo.clone(), Arc::new(FakePreferenceRepo));
    let dto = DispatchBatchNotificationCreate {
        user_ids: vec![" user_001 ".to_string(), "user_002".to_string(), "user_001".to_string()],
        title: "流程提醒".to_string(),
        body: "需要回执".to_string(),
        category: "dispatch".to_string(),
        severity: "warning".to_string(),
        flight_id: None,
        related_entity_type: None,
        related_entity_id: None,
        dispatch_order_id: Some("order_001".to_string()),
        group_id: None,
        sender_user_id: Some("sender_001".to_string()),
        sender_username_snapshot: Some("发送者甲".to_string()),
        origin_type: "workflow".to_string(),
        receipt_required: true,
    };

    let first = service
        .send_batch_with_idempotency(
            dto.clone(),
            Some("receipt_group_stable_001".to_string()),
            Some("notification_seed_001".to_string()),
        )
        .await
        .expect("first send succeeds");
    let second = service
        .send_batch_with_idempotency(
            dto,
            Some("receipt_group_stable_001".to_string()),
            Some("notification_seed_001".to_string()),
        )
        .await
        .expect("second send succeeds");

    assert_eq!(first["receipt_group_id"], "receipt_group_stable_001");
    assert_eq!(second["receipt_group_id"], "receipt_group_stable_001");
    assert_eq!(
        first["items"][0]["notification_id"],
        second["items"][0]["notification_id"]
    );
    assert_eq!(
        first["items"][1]["notification_id"],
        second["items"][1]["notification_id"]
    );
    let saved = repo.saved.lock().expect("lock saved").clone();
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(|item| item.notification_id.len() == 26));
    assert!(saved.iter().all(|item| item.notification_id.is_ascii()));
    assert!(saved
        .iter()
        .all(|item| { item.receipt_group_id.as_deref() == Some("receipt_group_stable_001") }));
}

#[tokio::test]
async fn acknowledge_publishes_sender_receipt_update() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let notification = Notification {
        notification_id: "notification_001".to_string(),
        user_id: "user_001".to_string(),
        title: "title".to_string(),
        body: "body".to_string(),
        category: "dispatch".to_string(),
        severity: "warning".to_string(),
        is_read: false,
        flight_id: Some("flight_001".to_string()),
        related_entity_type: None,
        related_entity_id: None,
        dispatch_order_id: None,
        group_id: None,
        event_id: None,
        sender_user_id: Some("sender_001".to_string()),
        sender_username_snapshot: Some("发送者甲".to_string()),
        recipient_username_snapshot: Some("zhangsan".to_string()),
        recipient_display_name_snapshot: Some("张三".to_string()),
        recipient_department_snapshot: Some("运行控制".to_string()),
        recipient_job_title_snapshot: Some("签派员".to_string()),
        origin_type: "workflow".to_string(),
        receipt_required: true,
        receipt_group_id: Some("receipt_group_001".to_string()),
        delivery_status: "delivered".to_string(),
        delivered_at: None,
        ack_status: "pending".to_string(),
        ack_at: None,
        ack_note: None,
        created_at: Utc::now() - chrono::Duration::minutes(5),
        read_at: None,
    };
    repo.saved.lock().expect("lock saved").push(notification.clone());
    *repo.ack_result.lock().expect("lock ack") = Some(Notification {
        ack_status: "acknowledged".to_string(),
        ack_at: Some(Utc::now()),
        is_read: true,
        ..notification.clone()
    });
    *repo.summary.lock().expect("lock summary") = Some(json!({
        "title": "title",
        "severity": "warning",
        "flight_id": "flight_001",
        "created_at": notification.created_at,
        "origin_type": "workflow",
        "total_count": 2,
        "pending_count": 1,
        "acknowledged_count": 1,
        "rejected_count": 0,
        "latest_updated_at": Utc::now(),
    }));
    let delivery = Arc::new(FakeDeliveryPublisher::default());
    let service = NotificationService::new(
        repo,
        Arc::new(FakePreferenceRepo),
        Arc::new(NoCollaborationEvents),
        delivery.clone(),
        Arc::new(NoopNotificationMetricsRecorder),
        Arc::new(NoopNotificationReceiptGroupSync),
    );

    let updated = service
        .acknowledge("notification_001", "user_001", "acknowledged", None, None)
        .await
        .expect("ack succeeds")
        .expect("updated notification present");

    assert_eq!(updated.ack_status, "acknowledged");
    let sender_updates = delivery.sender_updates.lock().expect("lock sender updates");
    assert_eq!(sender_updates.len(), 1);
    assert_eq!(sender_updates[0].0, "sender_001");
    assert_eq!(sender_updates[0].1["type"], "sender_receipt_update");
    assert_eq!(sender_updates[0].1["receipt_group_id"], "receipt_group_001");
    assert_eq!(sender_updates[0].1["recipient_user_id"], "user_001");
    assert_eq!(sender_updates[0].1["recipient_username"], "zhangsan");
    assert_eq!(sender_updates[0].1["summary"]["pending_count"], 1);
}

#[tokio::test]
async fn get_receipt_group_exposes_sender_and_overdue_fields() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let created_at = Utc::now() - chrono::Duration::minutes(5);
    *repo.summary.lock().expect("lock summary") = Some(json!({
        "title": "Dispatch",
        "severity": "warning",
        "flight_id": "flight-1",
        "dispatch_order_id": "order-1",
        "group_id": null,
        "created_at": created_at,
        "origin_type": "workflow",
        "receipt_required": true,
        "sender_user_id": "sender-1",
        "sender_username": "sender-name",
        "total_count": 2,
        "pending_count": 1,
        "acknowledged_count": 1,
        "rejected_count": 0,
        "latest_updated_at": created_at,
    }));
    repo.saved.lock().expect("lock saved").push(Notification {
        notification_id: "n-1".to_string(),
        user_id: "user-1".to_string(),
        title: "Dispatch".to_string(),
        body: "body".to_string(),
        category: "dispatch".to_string(),
        severity: "warning".to_string(),
        is_read: false,
        flight_id: Some("flight-1".to_string()),
        related_entity_type: None,
        related_entity_id: None,
        dispatch_order_id: Some("order-1".to_string()),
        group_id: None,
        event_id: None,
        sender_user_id: Some("sender-1".to_string()),
        sender_username_snapshot: Some("sender-name".to_string()),
        recipient_username_snapshot: Some("zhangsan".to_string()),
        recipient_display_name_snapshot: Some("张三".to_string()),
        recipient_department_snapshot: Some("运行控制".to_string()),
        recipient_job_title_snapshot: Some("签派员".to_string()),
        origin_type: "workflow".to_string(),
        receipt_required: true,
        receipt_group_id: Some("rg-1".to_string()),
        delivery_status: "delivered".to_string(),
        delivered_at: None,
        ack_status: "pending".to_string(),
        ack_at: None,
        ack_note: None,
        created_at,
        read_at: None,
    });
    let service = notification_service_without_side_channels(repo, Arc::new(FakePreferenceRepo));

    let payload = service
        .get_receipt_group("rg-1")
        .await
        .expect("receipt group lookup succeeds")
        .expect("receipt group exists");

    assert_eq!(payload["severity"], "warning");
    assert_eq!(payload["sender_user_id"], "sender-1");
    assert_eq!(payload["sender_username"], "sender-name");
    assert_eq!(payload["items"][0]["recipient_username"], "zhangsan");
    assert_eq!(payload["items"][0]["recipient_display_name"], "张三");
    assert_eq!(payload["is_overdue"], true);
    assert!(payload["remind_after_at"].is_string());
}

#[tokio::test]
async fn send_notification_normalizes_non_workflow_origin_to_manual() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let service = notification_service_without_side_channels(repo.clone(), Arc::new(FakePreferenceRepo));

    let response = service
        .send_notification(NotificationCreate {
            user_id: "user_001".to_string(),
            title: "AI 审批".to_string(),
            body: "body".to_string(),
            category: Some("system".to_string()),
            severity: Some("info".to_string()),
            flight_id: None,
            related_entity_type: None,
            related_entity_id: None,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: None,
            sender_username_snapshot: None,
            origin_type: Some("ai".to_string()),
            receipt_required: false,
            receipt_group_id: None,
        })
        .await
        .expect("send notification succeeds");

    assert_eq!(response.origin_type, "manual");
    assert_eq!(response.origin_label, "人工");
    let saved = repo.saved.lock().expect("lock saved").clone();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].origin_type, "manual");
}

#[tokio::test]
async fn send_batch_preserves_workflow_origin_case_insensitively() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let service = notification_service_without_side_channels(repo.clone(), Arc::new(FakePreferenceRepo));

    let result = service
        .send_batch(DispatchBatchNotificationCreate {
            user_ids: vec!["user_001".to_string()],
            title: "流程提醒".to_string(),
            body: "body".to_string(),
            category: "dispatch".to_string(),
            severity: "warning".to_string(),
            flight_id: None,
            related_entity_type: None,
            related_entity_id: None,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: None,
            sender_username_snapshot: None,
            origin_type: "Workflow".to_string(),
            receipt_required: true,
        })
        .await
        .expect("send batch succeeds");

    assert_eq!(result["items"][0]["origin_type"], "workflow");
    let saved = repo.saved.lock().expect("lock saved").clone();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].origin_type, "workflow");
}

#[tokio::test]
async fn send_batch_returns_empty_payload_when_no_recipients() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let service = notification_service_without_side_channels(repo.clone(), Arc::new(FakePreferenceRepo));

    let result = service
        .send_batch(DispatchBatchNotificationCreate {
            user_ids: vec!["".to_string(), "   ".to_string()],
            title: "流程提醒".to_string(),
            body: "body".to_string(),
            category: "dispatch".to_string(),
            severity: "warning".to_string(),
            flight_id: None,
            related_entity_type: None,
            related_entity_id: None,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: None,
            sender_username_snapshot: None,
            origin_type: "workflow".to_string(),
            receipt_required: true,
        })
        .await
        .expect("send batch succeeds");

    assert!(result["receipt_group_id"].is_null());
    assert_eq!(result["items"].as_array().expect("items array").len(), 0);
    let saved = repo.saved.lock().expect("lock saved").clone();
    assert!(saved.is_empty());
}

#[tokio::test]
async fn acknowledge_allows_pending_non_receipt_notifications() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let notification = Notification {
        notification_id: "notification_non_receipt".to_string(),
        user_id: "user_001".to_string(),
        title: "title".to_string(),
        body: "body".to_string(),
        category: "dispatch".to_string(),
        severity: "warning".to_string(),
        is_read: false,
        flight_id: None,
        related_entity_type: None,
        related_entity_id: None,
        dispatch_order_id: None,
        group_id: None,
        event_id: None,
        sender_user_id: None,
        sender_username_snapshot: None,
        recipient_username_snapshot: None,
        recipient_display_name_snapshot: None,
        recipient_department_snapshot: None,
        recipient_job_title_snapshot: None,
        origin_type: "manual".to_string(),
        receipt_required: false,
        receipt_group_id: None,
        delivery_status: "sent".to_string(),
        delivered_at: None,
        ack_status: "pending".to_string(),
        ack_at: None,
        ack_note: None,
        created_at: Utc::now(),
        read_at: None,
    };
    repo.saved.lock().expect("lock saved").push(notification.clone());
    *repo.ack_result.lock().expect("lock ack") = Some(Notification {
        ack_status: "acknowledged".to_string(),
        ack_at: Some(Utc::now()),
        is_read: true,
        ..notification.clone()
    });
    let service = notification_service_without_side_channels(repo, Arc::new(FakePreferenceRepo));

    let updated = service
        .acknowledge("notification_non_receipt", "user_001", "acknowledged", None, None)
        .await
        .expect("ack succeeds")
        .expect("updated notification present");

    assert_eq!(updated.ack_status, "acknowledged");
    assert!(!updated.receipt_required);
}

#[tokio::test]
async fn get_receipt_group_normalizes_unknown_origin_type() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let created_at = Utc::now() - chrono::Duration::minutes(1);
    *repo.summary.lock().expect("lock summary") = Some(json!({
        "title": "AI 消息",
        "severity": "info",
        "created_at": created_at,
        "origin_type": "ai",
        "receipt_required": true,
        "total_count": 1,
        "pending_count": 1,
        "acknowledged_count": 0,
        "rejected_count": 0,
        "latest_updated_at": created_at,
    }));
    let service = notification_service_without_side_channels(repo, Arc::new(FakePreferenceRepo));

    let payload = service
        .get_receipt_group("rg-ai")
        .await
        .expect("receipt group lookup succeeds")
        .expect("receipt group exists");

    assert_eq!(payload["origin_type"], "manual");
    assert_eq!(payload["origin_label"], "人工");
}

#[tokio::test]
async fn created_collaboration_event_includes_sender_metadata() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let collaboration_repo = Arc::new(FakeCollaborationRepo::default());
    let service = NotificationService::new(
        repo,
        Arc::new(FakePreferenceRepo),
        Arc::new(CollaborationEventRecorder::new(collaboration_repo.clone())),
        Arc::new(NoopNotificationDeliveryPublisher),
        Arc::new(NoopNotificationMetricsRecorder),
        Arc::new(NoopNotificationReceiptGroupSync),
    );

    let _ = service
        .send_notification(NotificationCreate {
            user_id: "user_001".to_string(),
            title: "Dispatch".to_string(),
            body: "body".to_string(),
            category: Some("dispatch".to_string()),
            severity: Some("warning".to_string()),
            flight_id: Some("flight_001".to_string()),
            related_entity_type: None,
            related_entity_id: None,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: Some("sender_001".to_string()),
            sender_username_snapshot: Some("发送者甲".to_string()),
            origin_type: Some("manual".to_string()),
            receipt_required: false,
            receipt_group_id: None,
        })
        .await
        .expect("send notification succeeds");

    let events = collaboration_repo.events.lock().expect("lock events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "notification_created");
    assert_eq!(events[0].payload["sender_user_id"], "sender_001");
    assert_eq!(events[0].payload["sender_username"], "发送者甲");
}

#[tokio::test]
async fn receipt_required_collaboration_event_includes_sender_user_id() {
    let repo = Arc::new(FakeNotificationRepo::new());
    let collaboration_repo = Arc::new(FakeCollaborationRepo::default());
    let service = NotificationService::new(
        repo,
        Arc::new(FakePreferenceRepo),
        Arc::new(CollaborationEventRecorder::new(collaboration_repo.clone())),
        Arc::new(NoopNotificationDeliveryPublisher),
        Arc::new(NoopNotificationMetricsRecorder),
        Arc::new(NoopNotificationReceiptGroupSync),
    );

    let result = service
        .send_batch(DispatchBatchNotificationCreate {
            user_ids: vec!["user_001".to_string()],
            title: "Dispatch".to_string(),
            body: "需要回执".to_string(),
            category: "dispatch".to_string(),
            severity: "warning".to_string(),
            flight_id: Some("flight_001".to_string()),
            related_entity_type: None,
            related_entity_id: None,
            dispatch_order_id: None,
            group_id: None,
            sender_user_id: Some("sender_001".to_string()),
            sender_username_snapshot: Some("发送者甲".to_string()),
            origin_type: "manual".to_string(),
            receipt_required: true,
        })
        .await
        .expect("send batch succeeds");

    let receipt_group_id = result["receipt_group_id"]
        .as_str()
        .expect("receipt group id present")
        .to_string();
    let events = collaboration_repo.events.lock().expect("lock events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].event_type, "notification_receipt_required");
    assert_eq!(events[1].payload["receipt_group_id"], receipt_group_id);
    assert_eq!(events[1].payload["sender_user_id"], "sender_001");
}
