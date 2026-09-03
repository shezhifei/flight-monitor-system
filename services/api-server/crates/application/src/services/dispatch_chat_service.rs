//! 派工聊天服务。

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use tracing::warn;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, DispatchOrderStatus};
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;
use fms_domain::models::dispatch_collaboration::{
    DispatchChatGroupList, DispatchChatGroupSummary, DispatchChatMember, DispatchChatMemberUpsert, DispatchChatMessage,
    DispatchChatMessageCursor, DispatchChatUserProfile, NewDispatchChatMessage,
};
use fms_domain::models::flight::Flight;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};

use crate::services::notification_service::{
    DispatchBatchNotificationCreate, NotificationCollaborationEvents, NotificationDeliveryPublisher,
    NotificationMetricsRecorder, NotificationReceiptGroupSync, NotificationService,
};

const DEPRECATION_REASON_ARRIVAL_GUARANTEE_COMPLETED: &str = "arrival_guarantee_completed";
const DEPRECATION_REASON_DEPARTURE_DEPARTED: &str = "departure_departed";
const DEPRECATION_REASON_TRANSIT_DEPARTED: &str = "transit_departed";

#[derive(Debug, Clone)]
pub enum DispatchChatLifecycleChange {
    Upserted {
        group_id: String,
    },
    Archived {
        group_id: String,
        archived_at: DateTime<Utc>,
    },
}

pub trait DispatchChatEventPublisher: Send + Sync {
    fn publish_user_event<'a>(
        &'a self,
        event_name: &'a str,
        events: Vec<(String, serde_json::Value)>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

#[async_trait]
pub trait DispatchChatMentionNotifier: Send + Sync {
    async fn notify_chat_mentions(&self, dto: DispatchBatchNotificationCreate) -> Result<(), DomainError>;
}

#[async_trait]
impl<NR, PR, CE, DP, MR, RS> DispatchChatMentionNotifier for NotificationService<NR, PR, CE, DP, MR, RS>
where
    NR: NotificationRepository + Send + Sync + ?Sized,
    PR: NotificationPreferenceRepository + Send + Sync + ?Sized,
    CE: NotificationCollaborationEvents + Send + Sync + ?Sized,
    DP: NotificationDeliveryPublisher + Send + Sync + ?Sized,
    MR: NotificationMetricsRecorder + Send + Sync + ?Sized,
    RS: NotificationReceiptGroupSync + Send + Sync + ?Sized,
{
    async fn notify_chat_mentions(&self, dto: DispatchBatchNotificationCreate) -> Result<(), DomainError> {
        self.send_batch(dto).await.map(|_| ())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchChatError {
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Archived(String),
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// Outcome of a send attempt.
///
/// `deduplicated` marks a retry that resolved to an already-stored message: the
/// caller must return it as-is and must **not** fan it out again, or the client
/// sees the same message twice.
#[derive(Debug, Clone)]
pub struct DispatchChatSendOutcome {
    pub message: DispatchChatMessage,
    pub deduplicated: bool,
}

/// Outcome of a mark-read attempt.
///
/// `advanced` is false for an idempotent re-read; such a call must not append an
/// audit ledger row or fan out a read-sync frame.
#[derive(Debug, Clone)]
pub struct DispatchChatReadOutcome {
    pub payload: serde_json::Value,
    pub last_read_seq: i64,
    pub advanced: bool,
}

pub struct DispatchChatService {
    collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    dispatch_order_repo: Option<Arc<dyn DispatchOrderRepository + Send + Sync>>,
    flight_repo: Option<Arc<dyn FlightRepository + Send + Sync>>,
    event_publisher: Option<Arc<dyn DispatchChatEventPublisher + Send + Sync>>,
    mention_notifier: Option<Arc<dyn DispatchChatMentionNotifier>>,
}

impl DispatchChatService {
    pub fn new(collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>) -> Self {
        Self {
            collaboration_repo,
            dispatch_order_repo: None,
            flight_repo: None,
            event_publisher: None,
            mention_notifier: None,
        }
    }
}

impl DispatchChatService {
    pub fn with_dispatch_order_repo(
        mut self,
        dispatch_order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    ) -> Self {
        self.dispatch_order_repo = Some(dispatch_order_repo);
        self
    }

    pub fn with_flight_repo(mut self, flight_repo: Arc<dyn FlightRepository + Send + Sync>) -> Self {
        self.flight_repo = Some(flight_repo);
        self
    }

    pub fn with_event_publisher(mut self, event_publisher: Arc<dyn DispatchChatEventPublisher + Send + Sync>) -> Self {
        self.event_publisher = Some(event_publisher);
        self
    }

    pub fn with_mention_notifier(mut self, mention_notifier: Arc<dyn DispatchChatMentionNotifier>) -> Self {
        self.mention_notifier = Some(mention_notifier);
        self
    }

    pub async fn sync_group_for_dispatch_order_id(
        &self,
        dispatch_order_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DispatchChatError> {
        let normalized_dispatch_order_id = normalize(dispatch_order_id);
        if normalized_dispatch_order_id.is_empty() {
            return Ok(None);
        }

        let Some(dispatch_order_repo) = self.dispatch_order_repo.as_ref() else {
            return Ok(None);
        };
        let Some(dispatch_order) = dispatch_order_repo
            .find_by_id(&normalized_dispatch_order_id, true, None)
            .await?
        else {
            return Ok(None);
        };

        self.sync_group_for_dispatch_order(&dispatch_order).await
    }

    pub async fn sync_group_for_dispatch_order(
        &self,
        dispatch_order: &DispatchOrder,
    ) -> Result<Option<DispatchChatGroupSummary>, DispatchChatError> {
        let actor_user_id = dispatch_order
            .dispatched_by
            .as_deref()
            .map(normalize)
            .filter(|value| !value.is_empty());
        self.sync_group_for_flight_context(
            &dispatch_order.flight_id,
            Some(dispatch_order.id.as_str()),
            actor_user_id.as_deref(),
        )
        .await
    }

    pub async fn sync_group_for_flight_id(
        &self,
        flight_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DispatchChatError> {
        self.sync_group_for_flight_context(flight_id, None, None).await
    }

    async fn sync_group_for_flight_context(
        &self,
        flight_id: &str,
        trigger_dispatch_order_id: Option<&str>,
        actor_user_id: Option<&str>,
    ) -> Result<Option<DispatchChatGroupSummary>, DispatchChatError> {
        let flight_id = normalize(flight_id);
        if flight_id.is_empty() {
            return Ok(None);
        }

        let Some(dispatch_order_repo) = self.dispatch_order_repo.as_ref() else {
            return Ok(None);
        };

        let orders = dispatch_order_repo.find_by_flight(&flight_id).await?;
        let relevant_orders = orders
            .iter()
            .filter(|order| is_dispatch_order_chat_relevant(order))
            .collect::<Vec<_>>();
        if relevant_orders.is_empty() {
            return Ok(None);
        }
        let active_orders = relevant_orders
            .iter()
            .copied()
            .filter(|order| !is_dispatch_order_terminal(order))
            .collect::<Vec<_>>();
        let assignee_user_ids = collect_assignee_user_ids_from_orders(&active_orders);
        let assignee_profiles = self.collaboration_repo.find_users_by_ids(&assignee_user_ids).await?;
        let related_departments = collect_related_departments(&active_orders, &assignee_profiles);
        let dispatcher_candidates = self
            .collaboration_repo
            .find_dispatchers_by_departments(&related_departments)
            .await?;

        let flight = self.load_flight(&flight_id).await?;
        let archive_at = flight.as_ref().and_then(resolve_archive_at);
        let group_name = build_group_name(&flight_id, flight.as_ref());
        let group_metadata =
            build_group_metadata(flight.as_ref(), &relevant_orders, &active_orders, &related_departments);
        let mut group = self
            .collaboration_repo
            .upsert_group_for_flight(&flight_id, &group_name, archive_at, &group_metadata)
            .await?;
        let group_id = group.group_id.clone();

        if is_arrival_flight(flight.as_ref()) && !active_orders.is_empty() {
            let _ = self
                .collaboration_repo
                .clear_group_deprecation(&group_id, DEPRECATION_REASON_ARRIVAL_GUARANTEE_COMPLETED)
                .await?;
        }

        let latest_seq = self.collaboration_repo.get_group_latest_seq(&group_id).await?;
        let existing_members = self.collaboration_repo.find_active_members(&group_id).await?;
        let existing_read_seq_map = existing_members
            .iter()
            .map(|member| (normalize(&member.user_id), member.last_read_seq))
            .filter(|(user_id, _)| !user_id.is_empty())
            .collect::<HashMap<_, _>>();

        let retained_dispatcher_user_ids = existing_members
            .iter()
            .filter(|member| member.is_dispatcher)
            .map(|member| normalize(&member.user_id))
            .filter(|user_id| !user_id.is_empty())
            .collect::<Vec<_>>();
        let memberships = build_group_memberships(
            &assignee_user_ids,
            &dispatcher_candidates,
            &retained_dispatcher_user_ids,
            &existing_read_seq_map,
            latest_seq,
        );
        let active_user_ids = memberships
            .iter()
            .map(|membership| membership.user_id.clone())
            .collect::<Vec<_>>();

        // Only rewrite memberships when we have a non-empty desired set.
        // An empty plan must not wipe existing members (was leaving groups with 0 members).
        let deactivated_members = if !memberships.is_empty() {
            self.collaboration_repo
                .upsert_group_memberships(&group_id, &memberships)
                .await?;
            self.collaboration_repo
                .deactivate_members_except(&group_id, &active_user_ids)
                .await?
        } else {
            Vec::new()
        };

        let added_user_ids = active_user_ids
            .iter()
            .filter(|user_id| !existing_read_seq_map.contains_key(*user_id))
            .cloned()
            .collect::<Vec<_>>();
        let removed_user_ids = deactivated_members
            .iter()
            .map(|member| normalize(&member.user_id))
            .filter(|user_id| !user_id.is_empty())
            .collect::<Vec<_>>();

        let correlation_id = ulid::Ulid::new().to_string();
        let actor_user_id = actor_user_id.map(normalize).filter(|value| !value.is_empty());
        self.collaboration_repo
            .create_event(&DispatchCollaborationEvent {
                event_id: ulid::Ulid::new().to_string(),
                flight_id: flight_id.clone(),
                dispatch_order_id: trigger_dispatch_order_id.map(str::to_string),
                group_id: Some(group_id.clone()),
                event_type: "group_upserted".to_string(),
                actor_user_id: actor_user_id.clone(),
                actor_username: None,
                correlation_id: Some(correlation_id.clone()),
                payload: json!({
                    "group_name": group.group_name,
                    "member_count": memberships.len(),
                    "related_departments": related_departments,
                    "dispatcher_user_ids": dispatcher_candidates.iter().map(|candidate| candidate.user_id.clone()).collect::<Vec<_>>(),
                    "active_dispatch_order_ids": active_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
                    "related_dispatch_order_ids": relevant_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
                }),
                occurred_at: Utc::now(),
                source_table: Some("dispatch_chat_groups".to_string()),
                source_record_id: Some(group_id.clone()),
            })
            .await?;
        if !memberships.is_empty() || !removed_user_ids.is_empty() {
            self.collaboration_repo
                .create_event(&DispatchCollaborationEvent {
                    event_id: ulid::Ulid::new().to_string(),
                    flight_id: flight_id.clone(),
                    dispatch_order_id: trigger_dispatch_order_id.map(str::to_string),
                    group_id: Some(group_id.clone()),
                    event_type: "group_member_synced".to_string(),
                    actor_user_id,
                    actor_username: None,
                    correlation_id: Some(correlation_id.clone()),
                    payload: json!({
                        "member_user_ids": active_user_ids,
                        "member_count": memberships.len(),
                        "related_departments": related_departments,
                        "added_user_ids": added_user_ids,
                        "deactivated_user_ids": removed_user_ids,
                    }),
                    occurred_at: Utc::now(),
                    source_table: Some("dispatch_chat_group_members".to_string()),
                    source_record_id: Some(group_id.clone()),
                })
                .await?;
        }

        if !added_user_ids.is_empty() || !removed_user_ids.is_empty() {
            let mut message_parts = Vec::new();
            if !added_user_ids.is_empty() {
                message_parts.push(format!("新增成员：{}", added_user_ids.join(", ")));
            }
            if !removed_user_ids.is_empty() {
                message_parts.push(format!("转只读成员：{}", removed_user_ids.join(", ")));
            }
            let event_id = ulid::Ulid::new().to_string();
            let mut message = self
                .collaboration_repo
                .insert_message(&NewDispatchChatMessage {
                    message_id: ulid::Ulid::new().to_string(),
                    group_id: group_id.clone(),
                    sender_user_id: None,
                    dispatch_order_id: trigger_dispatch_order_id.map(str::to_string),
                    event_id: Some(event_id.clone()),
                    message_type: "system".to_string(),
                    content: format!("系统消息：{}", message_parts.join("；")),
                    is_at_all: false,
                    metadata: json!({
                        "related_departments": related_departments,
                        "dispatcher_user_ids": dispatcher_candidates.iter().map(|candidate| candidate.user_id.clone()).collect::<Vec<_>>(),
                        "added_user_ids": added_user_ids,
                        "deactivated_user_ids": removed_user_ids,
                        "active_dispatch_order_ids": active_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
                        "related_dispatch_order_ids": relevant_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
                    }),
                    client_msg_id: None,
                })
                .await?;
            self.record_message_event(
                &flight_id,
                trigger_dispatch_order_id,
                &group_id,
                &message,
                Some(correlation_id),
                Some(event_id.clone()),
                None,
            )
            .await?;
            message.event_id = Some(event_id);
            self.emit_chat_message_event(&group_id, &message).await?;
        }

        let lifecycle_change = self.refresh_group_lifecycle_for_flight(&flight_id).await?;
        if let Some(updated_group) = self.collaboration_repo.get_group_by_id(&group_id).await? {
            group = updated_group;
        }

        if !matches!(
            lifecycle_change.as_ref(),
            Some(DispatchChatLifecycleChange::Upserted { .. })
        ) {
            self.emit_group_upserted_event(&group_id).await?;
        }
        if let Some(change) = lifecycle_change.as_ref() {
            self.emit_lifecycle_change(change).await?;
        }

        Ok(Some(group))
    }

    pub async fn list_user_groups(
        &self,
        user_id: &str,
        status: &str,
        limit: i64,
        offset: i64,
    ) -> Result<DispatchChatGroupList, DispatchChatError> {
        let normalized_user_id = normalize(user_id);
        let normalized_status = parse_status(status)?;
        if normalized_user_id.is_empty() {
            return Ok(DispatchChatGroupList {
                items: Vec::new(),
                total: 0,
                limit: limit.clamp(1, 200),
                offset: offset.max(0),
                unread_total: 0,
            });
        }

        let mut payload = self
            .collaboration_repo
            .list_user_groups(
                &normalized_user_id,
                normalized_status,
                limit.clamp(1, 200),
                offset.max(0),
            )
            .await?;

        if self.refresh_listed_group_lifecycle(&payload.items).await? {
            payload = self
                .collaboration_repo
                .list_user_groups(
                    &normalized_user_id,
                    normalized_status,
                    limit.clamp(1, 200),
                    offset.max(0),
                )
                .await?;
        }

        payload.unread_total = self.collaboration_repo.count_total_unread(&normalized_user_id).await?;
        Ok(payload)
    }

    pub async fn build_initial_stream_payload(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<serde_json::Value, DispatchChatError> {
        let groups = self.list_user_groups(user_id, "all", limit.clamp(1, 200), 0).await?;
        Ok(json!({
            "type": "dispatch_chat_initial",
            "items": groups.items,
            "unread_total": groups.unread_total,
            "total": groups.total,
            "timestamp": Utc::now().to_rfc3339(),
        }))
    }

    pub async fn get_group_for_user_by_flight(
        &self,
        flight_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DispatchChatError> {
        self.open_group_for_user_by_flight(flight_id, user_id, false).await
    }

    /// Open (and if needed sync/create) the system flight chat for a user.
    ///
    /// - Always re-syncs group membership from active dispatch orders first.
    /// - When `force_join` is true (e.g. system admin / ops), ensure the group
    ///   exists and the user is an active dispatcher member so they can open
    ///   the chat even without a current assignment.
    pub async fn open_group_for_user_by_flight(
        &self,
        flight_id: &str,
        user_id: &str,
        force_join: bool,
    ) -> Result<Option<DispatchChatGroupSummary>, DispatchChatError> {
        let normalized_flight_id = normalize(flight_id);
        let normalized_user_id = normalize(user_id);
        if normalized_flight_id.is_empty() || normalized_user_id.is_empty() {
            return Ok(None);
        }

        // Prefer full membership sync over lifecycle-only refresh so assignees/dispatchers
        // (and recovered empty member tables) are available before membership lookup.
        let _ = self.sync_group_for_flight_id(&normalized_flight_id).await?;

        if let Some(group) = self
            .collaboration_repo
            .get_group_for_user_by_flight(&normalized_flight_id, &normalized_user_id)
            .await?
        {
            return Ok(Some(group));
        }

        if !force_join {
            return Ok(None);
        }

        let group = match self
            .collaboration_repo
            .get_group_by_flight(&normalized_flight_id)
            .await?
        {
            Some(existing) => existing,
            None => {
                let flight = self.load_flight(&normalized_flight_id).await?;
                let archive_at = flight.as_ref().and_then(resolve_archive_at);
                let group_name = build_group_name(&normalized_flight_id, flight.as_ref());
                let metadata = json!({
                    "source": "force_join_open",
                    "force_joined_user_id": normalized_user_id,
                });
                self.collaboration_repo
                    .upsert_group_for_flight(&normalized_flight_id, &group_name, archive_at, &metadata)
                    .await?
            }
        };

        let latest_seq = self.collaboration_repo.get_group_latest_seq(&group.group_id).await?;
        self.collaboration_repo
            .upsert_group_memberships(
                &group.group_id,
                &[DispatchChatMemberUpsert {
                    user_id: normalized_user_id.clone(),
                    is_assignee: false,
                    is_dispatcher: true,
                    last_read_seq: latest_seq,
                    last_read_at: Some(Utc::now()),
                }],
            )
            .await?;

        Ok(self
            .collaboration_repo
            .get_group_for_user_by_flight(&normalized_flight_id, &normalized_user_id)
            .await?)
    }

    pub async fn list_group_messages(
        &self,
        group_id: &str,
        user_id: &str,
        limit: i64,
        before_seq: Option<i64>,
        after_seq: Option<i64>,
    ) -> Result<fms_domain::models::dispatch_collaboration::DispatchChatMessageList, DispatchChatError> {
        let normalized_group_id = normalize(group_id);
        let normalized_user_id = normalize(user_id);
        if normalized_group_id.is_empty() || normalized_user_id.is_empty() {
            return Err(DispatchChatError::Forbidden("群聊访问被拒绝".into()));
        }
        if matches!(before_seq, Some(value) if value <= 0) {
            return Err(DispatchChatError::Validation(
                "before_seq must be greater than 0".into(),
            ));
        }
        if matches!(after_seq, Some(value) if value < 0) {
            return Err(DispatchChatError::Validation("after_seq must not be negative".into()));
        }
        if before_seq.is_some() && after_seq.is_some() {
            return Err(DispatchChatError::Validation(
                "before_seq and after_seq are mutually exclusive".into(),
            ));
        }

        let cursor = match (before_seq, after_seq) {
            (Some(seq), _) => DispatchChatMessageCursor::Before(seq),
            (None, Some(seq)) => DispatchChatMessageCursor::After(seq),
            (None, None) => DispatchChatMessageCursor::Latest,
        };

        let Some(group) = self
            .collaboration_repo
            .get_group_for_user(&normalized_group_id, &normalized_user_id)
            .await?
        else {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        };

        let mut payload = self
            .collaboration_repo
            .list_group_messages(&normalized_group_id, limit.clamp(1, 200), cursor)
            .await?;
        for item in &mut payload.items {
            if item.mention_user_ids.is_empty() {
                item.mention_user_ids = DispatchChatMessage::mention_user_ids_from_metadata(&item.metadata);
            }
        }
        payload.limit = limit.clamp(1, 200);
        if !group.member_is_active && payload.items.is_empty() {
            payload.has_more = false;
        }
        Ok(payload)
    }

    pub async fn list_group_members(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<serde_json::Value, DispatchChatError> {
        let normalized_group_id = normalize(group_id);
        let normalized_user_id = normalize(user_id);
        if normalized_group_id.is_empty() || normalized_user_id.is_empty() {
            return Err(DispatchChatError::Forbidden("群聊访问被拒绝".into()));
        }

        if self
            .collaboration_repo
            .get_group_for_user(&normalized_group_id, &normalized_user_id)
            .await?
            .is_none()
        {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        }

        let items: Vec<serde_json::Value> = self
            .collaboration_repo
            .find_group_members(&normalized_group_id)
            .await?
            .into_iter()
            .map(|member| {
                json!({
                    "user_id": member.user_id.trim(),
                    "username": member.username.as_deref().unwrap_or("").trim(),
                    "is_assignee": member.is_assignee,
                    "is_dispatcher": member.is_dispatcher,
                    "is_active": member.is_active,
                })
            })
            .collect();
        Ok(json!({ "items": items }))
    }

    pub async fn send_message(
        &self,
        group_id: &str,
        user_id: &str,
        content: &str,
        at_all: bool,
        client_msg_id: Option<&str>,
        mention_user_ids: &[String],
    ) -> Result<DispatchChatSendOutcome, DispatchChatError> {
        let normalized_group_id = normalize(group_id);
        let normalized_user_id = normalize(user_id);
        let normalized_content = normalize(content);
        let normalized_client_msg_id = client_msg_id.map(normalize).filter(|value| !value.is_empty());

        if normalized_group_id.is_empty() || normalized_user_id.is_empty() {
            return Err(DispatchChatError::Forbidden("群聊访问被拒绝".into()));
        }
        if normalized_content.is_empty() || normalized_content.chars().count() > 2000 {
            return Err(DispatchChatError::Validation("消息内容长度应在 1~2000 字符".into()));
        }
        if let Some(client_msg_id) = normalized_client_msg_id.as_deref() {
            if client_msg_id.chars().count() > 64 {
                return Err(DispatchChatError::Validation(
                    "client_msg_id 长度不应超过 64 字符".into(),
                ));
            }
        }

        // Resolve a retry before any lifecycle side effects: a duplicate send
        // must be a pure read, not a second round of group refresh + fan-out.
        if let Some(client_msg_id) = normalized_client_msg_id.as_deref() {
            if let Some(existing) = self
                .collaboration_repo
                .find_message_by_client_id(&normalized_group_id, client_msg_id)
                .await?
            {
                return Ok(DispatchChatSendOutcome {
                    message: existing,
                    deduplicated: true,
                });
            }
        }

        if let Some(change) = self.refresh_group_lifecycle_for_group_id(&normalized_group_id).await? {
            self.emit_lifecycle_change(&change).await?;
        }

        let Some(group) = self
            .collaboration_repo
            .get_group_for_user(&normalized_group_id, &normalized_user_id)
            .await?
        else {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        };

        if !group.member_is_active {
            return Err(DispatchChatError::Archived(
                "当前用户已转为只读成员，无法发送消息".into(),
            ));
        }
        if group.read_only || group.status.eq_ignore_ascii_case("archived") {
            return Err(DispatchChatError::Archived("群已归档，当前只读".into()));
        }

        let members = self.collaboration_repo.find_group_members(&normalized_group_id).await?;
        let (is_at_all, mention_ids) = resolve_mentions(
            mention_user_ids,
            at_all,
            &normalized_content,
            &normalized_user_id,
            &members,
        );

        let message_id = ulid::Ulid::new().to_string();
        let event_id = ulid::Ulid::new().to_string();
        let mut message = self
            .collaboration_repo
            .insert_message(&NewDispatchChatMessage {
                message_id: message_id.clone(),
                group_id: normalized_group_id.clone(),
                sender_user_id: Some(normalized_user_id.clone()),
                dispatch_order_id: None,
                event_id: Some(event_id.clone()),
                message_type: "text".to_string(),
                content: normalized_content.clone(),
                is_at_all,
                metadata: json!({ "mention_user_ids": mention_ids }),
                client_msg_id: normalized_client_msg_id.clone(),
            })
            .await?;

        // The pre-check above can lose a race with a concurrent retry. The
        // insert then resolves to the row that retry stored, recognisable
        // because it carries a different message id than the one generated here.
        if message.message_id != message_id {
            return Ok(DispatchChatSendOutcome {
                message,
                deduplicated: true,
            });
        }
        message.mention_user_ids = mention_ids;

        if message.seq_no > 0 {
            let _ = self
                .collaboration_repo
                .mark_group_read(&normalized_group_id, &normalized_user_id, message.seq_no)
                .await?;
        }

        if !group.flight_id.trim().is_empty() {
            self.record_message_event(
                &group.flight_id,
                None,
                &normalized_group_id,
                &message,
                Some(ulid::Ulid::new().to_string()),
                Some(event_id.clone()),
                Some(normalized_user_id.clone()),
            )
            .await?;
            let _ = self
                .collaboration_repo
                .update_message_event_id(&message.message_id, &event_id)
                .await?;
            message.event_id = Some(event_id);
        }

        self.notify_mentioned_members_best_effort(
            &group,
            &normalized_group_id,
            &normalized_user_id,
            &message,
            is_at_all,
            &members,
        )
        .await;

        Ok(DispatchChatSendOutcome {
            message,
            deduplicated: false,
        })
    }

    pub async fn build_message_stream_events(
        &self,
        group_id: &str,
        message: &DispatchChatMessage,
    ) -> Result<Vec<(String, serde_json::Value)>, DispatchChatError> {
        let Some(group) = self.collaboration_repo.get_group_by_id(group_id).await? else {
            return Ok(Vec::new());
        };
        // One batched query for every member's badge numbers; the per-member
        // form costs 2 round trips × member count on every single message.
        let member_unread = self.collaboration_repo.count_unread_for_group_members(group_id).await?;
        let timestamp = Utc::now().to_rfc3339();
        let mut events = Vec::new();
        for entry in member_unread {
            let user_id = normalize(&entry.user_id);
            if user_id.is_empty() {
                continue;
            }
            events.push((
                user_id,
                json!({
                    "type": "dispatch_chat_message",
                    "group_id": group_id,
                    "flight_id": group.flight_id,
                    "message": message,
                    "unread_count": entry.unread_count,
                    "unread_total": entry.unread_total,
                    "timestamp": timestamp,
                }),
            ));
        }
        Ok(events)
    }

    pub async fn build_group_upserted_stream_events(
        &self,
        group_id: &str,
    ) -> Result<Vec<(String, serde_json::Value)>, DispatchChatError> {
        let member_unread = self.collaboration_repo.count_unread_for_group_members(group_id).await?;
        let timestamp = Utc::now().to_rfc3339();
        let mut events = Vec::new();
        for entry in member_unread {
            let user_id = normalize(&entry.user_id);
            if user_id.is_empty() {
                continue;
            }
            let Some(group) = self.collaboration_repo.get_group_for_user(group_id, &user_id).await? else {
                continue;
            };
            events.push((
                user_id,
                json!({
                    "type": "dispatch_chat_group_upserted",
                    "group": group,
                    "timestamp": timestamp,
                }),
            ));
        }
        Ok(events)
    }

    pub async fn build_group_archived_stream_events(
        &self,
        group_id: &str,
        archived_at: DateTime<Utc>,
    ) -> Result<Vec<(String, serde_json::Value)>, DispatchChatError> {
        let member_unread = self.collaboration_repo.count_unread_for_group_members(group_id).await?;
        let timestamp = Utc::now().to_rfc3339();
        let mut events = Vec::new();
        for entry in member_unread {
            let user_id = normalize(&entry.user_id);
            if user_id.is_empty() {
                continue;
            }
            events.push((
                user_id,
                json!({
                    "type": "dispatch_chat_group_archived",
                    "group_id": group_id,
                    "archived_at": archived_at.to_rfc3339(),
                    "timestamp": timestamp,
                }),
            ));
        }
        Ok(events)
    }

    pub async fn mark_group_read(
        &self,
        group_id: &str,
        user_id: &str,
        read_seq: Option<i64>,
    ) -> Result<DispatchChatReadOutcome, DispatchChatError> {
        let normalized_group_id = normalize(group_id);
        let normalized_user_id = normalize(user_id);
        if normalized_group_id.is_empty() || normalized_user_id.is_empty() {
            return Err(DispatchChatError::Forbidden("群聊访问被拒绝".into()));
        }

        let Some(group) = self
            .collaboration_repo
            .get_group_for_user(&normalized_group_id, &normalized_user_id)
            .await?
        else {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        };

        let latest_seq = self
            .collaboration_repo
            .get_group_latest_seq(&normalized_group_id)
            .await?;
        let target_seq = read_seq.unwrap_or(latest_seq).clamp(0, latest_seq);

        let updated = self
            .collaboration_repo
            .mark_group_read(&normalized_group_id, &normalized_user_id, target_seq)
            .await?;
        let Some(cursor_update) = updated else {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        };
        let advanced = cursor_update.advanced();
        let updated_member = cursor_update.member;
        // The cursor never moves backwards, so report where it actually landed
        // rather than what was asked for.
        let effective_seq = updated_member.last_read_seq;

        let unread_count = self
            .collaboration_repo
            .count_group_unread(&normalized_group_id, &normalized_user_id)
            .await?;
        let unread_total = self.collaboration_repo.count_total_unread(&normalized_user_id).await?;

        // Read receipts are the highest-frequency write on this table and the
        // client re-marks on every focus/scroll. Only a cursor that actually
        // moved is worth an append to the audit ledger.
        if advanced && !group.flight_id.trim().is_empty() {
            let related_dispatch_order_ids = extract_related_dispatch_order_ids(&group.metadata);
            let dispatch_order_id = related_dispatch_order_ids.first().cloned();
            self.collaboration_repo
                .create_event(&DispatchCollaborationEvent {
                    event_id: ulid::Ulid::new().to_string(),
                    flight_id: group.flight_id.clone(),
                    dispatch_order_id,
                    group_id: Some(normalized_group_id.clone()),
                    event_type: "group_read_synced".to_string(),
                    actor_user_id: Some(normalized_user_id.clone()),
                    actor_username: updated_member.username.clone(),
                    correlation_id: Some(ulid::Ulid::new().to_string()),
                    payload: json!({
                        "group_id": normalized_group_id.clone(),
                        "flight_id": group.flight_id.clone(),
                        "member_id": updated_member.id.clone(),
                        "last_read_seq": effective_seq,
                        "unread_count": unread_count,
                        "unread_total": unread_total,
                        "related_dispatch_order_ids": related_dispatch_order_ids,
                    }),
                    occurred_at: updated_member.last_read_at.unwrap_or_else(Utc::now),
                    source_table: Some("dispatch_chat_group_members".to_string()),
                    source_record_id: Some(updated_member.id.clone()),
                })
                .await?;
        }

        Ok(DispatchChatReadOutcome {
            payload: json!({
                "group_id": normalized_group_id,
                "last_read_seq": effective_seq,
                "unread_count": unread_count,
                "unread_total": unread_total,
            }),
            last_read_seq: effective_seq,
            advanced,
        })
    }

    pub async fn build_read_synced_stream_event(
        &self,
        group_id: &str,
        user_id: &str,
        last_read_seq: i64,
    ) -> Result<serde_json::Value, DispatchChatError> {
        let unread_count = self.collaboration_repo.count_group_unread(group_id, user_id).await?;
        let unread_total = self.collaboration_repo.count_total_unread(user_id).await?;
        Ok(json!({
            "type": "dispatch_chat_read_synced",
            "group_id": group_id,
            "last_read_seq": last_read_seq,
            "unread_count": unread_count,
            "unread_total": unread_total,
            "timestamp": Utc::now().to_rfc3339(),
        }))
    }

    async fn publish_stream_events(&self, event_name: &str, events: Vec<(String, serde_json::Value)>) {
        let Some(event_publisher) = self.event_publisher.as_ref() else {
            return;
        };
        if events.is_empty() {
            return;
        }
        event_publisher.publish_user_event(event_name, events).await;
    }

    async fn emit_chat_message_event(
        &self,
        group_id: &str,
        message: &DispatchChatMessage,
    ) -> Result<(), DispatchChatError> {
        let events = self.build_message_stream_events(group_id, message).await?;
        self.publish_stream_events("chat_message", events).await;
        Ok(())
    }

    async fn emit_group_upserted_event(&self, group_id: &str) -> Result<(), DispatchChatError> {
        let events = self.build_group_upserted_stream_events(group_id).await?;
        self.publish_stream_events("chat_group_upserted", events).await;
        Ok(())
    }

    async fn emit_group_archived_event(
        &self,
        group_id: &str,
        archived_at: DateTime<Utc>,
    ) -> Result<(), DispatchChatError> {
        let events = self.build_group_archived_stream_events(group_id, archived_at).await?;
        self.publish_stream_events("chat_group_archived", events).await;
        Ok(())
    }

    async fn emit_lifecycle_change(&self, change: &DispatchChatLifecycleChange) -> Result<(), DispatchChatError> {
        match change {
            DispatchChatLifecycleChange::Upserted { group_id } => self.emit_group_upserted_event(group_id).await,
            DispatchChatLifecycleChange::Archived { group_id, archived_at } => {
                self.emit_group_archived_event(group_id, archived_at.to_owned()).await
            }
        }
    }

    pub async fn refresh_group_lifecycle_for_flight(
        &self,
        flight_id: &str,
    ) -> Result<Option<DispatchChatLifecycleChange>, DispatchChatError> {
        let normalized_flight_id = normalize(flight_id);
        if normalized_flight_id.is_empty() {
            return Ok(None);
        }

        let Some(group) = self
            .collaboration_repo
            .get_group_by_flight(&normalized_flight_id)
            .await?
        else {
            return Ok(None);
        };
        if group.status.eq_ignore_ascii_case("archived") {
            return Ok(None);
        }

        if let Some(archive_at) = group.archive_at.filter(|value| *value <= Utc::now()) {
            return self.archive_group(&group, archive_at).await;
        }

        let flight = self.load_flight(&normalized_flight_id).await?;
        if group.deprecated
            && group.deprecation_reason.as_deref() == Some(DEPRECATION_REASON_ARRIVAL_GUARANTEE_COMPLETED)
        {
            let is_reopened_arrival = is_arrival_flight(flight.as_ref())
                && !self.all_orders_terminal_for_flight(&normalized_flight_id).await?;
            if is_reopened_arrival
                && self
                    .collaboration_repo
                    .clear_group_deprecation(&group.group_id, DEPRECATION_REASON_ARRIVAL_GUARANTEE_COMPLETED)
                    .await?
                    .is_some()
            {
                return Ok(Some(DispatchChatLifecycleChange::Upserted {
                    group_id: group.group_id,
                }));
            }
        }

        if group.deprecated {
            return Ok(None);
        }

        let Some(reason) = self
            .resolve_group_deprecation_reason(&normalized_flight_id, flight.as_ref())
            .await?
        else {
            return Ok(None);
        };

        if self.mark_group_deprecated(&group, &reason).await? {
            return Ok(Some(DispatchChatLifecycleChange::Upserted {
                group_id: group.group_id,
            }));
        }
        Ok(None)
    }

    pub async fn deprecate_due_groups_once(&self, limit: i64) -> Result<serde_json::Value, DispatchChatError> {
        let candidate_groups = self
            .collaboration_repo
            .find_groups_pending_deprecation(limit.clamp(1, 500))
            .await?;
        let mut due_count = 0;
        let mut deprecated_count = 0;

        for group in candidate_groups {
            let flight = self.load_flight(&group.flight_id).await?;
            let Some(reason) = self
                .resolve_group_deprecation_reason(&group.flight_id, flight.as_ref())
                .await?
            else {
                continue;
            };
            due_count += 1;
            if self.mark_group_deprecated(&group, &reason).await? {
                deprecated_count += 1;
                self.emit_group_upserted_event(&group.group_id).await?;
            }
        }

        Ok(json!({
            "due_count": due_count,
            "deprecated_count": deprecated_count,
            "failed_count": (due_count - deprecated_count).max(0),
        }))
    }

    pub async fn archive_due_groups_once(
        &self,
        limit: i64,
    ) -> Result<(serde_json::Value, Vec<DispatchChatLifecycleChange>), DispatchChatError> {
        let due_groups = self
            .collaboration_repo
            .find_due_archive_groups(limit.clamp(1, 500))
            .await?;
        let mut archived_count = 0;
        let mut changes = Vec::new();

        for group in due_groups.iter() {
            let archived_at = group.archive_at.unwrap_or_else(Utc::now);
            if let Some(change) = self.archive_group(group, archived_at).await? {
                archived_count += 1;
                self.emit_lifecycle_change(&change).await?;
                changes.push(change);
            }
        }

        Ok((
            json!({
                "due_count": due_groups.len(),
                "archived_count": archived_count,
                "failed_count": (due_groups.len() as i64 - archived_count).max(0),
            }),
            changes,
        ))
    }

    async fn refresh_listed_group_lifecycle(
        &self,
        groups: &[DispatchChatGroupSummary],
    ) -> Result<bool, DispatchChatError> {
        let mut changed = false;
        let mut flight_ids = HashSet::new();
        for group in groups {
            let flight_id = normalize(&group.flight_id);
            if !flight_id.is_empty() {
                flight_ids.insert(flight_id);
            }
        }
        for flight_id in flight_ids {
            if self.refresh_group_lifecycle_for_flight(&flight_id).await?.is_some() {
                changed = true;
            }
        }
        Ok(changed)
    }

    async fn refresh_group_lifecycle_for_group_id(
        &self,
        group_id: &str,
    ) -> Result<Option<DispatchChatLifecycleChange>, DispatchChatError> {
        let Some(group) = self.collaboration_repo.get_group_by_id(group_id).await? else {
            return Ok(None);
        };
        self.refresh_group_lifecycle_for_flight(&group.flight_id).await
    }

    async fn load_flight(&self, flight_id: &str) -> Result<Option<Flight>, DispatchChatError> {
        let Some(flight_repo) = self.flight_repo.as_ref() else {
            return Ok(None);
        };
        Ok(flight_repo.find_by_id(flight_id).await?)
    }

    async fn all_orders_terminal_for_flight(&self, flight_id: &str) -> Result<bool, DispatchChatError> {
        let Some(dispatch_order_repo) = self.dispatch_order_repo.as_ref() else {
            return Ok(false);
        };
        let orders = dispatch_order_repo
            .find_by_flight(flight_id)
            .await?
            .into_iter()
            .filter(is_dispatch_order_chat_relevant)
            .collect::<Vec<_>>();
        if orders.is_empty() {
            return Ok(false);
        }
        Ok(orders.iter().all(is_dispatch_order_terminal))
    }

    async fn resolve_group_deprecation_reason(
        &self,
        flight_id: &str,
        flight: Option<&Flight>,
    ) -> Result<Option<String>, DispatchChatError> {
        if is_arrival_flight(flight) && self.all_orders_terminal_for_flight(flight_id).await? {
            return Ok(Some(DEPRECATION_REASON_ARRIVAL_GUARANTEE_COMPLETED.to_string()));
        }
        if is_departure_flight(flight) && flight.and_then(|value| value.actual_departure).is_some() {
            return Ok(Some(DEPRECATION_REASON_DEPARTURE_DEPARTED.to_string()));
        }
        if is_transit_flight(flight) && flight.and_then(|value| value.actual_departure).is_some() {
            return Ok(Some(DEPRECATION_REASON_TRANSIT_DEPARTED.to_string()));
        }
        Ok(None)
    }

    async fn mark_group_deprecated(
        &self,
        group: &DispatchChatGroupSummary,
        reason: &str,
    ) -> Result<bool, DispatchChatError> {
        let Some(updated_group) = self
            .collaboration_repo
            .mark_group_deprecated(&group.group_id, reason)
            .await?
        else {
            return Ok(false);
        };

        let event_id = ulid::Ulid::new().to_string();
        let message = self
            .collaboration_repo
            .insert_message(&NewDispatchChatMessage {
                message_id: ulid::Ulid::new().to_string(),
                group_id: group.group_id.clone(),
                sender_user_id: None,
                dispatch_order_id: None,
                event_id: Some(event_id.clone()),
                message_type: "system".to_string(),
                content: build_deprecation_message(reason),
                is_at_all: false,
                metadata: json!({ "reason": reason }),
                client_msg_id: None,
            })
            .await?;
        self.collaboration_repo
            .create_event(&DispatchCollaborationEvent {
                event_id: ulid::Ulid::new().to_string(),
                flight_id: updated_group.flight_id.clone(),
                dispatch_order_id: None,
                group_id: Some(updated_group.group_id.clone()),
                event_type: "group_deprecated".to_string(),
                actor_user_id: None,
                actor_username: None,
                correlation_id: Some(ulid::Ulid::new().to_string()),
                payload: json!({
                    "deprecated_at": updated_group.deprecated_at.map(|value| value.to_rfc3339()),
                    "reason": reason,
                }),
                occurred_at: updated_group.deprecated_at.unwrap_or_else(Utc::now),
                source_table: Some("dispatch_chat_groups".to_string()),
                source_record_id: Some(updated_group.group_id.clone()),
            })
            .await?;
        self.record_message_event(
            &updated_group.flight_id,
            None,
            &updated_group.group_id,
            &message,
            Some(ulid::Ulid::new().to_string()),
            Some(event_id),
            None,
        )
        .await?;
        self.emit_chat_message_event(&updated_group.group_id, &message).await?;
        Ok(true)
    }

    async fn archive_group(
        &self,
        group: &DispatchChatGroupSummary,
        archived_at: DateTime<Utc>,
    ) -> Result<Option<DispatchChatLifecycleChange>, DispatchChatError> {
        let archived_groups = self
            .collaboration_repo
            .archive_groups_batch(std::slice::from_ref(&group.group_id))
            .await?;
        let Some(updated_group) = archived_groups.into_iter().next() else {
            return Ok(None);
        };

        let event_id = ulid::Ulid::new().to_string();
        let message = self
            .collaboration_repo
            .insert_message(&NewDispatchChatMessage {
                message_id: ulid::Ulid::new().to_string(),
                group_id: updated_group.group_id.clone(),
                sender_user_id: None,
                dispatch_order_id: None,
                event_id: Some(event_id.clone()),
                message_type: "system".to_string(),
                content: "系统消息：航班起飞超过 6 小时，群组已归档并切换为只读。".to_string(),
                is_at_all: false,
                metadata: json!({ "reason": "flight_departed_6h" }),
                client_msg_id: None,
            })
            .await?;
        self.collaboration_repo
            .create_event(&DispatchCollaborationEvent {
                event_id: ulid::Ulid::new().to_string(),
                flight_id: updated_group.flight_id.clone(),
                dispatch_order_id: None,
                group_id: Some(updated_group.group_id.clone()),
                event_type: "group_archived".to_string(),
                actor_user_id: None,
                actor_username: None,
                correlation_id: Some(ulid::Ulid::new().to_string()),
                payload: json!({
                    "archived_at": archived_at.to_rfc3339(),
                    "reason": "flight_departed_6h",
                }),
                occurred_at: archived_at,
                source_table: Some("dispatch_chat_groups".to_string()),
                source_record_id: Some(updated_group.group_id.clone()),
            })
            .await?;
        self.record_message_event(
            &updated_group.flight_id,
            None,
            &updated_group.group_id,
            &message,
            Some(ulid::Ulid::new().to_string()),
            Some(event_id),
            None,
        )
        .await?;
        self.emit_chat_message_event(&updated_group.group_id, &message).await?;

        Ok(Some(DispatchChatLifecycleChange::Archived {
            group_id: updated_group.group_id,
            archived_at,
        }))
    }

    async fn notify_mentioned_members_best_effort(
        &self,
        group: &DispatchChatGroupSummary,
        group_id: &str,
        sender_user_id: &str,
        message: &DispatchChatMessage,
        is_at_all: bool,
        members: &[DispatchChatMember],
    ) {
        let Some(notifier) = self.mention_notifier.as_ref() else {
            return;
        };

        let recipients = if is_at_all {
            let mut ids: Vec<String> = members
                .iter()
                .map(|member| normalize(&member.user_id))
                .filter(|id| !id.is_empty() && id != sender_user_id)
                .collect();
            ids.sort();
            ids.dedup();
            ids
        } else {
            message.mention_user_ids.clone()
        };
        if recipients.is_empty() {
            return;
        }

        let title = if is_at_all {
            format!("{} 群聊有人@全体", group.group_name)
        } else {
            format!("{} 群聊有人@你", group.group_name)
        };
        let sender_label = message
            .sender_username
            .as_deref()
            .map(normalize)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| sender_user_id.to_string());
        let preview: String = message.content.chars().take(200).collect();
        let flight_id = {
            let trimmed = group.flight_id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        };

        if let Err(error) = notifier
            .notify_chat_mentions(DispatchBatchNotificationCreate {
                user_ids: recipients,
                title,
                body: format!("{sender_label}: {preview}"),
                category: "dispatch_chat_mention".to_string(),
                severity: if is_at_all {
                    "warning".to_string()
                } else {
                    "info".to_string()
                },
                flight_id,
                related_entity_type: Some("dispatch_chat_group".to_string()),
                related_entity_id: Some(group_id.to_string()),
                dispatch_order_id: None,
                group_id: Some(group_id.to_string()),
                sender_user_id: Some(sender_user_id.to_string()),
                sender_username_snapshot: message.sender_username.clone(),
                origin_type: "dispatch_chat".to_string(),
                receipt_required: false,
            })
            .await
        {
            warn!(
                group_id = %group_id,
                error = %error,
                "failed to send dispatch chat mention notifications"
            );
        }
    }

    async fn record_message_event(
        &self,
        flight_id: &str,
        dispatch_order_id: Option<&str>,
        group_id: &str,
        message: &DispatchChatMessage,
        correlation_id: Option<String>,
        event_id: Option<String>,
        actor_user_id: Option<String>,
    ) -> Result<(), DispatchChatError> {
        if flight_id.trim().is_empty() {
            return Ok(());
        }

        let recorded_event_id = event_id.unwrap_or_else(|| ulid::Ulid::new().to_string());
        self.collaboration_repo
            .create_event(&DispatchCollaborationEvent {
                event_id: recorded_event_id.clone(),
                flight_id: flight_id.to_string(),
                dispatch_order_id: dispatch_order_id.map(str::to_string),
                group_id: Some(group_id.to_string()),
                event_type: "message_sent".to_string(),
                actor_user_id,
                actor_username: message.sender_username.clone(),
                correlation_id,
                payload: json!({
                    "message_id": message.message_id,
                    "message_type": message.message_type,
                    "content": message.content,
                    "is_at_all": message.is_at_all,
                    "seq_no": message.seq_no,
                    "mention_user_ids": message.mention_user_ids,
                }),
                occurred_at: message.sent_at,
                source_table: Some("dispatch_chat_messages".to_string()),
                source_record_id: Some(message.message_id.clone()),
            })
            .await?;
        let _ = self
            .collaboration_repo
            .update_message_event_id(&message.message_id, &recorded_event_id)
            .await?;
        Ok(())
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_string()
}

fn parse_status(status: &str) -> Result<&'static str, DispatchChatError> {
    match status.trim().to_ascii_lowercase().as_str() {
        "active" => Ok("active"),
        "archived" => Ok("archived"),
        "all" | "" => Ok("all"),
        _ => Err(DispatchChatError::Validation(
            "status must be active|archived|all".into(),
        )),
    }
}

fn contains_at_all(content: &str) -> bool {
    let normalized = content.to_ascii_lowercase();
    normalized.contains("@all") || content.contains("@全体")
}

fn resolve_mentions(
    requested: &[String],
    at_all_flag: bool,
    content: &str,
    sender_user_id: &str,
    members: &[DispatchChatMember],
) -> (bool, Vec<String>) {
    let is_at_all = at_all_flag || contains_at_all(content);
    let member_ids: HashSet<String> = members
        .iter()
        .map(|m| normalize(&m.user_id))
        .filter(|id| !id.is_empty())
        .collect();
    let mut ids: Vec<String> = requested
        .iter()
        .map(|id| normalize(id))
        .filter(|id| !id.is_empty() && id != sender_user_id && member_ids.contains(id))
        .collect();
    ids.sort();
    ids.dedup();
    ids.truncate(50);
    (is_at_all, ids)
}

fn extract_related_dispatch_order_ids(metadata: &serde_json::Value) -> Vec<String> {
    let mut order_ids = metadata
        .get("related_dispatch_order_ids")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(normalize)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    if order_ids.is_empty() {
        if let Some(dispatch_order_id) = metadata
            .get("dispatch_order_id")
            .and_then(serde_json::Value::as_str)
            .map(normalize)
            .filter(|value| !value.is_empty())
        {
            order_ids.push(dispatch_order_id);
        }
    }

    order_ids
}

fn resolve_archive_anchor(
    inbound: bool,
    outbound: bool,
    actual_departure: Option<DateTime<Utc>>,
    actual_arrival: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let arrival_only = inbound && !outbound;
    if arrival_only {
        actual_arrival.or(actual_departure)
    } else {
        actual_departure
    }
}

fn resolve_archive_at(flight: &Flight) -> Option<DateTime<Utc>> {
    resolve_archive_anchor(
        flight.is_arrival_flight(),
        flight.is_departure_flight(),
        flight.actual_departure,
        flight.actual_arrival,
    )
    .map(|value| value + Duration::hours(6))
}

fn build_group_name(flight_id: &str, flight: Option<&Flight>) -> String {
    let flight_no = flight
        .and_then(|value| value.get_flight_numbers().into_iter().next())
        .map(|value| normalize(&value))
        .filter(|value| !value.is_empty());
    match flight_no {
        Some(flight_no) => format!("{} 保障协同群", flight_no),
        None => format!("航班 {} 保障协同群", flight_id),
    }
}

fn build_group_metadata(
    flight: Option<&Flight>,
    related_orders: &[&DispatchOrder],
    active_orders: &[&DispatchOrder],
    related_departments: &[String],
) -> serde_json::Value {
    let numbers = flight.map(Flight::get_flight_numbers).unwrap_or_default();
    json!({
        "flight_numbers": numbers,
        "related_departments": related_departments,
        "active_dispatch_order_ids": active_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
        "related_dispatch_order_ids": related_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
        "source": "dispatch_chat_v2",
    })
}

fn build_group_memberships(
    assignee_user_ids: &[String],
    dispatcher_candidates: &[fms_domain::models::dispatch_collaboration::DispatchChatDispatcherCandidate],
    retained_dispatcher_user_ids: &[String],
    existing_read_seq_map: &HashMap<String, i64>,
    latest_seq: i64,
) -> Vec<DispatchChatMemberUpsert> {
    let mut membership_map = HashMap::<String, (bool, bool)>::new();
    for user_id in assignee_user_ids {
        membership_map.insert(user_id.clone(), (true, false));
    }
    for candidate in dispatcher_candidates {
        let user_id = normalize(&candidate.user_id);
        if user_id.is_empty() {
            continue;
        }
        membership_map
            .entry(user_id)
            .and_modify(|flags| flags.1 = true)
            .or_insert((false, true));
    }
    for user_id in retained_dispatcher_user_ids {
        let user_id = normalize(user_id);
        if user_id.is_empty() {
            continue;
        }
        membership_map
            .entry(user_id)
            .and_modify(|flags| flags.1 = true)
            .or_insert((false, true));
    }

    let now = Utc::now();
    membership_map
        .into_iter()
        .map(|(user_id, (is_assignee, is_dispatcher))| DispatchChatMemberUpsert {
            last_read_seq: existing_read_seq_map.get(&user_id).copied().unwrap_or(latest_seq),
            last_read_at: if existing_read_seq_map.contains_key(&user_id) {
                None
            } else {
                Some(now)
            },
            user_id,
            is_assignee,
            is_dispatcher,
        })
        .collect()
}

fn collect_related_departments(
    dispatch_orders: &[&DispatchOrder],
    assignee_profiles: &[DispatchChatUserProfile],
) -> Vec<String> {
    let mut departments = Vec::new();

    for dispatch_order in dispatch_orders {
        if let Some(target_department) = extract_target_department(dispatch_order) {
            push_unique(&mut departments, target_department);
        }
    }

    for profile in assignee_profiles {
        if let Some(department) = profile
            .department
            .as_deref()
            .map(normalize)
            .filter(|value| !value.is_empty())
        {
            push_unique(&mut departments, department);
        }
    }

    departments
}

fn extract_target_department(dispatch_order: &DispatchOrder) -> Option<String> {
    dispatch_order
        .workflow_context
        .get("target_department")
        .and_then(serde_json::Value::as_str)
        .map(normalize)
        .filter(|value| !value.is_empty())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn collect_assignee_user_ids_from_orders(dispatch_orders: &[&DispatchOrder]) -> Vec<String> {
    let mut user_ids = Vec::new();
    for dispatch_order in dispatch_orders {
        for user_id in collect_assignee_user_ids(dispatch_order) {
            if !user_ids.iter().any(|existing| existing == &user_id) {
                user_ids.push(user_id);
            }
        }
    }
    user_ids
}

fn collect_assignee_user_ids(dispatch_order: &DispatchOrder) -> Vec<String> {
    let mut user_ids = Vec::new();
    if let Some(user_id) = dispatch_order
        .individual_user_id
        .as_deref()
        .map(normalize)
        .filter(|value| !value.is_empty())
    {
        user_ids.push(user_id);
    }
    for member in dispatch_order.members.iter().filter(|member| member.is_active) {
        let user_id = normalize(&member.user_id);
        if !user_id.is_empty() && !user_ids.iter().any(|existing| existing == &user_id) {
            user_ids.push(user_id);
        }
    }
    user_ids
}

fn is_dispatch_order_terminal(dispatch_order: &DispatchOrder) -> bool {
    matches!(
        dispatch_order.status,
        DispatchOrderStatus::Completed | DispatchOrderStatus::Cancelled
    )
}

fn is_dispatch_order_chat_relevant(dispatch_order: &DispatchOrder) -> bool {
    !dispatch_order
        .publication_state
        .trim()
        .eq_ignore_ascii_case("prepublished")
}

fn is_arrival_flight(flight: Option<&Flight>) -> bool {
    matches!(flight, Some(flight) if flight.is_arrival_flight() && !flight.is_departure_flight())
}

fn is_departure_flight(flight: Option<&Flight>) -> bool {
    matches!(flight, Some(flight) if flight.is_departure_flight() && !flight.is_arrival_flight())
}

fn is_transit_flight(flight: Option<&Flight>) -> bool {
    matches!(flight, Some(flight) if flight.is_turnaround_flight())
}

fn build_deprecation_message(reason: &str) -> String {
    match reason {
        DEPRECATION_REASON_ARRIVAL_GUARANTEE_COMPLETED => {
            "系统消息：单进港航班保障已完成，群组已标记为弃用。".to_string()
        }
        DEPRECATION_REASON_DEPARTURE_DEPARTED => "系统消息：单出港航班已起飞，群组已标记为弃用。".to_string(),
        DEPRECATION_REASON_TRANSIT_DEPARTED => "系统消息：中转航班已起飞，群组已标记为弃用。".to_string(),
        _ => "系统消息：群组已标记为弃用。".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::models::dispatch_collaboration::{
        DispatchChatDispatcherCandidate, DispatchChatMember, DispatchChatMemberUnread, DispatchChatMessageList,
        DispatchChatReadCursorUpdate, NotificationReceiptSummary,
    };
    use fms_domain::models::notification::Notification;
    use std::sync::Mutex;

    const GROUP_ID: &str = "group-1";
    const FLIGHT_ID: &str = "flight-1";
    const USER_ID: &str = "user-1";

    /// Stores messages and the read cursor the way Postgres does, so idempotency
    /// and advance decisions in the service are exercised for real.
    #[derive(Default)]
    struct FakeChatRepo {
        messages: Mutex<Vec<DispatchChatMessage>>,
        events: Mutex<Vec<DispatchCollaborationEvent>>,
        last_read_seq: Mutex<i64>,
        mark_read_calls: Mutex<Vec<i64>>,
    }

    impl FakeChatRepo {
        fn seed_message(&self, sender: &str) {
            let mut messages = self.messages.lock().expect("lock messages");
            let seq_no = messages.len() as i64 + 1;
            messages.push(message_row(&format!("seed-{seq_no}"), seq_no, sender, None));
        }

        fn stored_count(&self) -> usize {
            self.messages.lock().expect("lock messages").len()
        }

        fn events_of_type(&self, event_type: &str) -> usize {
            self.events
                .lock()
                .expect("lock events")
                .iter()
                .filter(|event| event.event_type == event_type)
                .count()
        }

        fn mark_read_call_count(&self) -> usize {
            self.mark_read_calls.lock().expect("lock mark read calls").len()
        }
    }

    fn message_row(message_id: &str, seq_no: i64, sender: &str, client_msg_id: Option<&str>) -> DispatchChatMessage {
        DispatchChatMessage {
            message_id: message_id.to_string(),
            seq_no,
            group_id: GROUP_ID.to_string(),
            sender_user_id: Some(sender.to_string()),
            sender_username: None,
            message_type: "text".to_string(),
            content: "内容".to_string(),
            is_at_all: false,
            mention_user_ids: vec![],
            metadata: json!({}),
            sent_at: Utc::now(),
            client_msg_id: client_msg_id.map(str::to_string),
            dispatch_order_id: None,
            event_id: None,
        }
    }

    fn group_row() -> DispatchChatGroupSummary {
        DispatchChatGroupSummary {
            group_id: GROUP_ID.to_string(),
            channel_type: "system_flight_dispatch".to_string(),
            flight_id: FLIGHT_ID.to_string(),
            group_name: "保障群".to_string(),
            status: "active".to_string(),
            read_only: false,
            deprecated: false,
            deprecated_at: None,
            deprecation_reason: None,
            archive_at: None,
            archived_at: None,
            metadata: json!({}),
            member_count: 2,
            unread_count: 0,
            last_message_seq: None,
            last_message_preview: None,
            last_message_at: None,
            member_is_active: true,
        }
    }

    fn member_row(last_read_seq: i64) -> DispatchChatMember {
        member_row_for(USER_ID, last_read_seq)
    }

    fn member_row_for(user_id: &str, last_read_seq: i64) -> DispatchChatMember {
        DispatchChatMember {
            id: format!("member-{user_id}"),
            group_id: GROUP_ID.to_string(),
            user_id: user_id.to_string(),
            username: Some(user_id.to_string()),
            is_assignee: true,
            is_dispatcher: false,
            is_active: true,
            joined_at: Some(Utc::now()),
            left_at: None,
            last_read_seq,
            last_read_at: Some(Utc::now()),
        }
    }

    fn receipt_summary() -> NotificationReceiptSummary {
        NotificationReceiptSummary {
            total_count: 0,
            pending_count: 0,
            acknowledged_count: 0,
            rejected_count: 0,
            latest_updated_at: None,
            receipt_group_ids: vec![],
        }
    }

    #[async_trait::async_trait]
    impl DispatchCollaborationRepository for FakeChatRepo {
        async fn get_group_by_id(&self, _group_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
            Ok(Some(group_row()))
        }

        async fn get_group_for_user(
            &self,
            _group_id: &str,
            user_id: &str,
        ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
            if user_id == "stranger" {
                return Ok(None);
            }
            Ok(Some(group_row()))
        }

        async fn get_group_for_user_by_flight(
            &self,
            _flight_id: &str,
            _user_id: &str,
        ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
            Ok(Some(group_row()))
        }

        async fn get_group_by_flight(&self, _flight_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
            Ok(Some(group_row()))
        }

        async fn list_user_groups(
            &self,
            _user_id: &str,
            _status: &str,
            limit: i64,
            offset: i64,
        ) -> Result<DispatchChatGroupList, DomainError> {
            Ok(DispatchChatGroupList {
                items: vec![group_row()],
                total: 1,
                limit,
                offset,
                unread_total: 0,
            })
        }

        async fn list_group_messages(
            &self,
            _group_id: &str,
            limit: i64,
            cursor: DispatchChatMessageCursor,
        ) -> Result<DispatchChatMessageList, DomainError> {
            Ok(DispatchChatMessageList {
                items: vec![],
                total: 0,
                limit,
                before_seq: cursor.before_seq(),
                after_seq: cursor.after_seq(),
                has_more: false,
                next_before_seq: None,
                next_after_seq: None,
            })
        }

        async fn insert_message(&self, message: &NewDispatchChatMessage) -> Result<DispatchChatMessage, DomainError> {
            let mut messages = self.messages.lock().expect("lock messages");
            // Mirrors the partial unique index on (group_id, client_msg_id).
            if let Some(client_msg_id) = message.client_msg_id.as_deref() {
                if let Some(existing) = messages.iter().find(|stored| {
                    stored.group_id == message.group_id && stored.client_msg_id.as_deref() == Some(client_msg_id)
                }) {
                    return Ok(existing.clone());
                }
            }
            let seq_no = messages.len() as i64 + 1;
            let mut stored = message_row(
                &message.message_id,
                seq_no,
                message.sender_user_id.as_deref().unwrap_or_default(),
                message.client_msg_id.as_deref(),
            );
            stored.content = message.content.clone();
            stored.is_at_all = message.is_at_all;
            stored.metadata = message.metadata.clone();
            stored.mention_user_ids = DispatchChatMessage::mention_user_ids_from_metadata(&message.metadata);
            messages.push(stored.clone());
            Ok(stored)
        }

        async fn find_message_by_client_id(
            &self,
            group_id: &str,
            client_msg_id: &str,
        ) -> Result<Option<DispatchChatMessage>, DomainError> {
            Ok(self
                .messages
                .lock()
                .expect("lock messages")
                .iter()
                .find(|stored| stored.group_id == group_id && stored.client_msg_id.as_deref() == Some(client_msg_id))
                .cloned())
        }

        async fn update_message_event_id(
            &self,
            _message_id: &str,
            _event_id: &str,
        ) -> Result<Option<DispatchChatMessage>, DomainError> {
            Ok(None)
        }

        async fn mark_group_read(
            &self,
            _group_id: &str,
            _user_id: &str,
            read_seq: i64,
        ) -> Result<Option<DispatchChatReadCursorUpdate>, DomainError> {
            self.mark_read_calls
                .lock()
                .expect("lock mark read calls")
                .push(read_seq);
            let mut cursor = self.last_read_seq.lock().expect("lock cursor");
            let previous_last_read_seq = *cursor;
            *cursor = (*cursor).max(read_seq);
            Ok(Some(DispatchChatReadCursorUpdate {
                member: member_row(*cursor),
                previous_last_read_seq,
            }))
        }

        async fn get_group_latest_seq(&self, _group_id: &str) -> Result<i64, DomainError> {
            Ok(self.messages.lock().expect("lock messages").len() as i64)
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
            Ok(vec![DispatchChatMemberUnread {
                user_id: USER_ID.to_string(),
                unread_count: 0,
                unread_total: 0,
            }])
        }

        async fn find_active_members(&self, _group_id: &str) -> Result<Vec<DispatchChatMember>, DomainError> {
            let last_read_seq = *self.last_read_seq.lock().expect("lock cursor");
            Ok(vec![
                member_row(last_read_seq),
                member_row_for("user-2", last_read_seq),
                member_row_for("user-3", last_read_seq),
            ])
        }

        async fn find_group_members(&self, group_id: &str) -> Result<Vec<DispatchChatMember>, DomainError> {
            let mut members = self.find_active_members(group_id).await?;
            let mut inactive = member_row_for("user-readonly", 0);
            inactive.is_active = false;
            inactive.is_assignee = false;
            inactive.username = None;
            members.push(inactive);
            Ok(members)
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
            _archive_at: Option<DateTime<Utc>>,
            _metadata: &serde_json::Value,
        ) -> Result<DispatchChatGroupSummary, DomainError> {
            Ok(group_row())
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

        async fn find_groups_pending_deprecation(
            &self,
            _limit: i64,
        ) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
            Ok(vec![])
        }

        async fn find_due_archive_groups(&self, _limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
            Ok(vec![])
        }

        async fn archive_groups_batch(
            &self,
            _group_ids: &[String],
        ) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
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

        async fn summarize_receipts_for_flight(
            &self,
            _flight_id: &str,
        ) -> Result<NotificationReceiptSummary, DomainError> {
            Ok(receipt_summary())
        }

        async fn summarize_receipts_for_order(
            &self,
            _order_id: &str,
        ) -> Result<NotificationReceiptSummary, DomainError> {
            Ok(receipt_summary())
        }
    }

    #[derive(Default)]
    struct FakeMentionNotifier {
        batches: Mutex<Vec<DispatchBatchNotificationCreate>>,
        fail: Mutex<bool>,
    }

    impl FakeMentionNotifier {
        fn fail(&self) {
            *self.fail.lock().expect("lock fail") = true;
        }

        fn batches(&self) -> Vec<DispatchBatchNotificationCreate> {
            self.batches.lock().expect("lock batches").clone()
        }
    }

    #[async_trait::async_trait]
    impl DispatchChatMentionNotifier for FakeMentionNotifier {
        async fn notify_chat_mentions(&self, dto: DispatchBatchNotificationCreate) -> Result<(), DomainError> {
            if *self.fail.lock().expect("lock fail") {
                return Err(DomainError::Internal("mention notifier failed".into()));
            }
            self.batches.lock().expect("lock batches").push(dto);
            Ok(())
        }
    }

    fn service(repo: Arc<FakeChatRepo>) -> DispatchChatService {
        DispatchChatService::new(repo)
    }

    fn service_with_notifier(repo: Arc<FakeChatRepo>, notifier: Arc<FakeMentionNotifier>) -> DispatchChatService {
        DispatchChatService::new(repo).with_mention_notifier(notifier)
    }

    #[tokio::test]
    async fn retried_send_returns_the_stored_message_without_a_second_insert() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo.clone());

        let first = svc
            .send_message(GROUP_ID, USER_ID, "只发一次", false, Some("key-1"), &[])
            .await
            .expect("first send succeeds");
        assert!(!first.deduplicated, "the first send is a real insert");
        assert_eq!(repo.stored_count(), 1);
        assert_eq!(repo.events_of_type("message_sent"), 1);
        let mark_read_calls_after_first = repo.mark_read_call_count();

        let retry = svc
            .send_message(GROUP_ID, USER_ID, "只发一次", false, Some("key-1"), &[])
            .await
            .expect("retry succeeds");
        assert!(retry.deduplicated, "the retry must be reported as a duplicate");
        assert_eq!(retry.message.message_id, first.message.message_id);
        assert_eq!(retry.message.seq_no, first.message.seq_no);
        assert_eq!(repo.stored_count(), 1, "a retry must not store a second message");
        assert_eq!(
            repo.events_of_type("message_sent"),
            1,
            "a retry must not append a second ledger event"
        );
        assert_eq!(
            repo.mark_read_call_count(),
            mark_read_calls_after_first,
            "a retry resolves as a pure read, with no cursor write"
        );
    }

    #[tokio::test]
    async fn sends_without_a_client_msg_id_are_never_deduplicated() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo.clone());

        for _ in 0..2 {
            let outcome = svc
                .send_message(GROUP_ID, USER_ID, "同样的内容", false, None, &[])
                .await
                .expect("send succeeds");
            assert!(!outcome.deduplicated);
        }
        assert_eq!(repo.stored_count(), 2, "identical content is not an idempotency key");
    }

    #[tokio::test]
    async fn blank_client_msg_id_is_ignored_and_overlong_one_is_rejected() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo.clone());

        let outcome = svc
            .send_message(GROUP_ID, USER_ID, "空白键", false, Some("   "), &[])
            .await
            .expect("a blank key is treated as absent");
        assert!(!outcome.deduplicated);
        assert_eq!(outcome.message.client_msg_id, None);

        let error = svc
            .send_message(GROUP_ID, USER_ID, "超长键", false, Some(&"k".repeat(65)), &[])
            .await
            .expect_err("an overlong key cannot be stored");
        assert!(matches!(error, DispatchChatError::Validation(_)), "got {error:?}");
    }

    #[tokio::test]
    async fn send_message_keeps_only_member_mentions_and_sets_metadata() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo.clone());
        let outcome = svc
            .send_message(
                GROUP_ID,
                USER_ID,
                "请看 @李四",
                false,
                None,
                &["user-2".into(), "stranger".into(), USER_ID.into()],
            )
            .await
            .unwrap();
        assert_eq!(outcome.message.metadata["mention_user_ids"], json!(["user-2"]));
        assert_eq!(outcome.message.mention_user_ids, vec!["user-2".to_string()]);
        assert!(!outcome.message.is_at_all);
        assert_eq!(
            repo.events
                .lock()
                .expect("lock events")
                .iter()
                .find(|event| event.event_type == "message_sent")
                .expect("message_sent event")
                .payload["mention_user_ids"],
            json!(["user-2"])
        );
    }

    #[tokio::test]
    async fn send_message_sets_at_all_from_flag_or_content_and_empty_mentions_stay_empty() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo);

        let flagged = svc
            .send_message(GROUP_ID, USER_ID, "hello", true, None, &[])
            .await
            .unwrap();
        assert!(flagged.message.is_at_all);
        assert!(flagged.message.mention_user_ids.is_empty());
        assert_eq!(flagged.message.metadata["mention_user_ids"], json!([]));

        let from_content = svc
            .send_message(GROUP_ID, USER_ID, "请看 @全体", false, None, &[])
            .await
            .unwrap();
        assert!(from_content.message.is_at_all);
        assert!(from_content.message.mention_user_ids.is_empty());

        let from_all = svc
            .send_message(GROUP_ID, USER_ID, "please see @ALL", false, None, &[])
            .await
            .unwrap();
        assert!(from_all.message.is_at_all);
        assert!(from_all.message.mention_user_ids.is_empty());
    }

    #[tokio::test]
    async fn send_message_notifies_mentioned_member() {
        let repo = Arc::new(FakeChatRepo::default());
        let notifier = Arc::new(FakeMentionNotifier::default());
        let svc = service_with_notifier(repo, notifier.clone());

        let outcome = svc
            .send_message(GROUP_ID, USER_ID, "请看 @李四", false, None, &["user-2".into()])
            .await
            .unwrap();
        assert!(!outcome.deduplicated);

        let batches = notifier.batches();
        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!(batch.user_ids, vec!["user-2".to_string()]);
        assert_eq!(batch.category, "dispatch_chat_mention");
        assert_eq!(batch.severity, "info");
        assert_eq!(batch.title, "保障群 群聊有人@你");
        assert_eq!(batch.body, "user-1: 请看 @李四");
        assert_eq!(batch.flight_id.as_deref(), Some(FLIGHT_ID));
        assert_eq!(batch.group_id.as_deref(), Some(GROUP_ID));
        assert_eq!(batch.sender_user_id.as_deref(), Some(USER_ID));
        assert_eq!(batch.sender_username_snapshot, None);
        assert_eq!(batch.related_entity_type.as_deref(), Some("dispatch_chat_group"));
        assert_eq!(batch.related_entity_id.as_deref(), Some(GROUP_ID));
        assert_eq!(batch.dispatch_order_id, None);
        assert_eq!(batch.origin_type, "dispatch_chat");
        assert!(!batch.receipt_required);
    }

    #[tokio::test]
    async fn send_message_at_all_notifies_all_members_except_sender() {
        let repo = Arc::new(FakeChatRepo::default());
        let notifier = Arc::new(FakeMentionNotifier::default());
        let svc = service_with_notifier(repo, notifier.clone());

        let outcome = svc
            .send_message(GROUP_ID, USER_ID, "请看 @全体", true, None, &[])
            .await
            .unwrap();
        assert!(outcome.message.is_at_all);

        let batches = notifier.batches();
        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!(
            batch.user_ids,
            vec!["user-2".to_string(), "user-3".to_string(), "user-readonly".to_string()]
        );
        assert_eq!(batch.category, "dispatch_chat_mention");
        assert_eq!(batch.severity, "warning");
        assert_eq!(batch.title, "保障群 群聊有人@全体");
        assert_eq!(batch.body, "user-1: 请看 @全体");
        assert!(!batch.user_ids.iter().any(|id| id == USER_ID));
    }

    #[tokio::test]
    async fn deduplicated_send_does_not_notify_again() {
        let repo = Arc::new(FakeChatRepo::default());
        let notifier = Arc::new(FakeMentionNotifier::default());
        let svc = service_with_notifier(repo, notifier.clone());

        let first = svc
            .send_message(
                GROUP_ID,
                USER_ID,
                "请看 @李四",
                false,
                Some("key-mention-1"),
                &["user-2".into()],
            )
            .await
            .unwrap();
        assert!(!first.deduplicated);
        assert_eq!(notifier.batches().len(), 1);

        let retry = svc
            .send_message(
                GROUP_ID,
                USER_ID,
                "请看 @李四",
                false,
                Some("key-mention-1"),
                &["user-2".into()],
            )
            .await
            .unwrap();
        assert!(retry.deduplicated);
        assert_eq!(notifier.batches().len(), 1, "a deduplicated retry must not send_batch");
    }

    #[tokio::test]
    async fn mention_notifier_failure_does_not_fail_send_message() {
        let repo = Arc::new(FakeChatRepo::default());
        let notifier = Arc::new(FakeMentionNotifier::default());
        notifier.fail();
        let svc = service_with_notifier(repo.clone(), notifier.clone());

        let outcome = svc
            .send_message(GROUP_ID, USER_ID, "请看 @李四", false, None, &["user-2".into()])
            .await
            .expect("notification failure must not fail send_message");
        assert!(!outcome.deduplicated);
        assert_eq!(outcome.message.mention_user_ids, vec!["user-2".to_string()]);
        assert_eq!(repo.stored_count(), 1);
        assert!(notifier.batches().is_empty());
    }

    #[test]
    fn resolve_mentions_drops_strangers_self_and_blanks() {
        let members = vec![member_row(0), member_row_for("user-2", 0)];
        let (is_at_all, ids) = resolve_mentions(
            &["user-2".into(), "stranger".into(), USER_ID.into(), "  ".into()],
            false,
            "请看 @李四",
            USER_ID,
            &members,
        );
        assert!(!is_at_all);
        assert_eq!(ids, vec!["user-2".to_string()]);

        let (is_at_all, ids) = resolve_mentions(&[], false, "no mention", USER_ID, &members);
        assert!(!is_at_all);
        assert!(ids.is_empty());
    }

    #[tokio::test]
    async fn repeated_mark_read_reports_no_advance_and_writes_no_ledger_event() {
        let repo = Arc::new(FakeChatRepo::default());
        repo.seed_message("someone-else");
        repo.seed_message("someone-else");
        let svc = service(repo.clone());

        let first = svc
            .mark_group_read(GROUP_ID, USER_ID, None)
            .await
            .expect("mark read succeeds");
        assert!(first.advanced, "0 -> 2 is a real advance");
        assert_eq!(first.last_read_seq, 2);
        assert_eq!(repo.events_of_type("group_read_synced"), 1);

        let repeated = svc
            .mark_group_read(GROUP_ID, USER_ID, None)
            .await
            .expect("re-read succeeds");
        assert!(!repeated.advanced, "re-reading the same seq is not news");
        assert_eq!(
            repo.events_of_type("group_read_synced"),
            1,
            "an unchanged cursor must not append to the audit ledger"
        );
    }

    #[tokio::test]
    async fn mark_read_reports_where_the_cursor_landed_not_what_was_asked_for() {
        let repo = Arc::new(FakeChatRepo::default());
        repo.seed_message("someone-else");
        repo.seed_message("someone-else");
        let svc = service(repo.clone());

        svc.mark_group_read(GROUP_ID, USER_ID, Some(2))
            .await
            .expect("advance to 2");
        let backwards = svc
            .mark_group_read(GROUP_ID, USER_ID, Some(1))
            .await
            .expect("a backwards mark still succeeds");
        assert!(!backwards.advanced);
        assert_eq!(
            backwards.last_read_seq, 2,
            "the cursor never moves backwards, so 2 is reported rather than the requested 1"
        );
        assert_eq!(repo.events_of_type("group_read_synced"), 1);
    }

    #[tokio::test]
    async fn list_group_messages_rejects_two_cursors_at_once() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo);

        let error = svc
            .list_group_messages(GROUP_ID, USER_ID, 50, Some(10), Some(2))
            .await
            .expect_err("before_seq and after_seq cannot both be honoured");
        assert!(matches!(error, DispatchChatError::Validation(_)), "got {error:?}");

        let gap = svc
            .list_group_messages(GROUP_ID, USER_ID, 50, None, Some(2))
            .await
            .expect("gap-fill is accepted on its own");
        assert_eq!(gap.after_seq, Some(2));
        assert_eq!(gap.before_seq, None);
    }

    #[tokio::test]
    async fn list_group_members_includes_inactive_colleague() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo);

        let payload = svc
            .list_group_members(GROUP_ID, USER_ID)
            .await
            .expect("a member can list group members");
        let items = payload["items"].as_array().expect("items array");
        assert!(
            items
                .iter()
                .any(|item| { item["user_id"] == USER_ID && item["is_active"] == true && item["username"] == USER_ID }),
            "active caller should appear, got {items:?}"
        );
        assert!(
            items.iter().any(|item| {
                item["user_id"] == "user-readonly"
                    && item["is_active"] == false
                    && item["is_assignee"] == false
                    && item["username"] == ""
            }),
            "inactive colleague should appear with empty username, got {items:?}"
        );
    }

    #[tokio::test]
    async fn list_group_members_rejects_non_members() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo);

        let error = svc
            .list_group_members(GROUP_ID, "stranger")
            .await
            .expect_err("non-members cannot list");
        match error {
            DispatchChatError::Forbidden(message) => {
                assert_eq!(message, "当前用户不是该群成员");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_group_members_rejects_empty_ids() {
        let repo = Arc::new(FakeChatRepo::default());
        let svc = service(repo);

        let empty_group = svc
            .list_group_members("   ", USER_ID)
            .await
            .expect_err("empty group_id is forbidden");
        assert!(
            matches!(empty_group, DispatchChatError::Forbidden(_)),
            "got {empty_group:?}"
        );

        let empty_user = svc
            .list_group_members(GROUP_ID, "   ")
            .await
            .expect_err("empty user_id is forbidden");
        assert!(
            matches!(empty_user, DispatchChatError::Forbidden(_)),
            "got {empty_user:?}"
        );
    }

    #[test]
    fn archive_anchor_for_arrival_uses_actual_arrival() {
        let arrived = Utc::now();
        let departed = arrived - Duration::hours(2);
        assert_eq!(
            resolve_archive_anchor(true, false, Some(departed), Some(arrived)),
            Some(arrived)
        );
        assert_eq!(resolve_archive_anchor(true, false, None, Some(arrived)), Some(arrived));
        assert_eq!(
            resolve_archive_anchor(false, true, Some(departed), Some(arrived)),
            Some(departed)
        );
        assert_eq!(
            resolve_archive_anchor(true, true, Some(departed), Some(arrived)),
            Some(departed)
        );
    }

    #[test]
    fn memberships_retain_existing_dispatchers() {
        let memberships = build_group_memberships(&[], &[], &["ops-admin".to_string()], &HashMap::new(), 4);
        let retained = memberships
            .iter()
            .find(|membership| membership.user_id == "ops-admin")
            .expect("force-joined dispatcher stays in the plan");
        assert!(retained.is_dispatcher);
        assert!(!retained.is_assignee);
        assert_eq!(retained.last_read_seq, 4);
    }
}
