//! Flight dispatch timeline event persistence and lookup.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};
use ulid::Ulid;

use fms_domain::error::DomainError;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_timeline_event_repository::{
    FlightTimelineEvent, FlightTimelineEventTransactionalRepository, FlightTimelineWriteResult,
};
use fms_domain::ports::unit_of_work::UnitOfWork;

use crate::schemas::flight_schemas::{DispatchTimelineEventCreate, DispatchTimelineEventResponse};
use crate::services::flight_domain_events::{
    build_timeline_deleted_payload, build_timeline_upserted_payload, write_flight_outbox_event, FLIGHT_AGGREGATE_TYPE,
    FLIGHT_TIMELINE_DELETED_EVENT, FLIGHT_TIMELINE_UPSERTED_EVENT,
};

use super::types::{DispatchTimelineEventWriteResult, FlightRuntimeService};

impl FlightRuntimeService {
    pub async fn list_dispatch_timeline(
        &self,
        flight_id: &str,
    ) -> Result<Vec<DispatchTimelineEventResponse>, DomainError> {
        let mut db_items = self.query_dispatch_timeline_from_db(flight_id).await?;
        let state = self.state.read().await;
        if let Some(memory_items) = state.timeline_by_flight.get(flight_id) {
            let mut known_ids = db_items
                .iter()
                .map(|item| item.timeline_id.clone())
                .collect::<HashSet<_>>();
            for item in memory_items {
                if known_ids.insert(item.timeline_id.clone()) {
                    db_items.push(item.clone());
                }
            }
        }
        db_items.sort_by(|left, right| {
            right
                .occurred_at
                .cmp(&left.occurred_at)
                .then_with(|| right.created_at.cmp(&left.created_at))
        });
        Ok(db_items)
    }

    pub async fn create_dispatch_timeline_event(
        &self,
        flight_id: &str,
        payload: DispatchTimelineEventCreate,
    ) -> Result<DispatchTimelineEventWriteResult, DomainError> {
        let writer = self
            .timeline_writer
            .as_ref()
            .ok_or_else(|| DomainError::Internal("flight timeline transactional repository unavailable".to_string()))?;

        let result = writer.create(flight_id, payload).await?;

        self.refresh_projection_for_flight(flight_id).await;
        Ok(result)
    }

    pub async fn delete_dispatch_timeline_event(
        &self,
        flight_id: &str,
        timeline_id: &str,
    ) -> Result<bool, DomainError> {
        let writer = self
            .timeline_writer
            .as_ref()
            .ok_or_else(|| DomainError::Internal("flight timeline transactional repository unavailable".to_string()))?;

        // 内存态那份副本不在事务里。这段原先夹在 DELETE 和 commit 之间，但它从不随
        // 事务回滚——放到事务外面语义相同。而「内存里删没删」只有服务自己知道，所以
        // 要作为入参交给写入方，参与「要不要发事件」的判断。
        let mut state = self.state.write().await;
        let memory_deleted = if let Some(items) = state.timeline_by_flight.get_mut(flight_id) {
            let before = items.len();
            items.retain(|item| item.timeline_id != timeline_id);
            before != items.len()
        } else {
            false
        };
        drop(state);

        let changed = writer.delete(flight_id, timeline_id, memory_deleted).await?;
        if changed {
            self.refresh_projection_for_flight(flight_id).await;
        }
        Ok(changed)
    }

    async fn query_dispatch_timeline_from_db(
        &self,
        flight_id: &str,
    ) -> Result<Vec<DispatchTimelineEventResponse>, DomainError> {
        // Add LIMIT to prevent unbounded memory growth from flights with
        // excessive timeline history. 2000 events is sufficient for any
        // operational timeline display.
        const MAX_TIMELINE_EVENTS: i64 = 2000;
        let Some(timeline_repo) = self.timeline_repo.as_ref() else {
            return Ok(Vec::new());
        };
        let rows = timeline_repo.list_by_flight(flight_id, MAX_TIMELINE_EVENTS).await?;
        Ok(rows.into_iter().map(to_response).collect())
    }
}

fn to_domain_event(event: &DispatchTimelineEventResponse) -> FlightTimelineEvent {
    FlightTimelineEvent {
        timeline_id: event.timeline_id.clone(),
        flight_id: event.flight_id.clone(),
        milestone_code: event.milestone_code.clone(),
        occurred_at: event.occurred_at,
        leg_type: event.leg_type.clone(),
        recorded_by: event.recorded_by.clone(),
        client_action_id: event.client_action_id.clone(),
        source: event.source.clone(),
        payload: event.payload.clone(),
        created_at: event.created_at,
    }
}

fn to_response(event: FlightTimelineEvent) -> DispatchTimelineEventResponse {
    DispatchTimelineEventResponse {
        timeline_id: event.timeline_id,
        flight_id: event.flight_id,
        milestone_code: event.milestone_code,
        occurred_at: event.occurred_at,
        leg_type: event.leg_type,
        recorded_by: event.recorded_by,
        client_action_id: event.client_action_id,
        source: event.source,
        payload: event.payload,
        created_at: event.created_at,
    }
}

fn to_write_result(result: FlightTimelineWriteResult) -> DispatchTimelineEventWriteResult {
    DispatchTimelineEventWriteResult {
        event: to_response(result.event),
        inserted: result.inserted,
    }
}

fn normalize_leg_type(value: String) -> Option<String> {
    match value.trim().to_lowercase().as_str() {
        "inbound" => Some("inbound".to_string()),
        "outbound" => Some("outbound".to_string()),
        _ => None,
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn normalize_text_or_default(value: Option<String>, default: &str) -> String {
    normalize_optional_text(value).unwrap_or_else(|| default.to_string())
}

fn normalize_json_object(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        Value::Null => json!({}),
        other => json!({ "value": other }),
    }
}

/// 调度时间线的两个写入事务。
///
/// 这是 `FlightRuntimeService` 里最后两处持有事务句柄的地方，但那个服务有 18 处
/// `web::Data` 注入、api 层要调它二十来个方法。把服务本身改成 `FlightRuntimeService<U>`，
/// 就得把那二十个方法在一个端口里重写一遍，端口会变成服务 API 的抄本——所以这里反过来：
/// 只把带事务的两个单元抽成泛型协作者，服务保持非泛型，端口只有两个方法。
#[async_trait::async_trait]
pub trait DispatchTimelineWriter: Send + Sync {
    /// 锁住 (flight_id, milestone_code) → 插入 → 确实新插入时写一行 outbox，同一事务。
    async fn create(
        &self,
        flight_id: &str,
        payload: DispatchTimelineEventCreate,
    ) -> Result<DispatchTimelineEventWriteResult, DomainError>;

    /// 删除 → 有变化时写一行 outbox，同一事务。
    ///
    /// `memory_deleted` 由调用方给，理由见 `delete_dispatch_timeline_event`。
    async fn delete(&self, flight_id: &str, timeline_id: &str, memory_deleted: bool) -> Result<bool, DomainError>;
}

pub struct FlightTimelineWriter<U: UnitOfWork> {
    timeline_tx_repo: Arc<dyn FlightTimelineEventTransactionalRepository<U::Tx> + Send + Sync>,
    outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> FlightTimelineWriter<U> {
    pub fn new(
        timeline_tx_repo: Arc<dyn FlightTimelineEventTransactionalRepository<U::Tx> + Send + Sync>,
        outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            timeline_tx_repo,
            outbox_repo,
            uow,
        }
    }
}

#[async_trait::async_trait]
impl<U: UnitOfWork> DispatchTimelineWriter for FlightTimelineWriter<U> {
    async fn create(
        &self,
        flight_id: &str,
        payload: DispatchTimelineEventCreate,
    ) -> Result<DispatchTimelineEventWriteResult, DomainError> {
        let milestone_code = payload.milestone_code.trim().to_string();

        let mut tx = self.uow.begin().await?;
        // Same concurrency protocol as batch-cells: serialize writers per
        // (flight_id, milestone_code) for the duration of this transaction.
        self.timeline_tx_repo
            .lock_milestone_in_tx(&mut tx, flight_id, &milestone_code)
            .await?;

        // Generate the last-write ordering keys only after acquiring the lock,
        // so created_at/timeline_id order matches the serialized writer order.
        let timeline_id = Ulid::new().to_string();
        let event = DispatchTimelineEventResponse {
            timeline_id: timeline_id.clone(),
            flight_id: flight_id.to_string(),
            milestone_code,
            occurred_at: payload.occurred_at,
            leg_type: payload.leg_type.and_then(normalize_leg_type),
            recorded_by: normalize_optional_text(payload.recorded_by),
            client_action_id: normalize_optional_text(payload.client_action_id),
            source: normalize_text_or_default(Some(payload.source), "manual"),
            payload: normalize_json_object(payload.payload),
            created_at: Utc::now(),
        };
        let domain_event = to_domain_event(&event);
        let write = self
            .timeline_tx_repo
            .insert_in_tx(&mut tx, &domain_event, event.client_action_id.as_deref())
            .await?;
        let result = to_write_result(write);
        if result.inserted {
            let timeline_value = serde_json::to_value(&result.event)
                .unwrap_or_else(|_| json!({ "timeline_id": result.event.timeline_id }));
            write_flight_outbox_event(
                self.outbox_repo.as_ref(),
                &mut tx,
                FLIGHT_AGGREGATE_TYPE,
                flight_id,
                FLIGHT_TIMELINE_UPSERTED_EVENT,
                build_timeline_upserted_payload(
                    flight_id,
                    &result.event.milestone_code,
                    &result.event.timeline_id,
                    timeline_value,
                    result.event.recorded_by.as_deref(),
                ),
            )
            .await?;
        }
        self.uow.commit(tx).await?;

        Ok(result)
    }

    async fn delete(&self, flight_id: &str, timeline_id: &str, memory_deleted: bool) -> Result<bool, DomainError> {
        let mut tx = self.uow.begin().await?;
        let deleted = self
            .timeline_tx_repo
            .delete_in_tx(&mut tx, flight_id, timeline_id)
            .await?;

        let changed = deleted || memory_deleted;
        if changed {
            write_flight_outbox_event(
                self.outbox_repo.as_ref(),
                &mut tx,
                FLIGHT_AGGREGATE_TYPE,
                flight_id,
                FLIGHT_TIMELINE_DELETED_EVENT,
                build_timeline_deleted_payload(flight_id, timeline_id),
            )
            .await?;
        }
        self.uow.commit(tx).await?;

        Ok(changed)
    }
}
