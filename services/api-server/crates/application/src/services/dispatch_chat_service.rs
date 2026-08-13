//! 派工聊天服务。

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, DispatchOrderStatus};
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;
use fms_domain::models::dispatch_collaboration::{
    DispatchChatGroupList, DispatchChatGroupSummary, DispatchChatMemberUpsert, DispatchChatMessage,
    DispatchChatUserProfile, NewDispatchChatMessage,
};
use fms_domain::models::flight::Flight;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::flight_repository::FlightRepository;

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

pub struct DispatchChatService {
    collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    dispatch_order_repo: Option<Arc<dyn DispatchOrderRepository + Send + Sync>>,
    flight_repo: Option<Arc<dyn FlightRepository + Send + Sync>>,
    event_publisher: Option<Arc<dyn DispatchChatEventPublisher + Send + Sync>>,
}

impl DispatchChatService {
    pub fn new(collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>) -> Self {
        Self {
            collaboration_repo,
            dispatch_order_repo: None,
            flight_repo: None,
            event_publisher: None,
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

        let memberships = build_group_memberships(
            &assignee_user_ids,
            &dispatcher_candidates,
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

        let Some(group) = self
            .collaboration_repo
            .get_group_for_user(&normalized_group_id, &normalized_user_id)
            .await?
        else {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        };

        let mut payload = self
            .collaboration_repo
            .list_group_messages(&normalized_group_id, limit.clamp(1, 200), before_seq)
            .await?;
        payload.limit = limit.clamp(1, 200);
        payload.before_seq = before_seq;
        if !group.member_is_active && payload.items.is_empty() {
            payload.has_more = false;
        }
        Ok(payload)
    }

    pub async fn send_message(
        &self,
        group_id: &str,
        user_id: &str,
        content: &str,
        at_all: bool,
    ) -> Result<DispatchChatMessage, DispatchChatError> {
        let normalized_group_id = normalize(group_id);
        let normalized_user_id = normalize(user_id);
        let normalized_content = normalize(content);

        if normalized_group_id.is_empty() || normalized_user_id.is_empty() {
            return Err(DispatchChatError::Forbidden("群聊访问被拒绝".into()));
        }
        if normalized_content.is_empty() || normalized_content.chars().count() > 2000 {
            return Err(DispatchChatError::Validation("消息内容长度应在 1~2000 字符".into()));
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

        let message_id = ulid::Ulid::new().to_string();
        let event_id = ulid::Ulid::new().to_string();
        let mut message = self
            .collaboration_repo
            .insert_message(&NewDispatchChatMessage {
                message_id,
                group_id: normalized_group_id.clone(),
                sender_user_id: Some(normalized_user_id.clone()),
                dispatch_order_id: None,
                event_id: Some(event_id.clone()),
                message_type: "text".to_string(),
                content: normalized_content.clone(),
                is_at_all: at_all || contains_at_all(&normalized_content),
                metadata: json!({}),
            })
            .await?;

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
                Some(normalized_user_id),
            )
            .await?;
            let _ = self
                .collaboration_repo
                .update_message_event_id(&message.message_id, &event_id)
                .await?;
            message.event_id = Some(event_id);
        }

        Ok(message)
    }

    pub async fn build_message_stream_events(
        &self,
        group_id: &str,
        message: &DispatchChatMessage,
    ) -> Result<Vec<(String, serde_json::Value)>, DispatchChatError> {
        let Some(group) = self.collaboration_repo.get_group_by_id(group_id).await? else {
            return Ok(Vec::new());
        };
        let members = self.collaboration_repo.find_active_members(group_id).await?;
        let timestamp = Utc::now().to_rfc3339();
        let mut events = Vec::new();
        for member in members {
            let user_id = normalize(&member.user_id);
            if user_id.is_empty() {
                continue;
            }
            let unread_count = self.collaboration_repo.count_group_unread(group_id, &user_id).await?;
            let unread_total = self.collaboration_repo.count_total_unread(&user_id).await?;
            events.push((
                user_id,
                json!({
                    "type": "dispatch_chat_message",
                    "group_id": group_id,
                    "flight_id": group.flight_id,
                    "message": message,
                    "unread_count": unread_count,
                    "unread_total": unread_total,
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
        let members = self.collaboration_repo.find_active_members(group_id).await?;
        let timestamp = Utc::now().to_rfc3339();
        let mut events = Vec::new();
        for member in members {
            let user_id = normalize(&member.user_id);
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
        let members = self.collaboration_repo.find_active_members(group_id).await?;
        let timestamp = Utc::now().to_rfc3339();
        let mut events = Vec::new();
        for member in members {
            let user_id = normalize(&member.user_id);
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
    ) -> Result<serde_json::Value, DispatchChatError> {
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
        let Some(updated_member) = updated else {
            return Err(DispatchChatError::Forbidden("当前用户不是该群成员".into()));
        };

        let unread_count = self
            .collaboration_repo
            .count_group_unread(&normalized_group_id, &normalized_user_id)
            .await?;
        let unread_total = self.collaboration_repo.count_total_unread(&normalized_user_id).await?;

        if !group.flight_id.trim().is_empty() {
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
                        "last_read_seq": target_seq,
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

        Ok(json!({
            "group_id": normalized_group_id,
            "last_read_seq": target_seq,
            "unread_count": unread_count,
            "unread_total": unread_total,
        }))
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
            if is_reopened_arrival {
                if self
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
        Ok(true)
    }

    async fn archive_group(
        &self,
        group: &DispatchChatGroupSummary,
        archived_at: DateTime<Utc>,
    ) -> Result<Option<DispatchChatLifecycleChange>, DispatchChatError> {
        let archived_groups = self
            .collaboration_repo
            .archive_groups_batch(&[group.group_id.clone()])
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

        Ok(Some(DispatchChatLifecycleChange::Archived {
            group_id: updated_group.group_id,
            archived_at,
        }))
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

fn resolve_archive_at(flight: &Flight) -> Option<DateTime<Utc>> {
    flight.actual_departure.map(|value| value + Duration::hours(6))
}

fn build_group_name(flight_id: &str, flight: Option<&Flight>) -> String {
    let outbound = flight
        .and_then(|value| value.outbound_leg.as_ref())
        .map(|leg| normalize(&leg.flight_no))
        .filter(|value| !value.is_empty());
    let inbound = flight
        .and_then(|value| value.inbound_leg.as_ref())
        .map(|leg| normalize(&leg.flight_no))
        .filter(|value| !value.is_empty());
    let flight_no = outbound.or(inbound);
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
    json!({
        "outbound_leg": flight
            .and_then(|value| value.outbound_leg.as_ref())
            .map(|leg| json!({ "flight_no": leg.flight_no }))
            .unwrap_or(serde_json::Value::Null),
        "inbound_leg": flight
            .and_then(|value| value.inbound_leg.as_ref())
            .map(|leg| json!({ "flight_no": leg.flight_no }))
            .unwrap_or(serde_json::Value::Null),
        "related_departments": related_departments,
        "active_dispatch_order_ids": active_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
        "related_dispatch_order_ids": related_orders.iter().map(|order| order.id.clone()).collect::<Vec<_>>(),
        "source": "dispatch_chat_v2",
    })
}

fn build_group_memberships(
    assignee_user_ids: &[String],
    dispatcher_candidates: &[fms_domain::models::dispatch_collaboration::DispatchChatDispatcherCandidate],
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
    matches!(flight, Some(flight) if flight.inbound_leg.is_some() && flight.outbound_leg.is_none())
}

fn is_departure_flight(flight: Option<&Flight>) -> bool {
    matches!(flight, Some(flight) if flight.outbound_leg.is_some() && flight.inbound_leg.is_none())
}

fn is_transit_flight(flight: Option<&Flight>) -> bool {
    matches!(flight, Some(flight) if flight.outbound_leg.is_some() && flight.inbound_leg.is_some())
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
