use super::*;
use chrono::Utc;
use fms_application::schemas::auth_schemas::TokenData;
use fms_domain::error::DomainError;
use fms_domain::models::notification::{Notification, NotificationPreference};
use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use tokio::sync::broadcast;

struct CountWake {
    pub(crate) count: AtomicUsize,
}

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

struct FakeNotificationRepo {
    pub(crate) item: Option<Notification>,
    pub(crate) ack_result: Option<Notification>,
    pub(crate) summary: Option<Value>,
}

#[async_trait::async_trait]
impl NotificationRepository for FakeNotificationRepo {
    async fn save(&self, _notification: &Notification) -> Result<(), DomainError> {
        Ok(())
    }

    async fn find_by_id(&self, _notification_id: &str) -> Result<Option<Notification>, DomainError> {
        Ok(self.item.clone())
    }

    async fn find_by_id_for_user(
        &self,
        _notification_id: &str,
        _user_id: &str,
    ) -> Result<Option<Notification>, DomainError> {
        Ok(self.item.clone())
    }

    async fn find_by_user(
        &self,
        _user_id: &str,
        _unread_only: bool,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        Ok(vec![])
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

    async fn count_unread(&self, _user_id: &str) -> Result<i64, DomainError> {
        Ok(0)
    }

    async fn acknowledge(
        &self,
        _notification_id: &str,
        _user_id: &str,
        _action: &str,
        _note: Option<&str>,
    ) -> Result<Option<Notification>, DomainError> {
        Ok(self.ack_result.clone())
    }

    async fn find_by_receipt_group(&self, _receipt_group_id: &str) -> Result<Vec<Notification>, DomainError> {
        Ok(self.item.clone().into_iter().collect())
    }

    async fn summarize_receipt_group(&self, _receipt_group_id: &str) -> Result<Option<Value>, DomainError> {
        Ok(self.summary.clone())
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

#[derive(Default)]
struct FakePublisher {
    pub(crate) sender_updates: Mutex<Vec<(String, Value)>>,
}

impl NotificationDeliveryPublisher for FakePublisher {
    fn publish_user_notification<'a>(
        &'a self,
        _notification: &'a fms_application::services::notification_service::NotificationResponse,
        _unread_count: i64,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }

    fn publish_sender_receipt_update<'a>(
        &'a self,
        sender_user_id: &'a str,
        payload: Value,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async move {
            self.sender_updates
                .lock()
                .expect("lock sender updates")
                .push((sender_user_id.to_string(), payload));
            Ok(1)
        })
    }
}

fn claims(user_id: &str, permissions: &[&str]) -> JwtAuth {
    JwtAuth(TokenData {
        sub: Some(user_id.to_string()),
        email: None,
        username: Some(user_id.to_string()),
        token_kind: Some("access".to_string()),
        is_admin: Some(false),
        permissions: permissions.iter().map(|value| value.to_string()).collect(),
        department: None,
        department_id: None,
        pv: None,
        iat: None,
        exp: None,
        iss: None,
        aud: None,
        ua_hash: None,
        ip_subnet_hash: None,
    })
}

fn sample_notification() -> Notification {
    Notification {
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
        created_at: Utc::now(),
        read_at: None,
    }
}

#[actix_web::test]
async fn receipt_group_allows_sender_without_dispatch_permission() {
    let item = sample_notification();
    let repo = Arc::new(FakeNotificationRepo {
        item: Some(item.clone()),
        ack_result: Some(item.clone()),
        summary: Some(json!({
            "title": "Dispatch",
            "severity": "warning",
            "flight_id": "flight-1",
            "dispatch_order_id": "order-1",
            "group_id": null,
            "created_at": item.created_at,
            "origin_type": "workflow",
            "receipt_required": true,
            "sender_user_id": "sender-1",
            "sender_username": "sender-name",
            "total_count": 1,
            "pending_count": 1,
            "acknowledged_count": 0,
            "rejected_count": 0,
            "latest_updated_at": item.created_at,
        })),
    });
    let svc = Arc::new(NotificationService::new(repo, Arc::new(FakePreferenceRepo)));

    let response = get_receipt_group_inner(
        svc.as_ref(),
        web::Path::from("rg-1".to_string()),
        claims("sender-1", &[]),
    )
    .await;

    assert!(response.is_ok());
}

#[actix_web::test]
async fn ack_returns_internal_when_service_returns_none_after_pending_check() {
    let item = sample_notification();
    let publisher = Arc::new(FakePublisher::default());
    let repo = Arc::new(FakeNotificationRepo {
        item: Some(item),
        ack_result: None,
        summary: None,
    });
    let svc = Arc::new(NotificationService::new(repo, Arc::new(FakePreferenceRepo)).with_delivery_publisher(publisher));

    let result = ack_notification_inner(
        svc.as_ref(),
        web::Path::from("n-1".to_string()),
        web::Json(NotificationAckRequest {
            action: "acknowledged".to_string(),
            note: None,
        }),
        claims("user-1", &[]),
    )
    .await;

    match result {
        Err(ApiError::Internal(message)) => {
            assert_eq!(message, "Failed to acknowledge notification");
        }
        other => panic!("expected internal error, got {other:?}"),
    }
}

#[actix_web::test]
async fn notification_stream_registers_broadcast_waker_for_realtime_delivery() {
    let (sender, receiver) = broadcast::channel(8);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
    heartbeat.tick().await;
    let mut stream = NotificationSseStream {
        receiver: BroadcastStream::new(receiver),
        heartbeat,
    };
    let wake = Arc::new(CountWake {
        count: AtomicUsize::new(0),
    });
    let waker = Waker::from(wake.clone());
    let mut cx = Context::from_waker(&waker);

    assert!(matches!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Pending));

    sender
        .send(SseMessage {
            topic: "user_notifications_user-1".to_string(),
            event: Some("user_notification".to_string()),
            serialized_data: Arc::new(json!({"notification_id": "n-1"}).to_string()),
        })
        .expect("message should send");
    tokio::task::yield_now().await;
    assert!(
        wake.count.load(Ordering::SeqCst) > 0,
        "broadcast send should wake a pending notification stream immediately"
    );
}
