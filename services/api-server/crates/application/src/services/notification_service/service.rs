use std::sync::Arc;

use chrono::Utc;
use tracing::{error, warn};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;
use fms_domain::models::notification::{Notification, NotificationPreference};
use fms_domain::ports::notification_repository::{
    NotificationPreferenceRepository, NotificationRepository, NotificationTransactionalRepository,
};

use super::helpers::{
    build_remind_after_at, default_preference, get_val, is_muted_now, is_overdue, normalize_ack_action,
    normalize_category_overrides, normalize_note, normalize_optional_text, normalize_origin_type, normalize_time_text,
    origin_label, receipt_group_summary_to_value, receipt_to_value, to_response,
};
use super::schemas::{
    DispatchBatchNotificationCreate, NotificationCreate, NotificationPreferenceUpdate, NotificationResponse,
};
use super::traits::{
    NotificationCollaborationEvents, NotificationDeliveryPublisher, NotificationMetricsRecorder,
    NotificationReceiptGroupSync,
};

/// 通知应用服务
pub struct NotificationService<
    NR: NotificationRepository + ?Sized,
    PR: NotificationPreferenceRepository + ?Sized,
    CE: NotificationCollaborationEvents + ?Sized,
    DP: NotificationDeliveryPublisher + ?Sized,
    MR: NotificationMetricsRecorder + ?Sized,
    RS: NotificationReceiptGroupSync + ?Sized,
> {
    pub(crate) repo: Arc<NR>,
    pub(crate) preference_repo: Arc<PR>,
    pub(crate) collaboration_events: Arc<CE>,
    pub(crate) delivery_publisher: Arc<DP>,
    pub(crate) metrics_recorder: Arc<MR>,
    /// 这里的 `RwLock` 是循环依赖的断点（notification ↔ business_case_workflow），
    /// 不是可选性：里面永远有一个实现，只是可以被换掉。
    pub(crate) receipt_group_sync: std::sync::RwLock<Arc<RS>>,
}

impl<
        NR: NotificationRepository + ?Sized,
        PR: NotificationPreferenceRepository + ?Sized,
        CE: NotificationCollaborationEvents + ?Sized,
        DP: NotificationDeliveryPublisher + ?Sized,
        MR: NotificationMetricsRecorder + ?Sized,
        RS: NotificationReceiptGroupSync + ?Sized,
    > NotificationService<NR, PR, CE, DP, MR, RS>
{
    pub fn new(
        repo: Arc<NR>,
        preference_repo: Arc<PR>,
        collaboration_events: Arc<CE>,
        delivery_publisher: Arc<DP>,
        metrics_recorder: Arc<MR>,
        receipt_group_sync: Arc<RS>,
    ) -> Self {
        Self {
            repo,
            preference_repo,
            collaboration_events,
            delivery_publisher,
            metrics_recorder,
            receipt_group_sync: std::sync::RwLock::new(receipt_group_sync),
        }
    }

    /// 打断 notification ↔ business_case_workflow 的构造环：先用一个占位实现建服务，
    /// 等对面服务建好后换进来。换之前也不是「没接线」，只是还没接到真实现。
    pub fn set_receipt_group_sync(&self, receipt_group_sync: Arc<RS>) {
        *self
            .receipt_group_sync
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = receipt_group_sync;
    }

    /// 获取用户通知列表
    pub async fn list_notifications(
        &self,
        user_id: &str,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NotificationResponse>, DomainError> {
        let items = self.repo.find_by_user(user_id, unread_only, limit, offset).await?;
        Ok(items.iter().map(to_response).collect())
    }

    /// 获取未读数
    pub async fn get_unread_count(&self, user_id: &str) -> Result<i64, DomainError> {
        self.repo.count_unread(user_id).await
    }

    pub async fn get_notification(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<Option<Notification>, DomainError> {
        self.repo.find_by_id_for_user(notification_id, user_id).await
    }

    /// 已读单条
    pub async fn mark_read(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError> {
        let updated = self.repo.mark_read(notification_id, user_id).await?;
        if updated {
            let _ = self.repo.mark_delivered(notification_id, user_id).await?;
        }
        Ok(updated)
    }

    pub async fn acknowledge(
        &self,
        notification_id: &str,
        user_id: &str,
        action: &str,
        note: Option<&str>,
        actor_username: Option<&str>,
    ) -> Result<Option<Notification>, DomainError> {
        let normalized_action = normalize_ack_action(action)?;
        let normalized_note = normalize_note(note);
        if normalized_action == "rejected" && normalized_note.is_none() {
            return Err(DomainError::ValidationError(
                "rejected notifications require a note".into(),
            ));
        }

        let Some(existing) = self.repo.find_by_id_for_user(notification_id, user_id).await? else {
            return Ok(None);
        };

        if existing.ack_status != "pending" {
            return Ok(None);
        }

        let updated = self
            .repo
            .acknowledge(notification_id, user_id, normalized_action, normalized_note.as_deref())
            .await?;

        if let Some(notification) = updated.as_ref() {
            self.record_ack_collaboration_event(notification, user_id).await;
            self.sync_receipt_group_workflow(notification).await;
            self.publish_sender_receipt_update(notification, actor_username).await?;
        }

        Ok(updated)
    }

    /// 全部已读
    pub async fn mark_all_read(&self, user_id: &str) -> Result<i64, DomainError> {
        self.repo.mark_all_read(user_id).await
    }

    pub async fn get_preferences(&self, user_id: &str) -> Result<NotificationPreference, DomainError> {
        match self.preference_repo.find_by_user(user_id).await? {
            Some(preference) => Ok(preference),
            None => Ok(default_preference(user_id)),
        }
    }

    pub async fn update_preferences(
        &self,
        user_id: &str,
        patch: NotificationPreferenceUpdate,
    ) -> Result<NotificationPreference, DomainError> {
        let mut preference = self.get_preferences(user_id).await?;
        if let Some(value) = patch.in_app_enabled {
            preference.in_app_enabled = value;
        }
        if let Some(value) = patch.external_enabled {
            preference.external_enabled = value;
        }
        if let Some(value) = patch.external_channel {
            preference.external_channel = normalize_optional_text(Some(value)).unwrap_or_else(|| "none".to_string());
        }
        if patch.mute_start.is_some() {
            preference.mute_start = normalize_time_text(patch.mute_start)?;
        }
        if patch.mute_end.is_some() {
            preference.mute_end = normalize_time_text(patch.mute_end)?;
        }
        if let Some(value) = patch.critical_override {
            preference.critical_override = value;
        }
        if let Some(value) = patch.category_overrides {
            preference.category_overrides = normalize_category_overrides(value);
        }
        preference.updated_at = Utc::now();
        self.preference_repo.save(&preference).await?;
        Ok(preference)
    }

    pub async fn get_receipt_group(&self, receipt_group_id: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let summary = self.repo.summarize_receipt_group(receipt_group_id).await?;
        let Some(summary) = summary else {
            return Ok(None);
        };
        let items = self.repo.find_by_receipt_group(receipt_group_id).await?;
        let empty_map = serde_json::Map::new();
        let map = summary.as_object().unwrap_or(&empty_map);
        let remind_after_at = build_remind_after_at(map.get("created_at"));
        let pending_count = map.get("pending_count").and_then(|value| value.as_i64()).unwrap_or(0);
        let normalized_origin_type = normalize_origin_type(map.get("origin_type").and_then(|value| value.as_str()));
        let info_severity = serde_json::Value::String("info".to_string());
        let true_val = serde_json::Value::Bool(true);
        let zero = serde_json::Value::from(0);
        Ok(Some(serde_json::json!({
            "receipt_group_id": receipt_group_id,
            "title": get_val(map, "title"),
            "severity": map.get("severity").unwrap_or(&info_severity),
            "flight_id": get_val(map, "flight_id"),
            "dispatch_order_id": get_val(map, "dispatch_order_id"),
            "group_id": get_val(map, "group_id"),
            "created_at": get_val(map, "created_at"),
            "origin_label": origin_label(Some(&normalized_origin_type)),
            "origin_type": normalized_origin_type,
            "receipt_required": map.get("receipt_required").unwrap_or(&true_val),
            "sender_user_id": get_val(map, "sender_user_id"),
            "sender_username": get_val(map, "sender_username"),
            "remind_after_at": remind_after_at,
            "is_overdue": is_overdue(pending_count, remind_after_at.as_ref()),
            "summary": {
                "total_count": map.get("total_count").unwrap_or(&zero),
                "pending_count": map.get("pending_count").unwrap_or(&zero),
                "acknowledged_count": map.get("acknowledged_count").unwrap_or(&zero),
                "rejected_count": map.get("rejected_count").unwrap_or(&zero),
                "latest_updated_at": get_val(map, "latest_updated_at"),
            },
            "items": items.iter().map(receipt_to_value).collect::<Vec<_>>(),
        })))
    }

    pub async fn list_sent_receipt_groups(
        &self,
        sender_user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<serde_json::Value, DomainError> {
        let limit = limit.clamp(1, 100);
        let offset = offset.max(0);
        let rows = self
            .repo
            .list_sent_receipt_groups(sender_user_id, limit, offset)
            .await?;
        let total = rows
            .first()
            .and_then(|row| row.get("matched_groups"))
            .and_then(|value| value.as_i64())
            .unwrap_or(0);

        Ok(serde_json::json!({
            "items": rows.iter().map(receipt_group_summary_to_value).collect::<Vec<_>>(),
            "total": total,
            "limit": limit,
            "offset": offset,
        }))
    }

    /// 发送通知
    pub async fn send_notification(&self, dto: NotificationCreate) -> Result<NotificationResponse, DomainError> {
        self.send(dto, None, None).await
    }

    /// 这个方法是泛型的，服务本身不是。
    ///
    /// 通知服务已经带着六个类型参数，且有一大票 `web::Data` 注入点；把事务类型加成第七个
    /// 参数会落到每一个注入点上。而这段 40 行的逻辑要用到本服务 7 个实例方法（共 151 行）
    /// 和 6 个字段里的 5 个——搬到独立写入方就得把服务大半复制过去，或者让写入方反过来
    /// 持有服务，那只是多一层转发。
    ///
    /// 所以事务仓储改由调用方传入：`DomainActionExecutor` 本来就已经这样持有
    /// `anomaly_tx_repo`。方法级泛型在这里是安全的——这个方法不属于任何 trait，
    /// 没有对象安全的约束。
    pub async fn send_notification_in_tx<Tx: Send>(
        &self,
        tx: &mut Tx,
        tx_repo: &(dyn NotificationTransactionalRepository<Tx> + Send + Sync),
        dto: NotificationCreate,
    ) -> Result<NotificationResponse, DomainError> {
        let mut notification = self.build_notification(dto, None, None)?;
        let preference = self.get_preferences(&notification.user_id).await?;
        let severity_normalized = notification.severity.trim().to_ascii_lowercase();
        let category_key = notification.category.trim().to_ascii_lowercase();
        let category_enabled = preference
            .category_overrides
            .get(&category_key)
            .copied()
            .unwrap_or(true);
        let critical_override = preference.critical_override && severity_normalized == "critical";
        let muted_now = is_muted_now(&preference, Utc::now());
        let mut should_send_in_app =
            preference.in_app_enabled && (category_enabled || critical_override) && (!muted_now || critical_override);
        if notification.receipt_required {
            should_send_in_app = true;
        }
        if should_send_in_app {
            tx_repo.save_in_tx(tx, &notification).await?;
            self.metrics_recorder.record_delivery_attempt("in_app", true);
            self.record_created_collaboration_event(&notification).await;
            if notification.receipt_required {
                self.record_receipt_required_collaboration_event(&notification).await;
            }
            let unread_count = self.get_unread_count(&notification.user_id).await.unwrap_or(0);
            if self
                .publish_in_app_notification(&mut notification, unread_count)
                .await?
            {
                self.record_delivered_collaboration_event(&notification, "sse").await;
            }
        }
        Ok(to_response(&notification))
    }

    pub async fn send_batch(&self, dto: DispatchBatchNotificationCreate) -> Result<serde_json::Value, DomainError> {
        self.send_batch_with_idempotency(dto, None, None).await
    }

    pub async fn send_batch_with_idempotency(
        &self,
        dto: DispatchBatchNotificationCreate,
        receipt_group_id_override: Option<String>,
        notification_id_seed: Option<String>,
    ) -> Result<serde_json::Value, DomainError> {
        let mut seen_user_ids = std::collections::HashSet::new();
        let normalized_user_ids = dto
            .user_ids
            .into_iter()
            .map(|user_id| user_id.trim().to_string())
            .filter(|user_id| !user_id.is_empty())
            .filter(|user_id| seen_user_ids.insert(user_id.clone()))
            .collect::<Vec<_>>();
        if normalized_user_ids.is_empty() {
            return Ok(serde_json::json!({
                "receipt_group_id": serde_json::Value::Null,
                "items": [],
            }));
        }

        let allow_receipt = dto.receipt_required;
        let receipt_group_id = if allow_receipt {
            normalize_optional_text(receipt_group_id_override).or_else(|| Some(ulid::Ulid::new().to_string()))
        } else {
            None
        };
        let notification_id_seed = normalize_optional_text(notification_id_seed);

        let mut items = Vec::new();
        for normalized_user_id in normalized_user_ids {
            let notification_id = notification_id_seed
                .as_deref()
                .map(|seed| super::helpers::stable_notification_id(seed, &normalized_user_id));
            let item = self
                .send(
                    NotificationCreate {
                        user_id: normalized_user_id.clone(),
                        title: dto.title.clone(),
                        body: dto.body.clone(),
                        category: Some(dto.category.clone()),
                        severity: Some(dto.severity.clone()),
                        flight_id: dto.flight_id.clone(),
                        related_entity_type: dto.related_entity_type.clone(),
                        related_entity_id: dto.related_entity_id.clone(),
                        dispatch_order_id: dto.dispatch_order_id.clone(),
                        group_id: dto.group_id.clone(),
                        sender_user_id: dto.sender_user_id.clone(),
                        sender_username_snapshot: dto.sender_username_snapshot.clone(),
                        origin_type: Some(dto.origin_type.clone()),
                        receipt_required: allow_receipt,
                        receipt_group_id: receipt_group_id.clone(),
                    },
                    receipt_group_id.clone(),
                    notification_id,
                )
                .await?;
            items.push(item);
        }

        Ok(serde_json::json!({
            "receipt_group_id": receipt_group_id,
            "items": items,
        }))
    }

    pub(crate) fn build_notification(
        &self,
        dto: NotificationCreate,
        receipt_group_id_override: Option<String>,
        notification_id_override: Option<String>,
    ) -> Result<Notification, DomainError> {
        let user_id = dto.user_id.trim().to_string();
        let title = dto.title.trim().to_string();
        if user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }
        if title.is_empty() {
            return Err(DomainError::ValidationError("title is required".into()));
        }

        let severity = normalize_optional_text(dto.severity).unwrap_or_else(|| "info".into());
        let receipt_required = dto.receipt_required;
        let now = Utc::now();
        let origin_type = normalize_origin_type(dto.origin_type.as_deref());
        Ok(Notification {
            notification_id: notification_id_override.unwrap_or_else(|| ulid::Ulid::new().to_string()),
            user_id,
            title,
            body: dto.body.trim().to_string(),
            category: normalize_optional_text(dto.category).unwrap_or_else(|| "system".into()),
            severity,
            is_read: false,
            flight_id: normalize_optional_text(dto.flight_id),
            related_entity_type: normalize_optional_text(dto.related_entity_type),
            related_entity_id: normalize_optional_text(dto.related_entity_id),
            dispatch_order_id: normalize_optional_text(dto.dispatch_order_id),
            group_id: normalize_optional_text(dto.group_id),
            event_id: None,
            sender_user_id: normalize_optional_text(dto.sender_user_id),
            sender_username_snapshot: normalize_optional_text(dto.sender_username_snapshot),
            recipient_username_snapshot: None,
            recipient_display_name_snapshot: None,
            recipient_department_snapshot: None,
            recipient_job_title_snapshot: None,
            origin_type,
            receipt_required,
            receipt_group_id: receipt_group_id_override.or(dto.receipt_group_id),
            delivery_status: "sent".into(),
            delivered_at: None,
            ack_status: "pending".into(),
            ack_at: None,
            ack_note: None,
            created_at: now,
            read_at: None,
        })
    }

    pub(crate) async fn send(
        &self,
        dto: NotificationCreate,
        receipt_group_id_override: Option<String>,
        notification_id_override: Option<String>,
    ) -> Result<NotificationResponse, DomainError> {
        let mut notification = self.build_notification(dto, receipt_group_id_override, notification_id_override)?;
        let preference = self.get_preferences(&notification.user_id).await?;
        let severity_normalized = notification.severity.trim().to_ascii_lowercase();
        let category_key = notification.category.trim().to_ascii_lowercase();
        let category_enabled = preference
            .category_overrides
            .get(&category_key)
            .copied()
            .unwrap_or(true);
        let critical_override = preference.critical_override && severity_normalized == "critical";
        let muted_now = is_muted_now(&preference, Utc::now());

        let mut should_send_in_app =
            preference.in_app_enabled && (category_enabled || critical_override) && (!muted_now || critical_override);
        if notification.receipt_required {
            should_send_in_app = true;
        }
        if should_send_in_app {
            self.repo.save(&notification).await?;
            self.metrics_recorder.record_delivery_attempt("in_app", true);
            self.record_created_collaboration_event(&notification).await;
            if notification.receipt_required {
                self.record_receipt_required_collaboration_event(&notification).await;
            }

            let unread_count = self.get_unread_count(&notification.user_id).await.unwrap_or(0);
            if self
                .publish_in_app_notification(&mut notification, unread_count)
                .await?
            {
                self.record_delivered_collaboration_event(&notification, "sse").await;
            }
        }

        Ok(to_response(&notification))
    }

    pub(crate) async fn publish_in_app_notification(
        &self,
        notification: &mut Notification,
        unread_count: i64,
    ) -> Result<bool, DomainError> {
        let response = to_response(notification);
        let delivered = self
            .delivery_publisher
            .publish_user_notification(&response, unread_count)
            .await?;
        if delivered == 0 {
            self.metrics_recorder.record_backfill_pending();
            return Ok(false);
        }

        let _ = self
            .repo
            .mark_delivered(&notification.notification_id, &notification.user_id)
            .await?;
        if notification.delivered_at.is_none() {
            notification.delivered_at = Some(Utc::now());
        }
        notification.delivery_status = "delivered".to_string();
        Ok(true)
    }

    pub(crate) async fn publish_sender_receipt_update(
        &self,
        notification: &Notification,
        actor_username: Option<&str>,
    ) -> Result<(), DomainError> {
        let Some(sender_user_id) = notification
            .sender_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let Some(receipt_group_id) = notification
            .receipt_group_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let Some(summary) = self.repo.summarize_receipt_group(receipt_group_id).await? else {
            return Ok(());
        };
        let empty_map = serde_json::Map::new();
        let summary_map = summary.as_object().unwrap_or(&empty_map);
        let remind_after_at = build_remind_after_at(summary_map.get("created_at"));
        let pending_count = summary_map
            .get("pending_count")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);
        let actor_account_name = actor_username
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != notification.user_id.as_str());
        let severity_fallback = serde_json::Value::String(notification.severity.clone());
        let flight_id_fallback = notification
            .flight_id
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null);
        let zero = serde_json::Value::from(0);
        let payload = serde_json::json!({
            "type": "sender_receipt_update",
            "receipt_group_id": receipt_group_id,
            "notification_id": notification.notification_id,
            "recipient_user_id": notification.user_id,
            "recipient_username": actor_account_name
                .or(notification.recipient_username_snapshot.as_deref())
                .or(notification.recipient_display_name_snapshot.as_deref())
                .unwrap_or("未知账号"),
            "recipient_display_name": notification.recipient_display_name_snapshot,
            "recipient_department": notification.recipient_department_snapshot,
            "recipient_job_title": notification.recipient_job_title_snapshot,
            "ack_status": notification.ack_status,
            "ack_note": notification.ack_note,
            "ack_at": notification.ack_at,
            "title": get_val(summary_map, "title"),
            "severity": summary_map.get("severity").unwrap_or(&severity_fallback),
            "origin_type": normalize_origin_type(summary_map.get("origin_type").and_then(|value| value.as_str())),
            "flight_id": summary_map.get("flight_id").unwrap_or(&flight_id_fallback),
            "timestamp": Utc::now(),
            "summary": {
                "total_count": summary_map.get("total_count").unwrap_or(&zero),
                "pending_count": summary_map.get("pending_count").unwrap_or(&zero),
                "acknowledged_count": summary_map.get("acknowledged_count").unwrap_or(&zero),
                "rejected_count": summary_map.get("rejected_count").unwrap_or(&zero),
                "latest_updated_at": get_val(summary_map, "latest_updated_at"),
                "remind_after_at": remind_after_at,
                "is_overdue": is_overdue(pending_count, remind_after_at.as_ref()),
            },
        });
        if let Err(error) = self
            .delivery_publisher
            .publish_sender_receipt_update(sender_user_id, payload)
            .await
        {
            warn!(
                sender_user_id,
                receipt_group_id,
                error = %error,
                "failed to publish sender receipt update"
            );
        }
        Ok(())
    }

    async fn record_created_collaboration_event(&self, notification: &Notification) {
        self.record_notification_collaboration_event(
            notification,
            "notification_created",
            Some(notification.user_id.as_str()),
            notification
                .event_id
                .clone()
                .or_else(|| Some(notification.notification_id.clone())),
            serde_json::json!({
                "notification_id": notification.notification_id,
                "user_id": notification.user_id,
                "title": notification.title,
                "category": notification.category,
                "severity": notification.severity.to_ascii_lowercase(),
                "sender_user_id": notification.sender_user_id,
                "sender_username": notification.sender_username_snapshot,
                "origin_type": notification.origin_type,
                "receipt_required": notification.receipt_required,
                "receipt_group_id": notification.receipt_group_id,
                "related_entity_type": notification.related_entity_type,
                "related_entity_id": notification.related_entity_id,
                "delivery_status": notification.delivery_status,
            }),
            notification.created_at,
        )
        .await;
    }

    async fn record_receipt_required_collaboration_event(&self, notification: &Notification) {
        let Some(receipt_group_id) = notification.receipt_group_id.clone() else {
            return;
        };

        self.record_notification_collaboration_event(
            notification,
            "notification_receipt_required",
            Some(notification.user_id.as_str()),
            Some(receipt_group_id.clone()),
            serde_json::json!({
                "notification_id": notification.notification_id,
                "receipt_group_id": receipt_group_id,
                "sender_user_id": notification.sender_user_id,
                "origin_type": notification.origin_type,
            }),
            notification.created_at,
        )
        .await;
    }

    async fn record_delivered_collaboration_event(&self, notification: &Notification, channel: &str) {
        self.record_notification_collaboration_event(
            notification,
            "notification_delivered",
            Some(notification.user_id.as_str()),
            notification
                .event_id
                .clone()
                .or_else(|| Some(notification.notification_id.clone())),
            serde_json::json!({
                "notification_id": notification.notification_id,
                "channel": channel,
                "delivery_status": notification.delivery_status,
            }),
            notification.delivered_at.unwrap_or(notification.created_at),
        )
        .await;
    }

    async fn record_notification_collaboration_event(
        &self,
        notification: &Notification,
        event_type: &str,
        actor_user_id: Option<&str>,
        correlation_id: Option<String>,
        payload: serde_json::Value,
        occurred_at: chrono::DateTime<chrono::Utc>,
    ) {
        let Some(flight_id) = notification.flight_id.as_ref() else {
            return;
        };

        let event = DispatchCollaborationEvent {
            event_id: ulid::Ulid::new().to_string(),
            flight_id: flight_id.clone(),
            dispatch_order_id: notification.dispatch_order_id.clone(),
            group_id: notification.group_id.clone(),
            event_type: event_type.to_string(),
            actor_user_id: actor_user_id.map(str::to_string),
            actor_username: None,
            correlation_id,
            payload,
            occurred_at,
            source_table: Some("notifications".to_string()),
            source_record_id: Some(notification.notification_id.clone()),
        };

        if let Err(error) = self.collaboration_events.create_event(&event).await {
            warn!(
                notification_id = %notification.notification_id,
                flight_id = %flight_id,
                event_type,
                error = %error,
                "failed to record notification collaboration event"
            );
        }
    }

    async fn record_ack_collaboration_event(&self, notification: &Notification, user_id: &str) {
        let event_type = if notification.ack_status == "rejected" {
            "notification_rejected"
        } else {
            "notification_acknowledged"
        };
        self.record_notification_collaboration_event(
            notification,
            event_type,
            Some(user_id),
            Some(
                notification
                    .event_id
                    .clone()
                    .unwrap_or_else(|| notification.notification_id.clone()),
            ),
            serde_json::json!({
                "notification_id": notification.notification_id,
                "ack_status": notification.ack_status,
                "ack_note": notification.ack_note,
                "receipt_group_id": notification.receipt_group_id,
                "origin_type": notification.origin_type,
            }),
            notification.ack_at.unwrap_or_else(Utc::now),
        )
        .await;
    }

    async fn sync_receipt_group_workflow(&self, notification: &Notification) {
        let Some(receipt_group_id) = notification.receipt_group_id.as_deref() else {
            return;
        };
        let receipt_group_sync = self
            .receipt_group_sync
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let mut last_error = None;
        for attempt in 0..2 {
            match receipt_group_sync.sync_receipt_group(receipt_group_id).await {
                Ok(()) => return,
                Err(error) => {
                    last_error = Some(error);
                    if attempt == 0 {
                        warn!(
                            notification_id = %notification.notification_id,
                            receipt_group_id,
                            attempt = 1,
                            "receipt group sync failed, retrying"
                        );
                    }
                }
            }
        }

        if let Some(error) = last_error {
            error!(
                notification_id = %notification.notification_id,
                receipt_group_id,
                error = %error,
                "receipt group sync PERMANENTLY failed after 2 attempts. Workflow may be stuck."
            );
        }
    }
}
