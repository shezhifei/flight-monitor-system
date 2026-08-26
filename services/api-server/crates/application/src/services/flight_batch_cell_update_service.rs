//! Atomic batch cell update for flight monitor multi-select edits.
//!
//! ADR-0002: Route → Application Service → Repository + same-tx outbox.
//! Snapshot fields update `flights` via `update_partial_in_tx`; timeline fields
//! append dispatch timeline events. All targets succeed or the transaction rolls back.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tracing::warn;
use ulid::Ulid;

use fms_domain::error::DomainError;
use fms_domain::models::flight::Flight;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_repository::{
    FlightRepository, FlightTransactionalRepository, FlightUpdatePatch, PatchField,
};
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjectionRepository;
use fms_domain::ports::flight_timeline_event_repository::{
    FlightTimelineEvent, FlightTimelineEventRepository, FlightTimelineEventTransactionalRepository,
};
use fms_domain::ports::unit_of_work::UnitOfWork;

use crate::schemas::flight_schemas::{
    FlightBatchCellConflictItem, FlightBatchCellResultItem, FlightBatchCellUpdateRequest,
    FlightBatchCellUpdateResponse, FlightBatchEditableField,
};
use crate::services::flight_domain_events::{
    build_timeline_upserted_payload, write_flight_outbox_event, write_flight_update_outbox_events,
    FLIGHT_AGGREGATE_TYPE, FLIGHT_TIMELINE_UPSERTED_EVENT,
};

pub const MAX_BATCH_CELL_TARGETS: usize = 200;
pub const MANUAL_BATCH_EDIT_SOURCE: &str = "manual_batch_edit";

#[derive(Debug)]
pub enum FlightBatchCellError {
    Validation(String),
    Forbidden(String),
    NotFound(String),
    Conflict { message: String, details: Value },
    Internal(String),
}

impl FlightBatchCellError {
    pub fn from_domain(error: DomainError) -> Self {
        match error {
            DomainError::ValidationError(message) | DomainError::BusinessRuleViolation(message) => {
                Self::Validation(message)
            }
            DomainError::PermissionDenied(message) => Self::Forbidden(message),
            DomainError::NotFound { entity_type, id } => Self::NotFound(format!("{entity_type} (id={id}) 未找到")),
            DomainError::Conflict(message) | DomainError::ConcurrencyConflict(message) => Self::Conflict {
                message,
                details: json!({ "conflicts": [] }),
            },
            DomainError::BusinessRuleViolationWithDetails { message, details } => Self::Conflict { message, details },
            DomainError::Internal(message) => Self::Internal(message),
            other => Self::Internal(other.to_string()),
        }
    }
}

impl std::fmt::Display for FlightBatchCellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(m)
            | Self::Forbidden(m)
            | Self::NotFound(m)
            | Self::Internal(m)
            | Self::Conflict { message: m, .. } => write!(f, "{m}"),
        }
    }
}

pub(crate) async fn load_flights_for_batch(
    repo: &(dyn FlightRepository + Send + Sync),
    flight_ids: &[String],
) -> Result<HashMap<String, Flight>, FlightBatchCellError> {
    let flights = repo
        .find_by_ids(flight_ids)
        .await
        .map_err(|error| FlightBatchCellError::Internal(error.to_string()))?;
    Ok(flights
        .into_iter()
        .map(|flight| (flight.flight_id.0.clone(), flight))
        .collect())
}

/// 批量改单元格的入口。
///
/// api 层只需要 `execute`，而服务本身因为持有事务句柄而带上了泛型参数。
/// 这个端口的存在就是为了**让那个泛型参数在这里停住**：否则 `fms-api`
/// 的处理器签名就得点名 `PgUnitOfWork`，于是 api 层多出一条对
/// `fms-infrastructure` 的生产依赖——把 P3 刚押下的边界输到隔壁一层。
#[async_trait::async_trait]
pub trait FlightBatchCellUpdate: Send + Sync {
    async fn execute(
        &self,
        request: FlightBatchCellUpdateRequest,
        actor_id: &str,
        is_admin: bool,
        permissions: &[String],
    ) -> Result<FlightBatchCellUpdateResponse, FlightBatchCellError>;
}

pub struct FlightBatchCellUpdateService<U: UnitOfWork> {
    repo: Arc<dyn FlightRepository + Send + Sync>,
    tx_repo: Arc<dyn FlightTransactionalRepository<U::Tx> + Send + Sync>,
    timeline_tx_repo: Arc<dyn FlightTimelineEventTransactionalRepository<U::Tx> + Send + Sync>,
    timeline_repo: Arc<dyn FlightTimelineEventRepository + Send + Sync>,
    projection_repo: Option<Arc<dyn FlightRuntimeProjectionRepository + Send + Sync>>,
    outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
    uow: Arc<U>,
}

impl<U: UnitOfWork> FlightBatchCellUpdateService<U> {
    pub fn new(
        repo: Arc<dyn FlightRepository + Send + Sync>,
        tx_repo: Arc<dyn FlightTransactionalRepository<U::Tx> + Send + Sync>,
        timeline_tx_repo: Arc<dyn FlightTimelineEventTransactionalRepository<U::Tx> + Send + Sync>,
        timeline_repo: Arc<dyn FlightTimelineEventRepository + Send + Sync>,
        outbox_repo: Arc<dyn DomainEventOutboxTransactionalRepository<U::Tx> + Send + Sync>,
        uow: Arc<U>,
    ) -> Self {
        Self {
            repo,
            tx_repo,
            timeline_tx_repo,
            timeline_repo,
            projection_repo: None,
            outbox_repo,
            uow,
        }
    }

    pub fn with_projection_repository(
        mut self,
        projection_repo: Arc<dyn FlightRuntimeProjectionRepository + Send + Sync>,
    ) -> Self {
        self.projection_repo = Some(projection_repo);
        self
    }

    /// Authorize, validate, and apply an all-or-nothing batch cell update.
    pub async fn execute(
        &self,
        request: FlightBatchCellUpdateRequest,
        actor_id: &str,
        is_admin: bool,
        permissions: &[String],
    ) -> Result<FlightBatchCellUpdateResponse, FlightBatchCellError> {
        authorize_field(request.field, is_admin, permissions)?;
        let validated = validate_request(&request)?;

        let batch_id =
            normalize_optional_text(request.client_action_id.clone()).unwrap_or_else(|| Ulid::new().to_string());

        // Sort targets by flight_id to reduce deadlock risk under concurrent batches.
        let mut targets = validated.targets;
        targets.sort_by(|a, b| a.flight_id.cmp(&b.flight_id));

        // For timeline fields, load latest milestone times once for all targets.
        let timeline_latest: HashMap<String, HashMap<String, DateTime<Utc>>> = if validated.field.is_timeline() {
            let ids: Vec<String> = targets.iter().map(|t| t.flight_id.clone()).collect();
            self.timeline_repo
                .latest_snapshots(&ids)
                .await
                .map_err(|e| FlightBatchCellError::Internal(e.to_string()))?
        } else {
            HashMap::new()
        };

        // Pre-load flights and collect all conflicts before any write.
        // Timeline expected_value is re-checked inside the transaction under
        // an advisory lock (see apply_timeline) to close the TOCTOU window.
        let ids: Vec<String> = targets.iter().map(|target| target.flight_id.clone()).collect();
        let flights_by_id = load_flights_for_batch(self.repo.as_ref(), &ids).await?;

        let mut loaded: Vec<(ValidatedTarget, Flight)> = Vec::with_capacity(targets.len());
        let mut conflicts = Vec::new();
        for target in targets {
            let Some(current) = flights_by_id.get(&target.flight_id).cloned() else {
                return Err(FlightBatchCellError::NotFound(format!(
                    "航班 {} 未找到",
                    target.flight_id
                )));
            };

            if let Some(expected_version) = target.expected_version {
                if current.version != expected_version {
                    conflicts.push(FlightBatchCellConflictItem {
                        flight_id: target.flight_id.clone(),
                        reason: "version_mismatch".to_string(),
                        expected_version: Some(expected_version),
                        current_version: Some(current.version),
                        expected_value: None,
                        current_value: None,
                    });
                    continue;
                }
            }

            // Fast-path optimistic check for snapshot fields (also enforced by
            // expected_version in partial update). For timeline, still do a
            // best-effort precheck here; definitive check is in-tx.
            let expected_value = &target.expected_value;
            let current_value = current_field_value(
                validated.field,
                &current,
                timeline_latest
                    .get(&target.flight_id)
                    .and_then(|m| m.get(validated.field.as_str()).copied()),
            );
            if !values_equal_for_field(validated.field, expected_value, &current_value) {
                conflicts.push(FlightBatchCellConflictItem {
                    flight_id: target.flight_id.clone(),
                    reason: "value_mismatch".to_string(),
                    expected_version: target.expected_version,
                    current_version: Some(current.version),
                    expected_value: Some(expected_value.clone()),
                    current_value: Some(current_value),
                });
                continue;
            }

            loaded.push((target, current));
        }

        if !conflicts.is_empty() {
            return Err(FlightBatchCellError::Conflict {
                message: format!("批量更新存在 {} 处冲突，未写入任何变更", conflicts.len()),
                details: json!({
                    "code": "FLIGHT_BATCH_CONFLICT",
                    "conflicts": conflicts,
                }),
            });
        }

        let mut tx = self
            .uow
            .begin()
            .await
            .map_err(|e| FlightBatchCellError::Internal(e.to_string()))?;

        let mut results = Vec::with_capacity(loaded.len());
        for (target, current) in &loaded {
            let item = if validated.field.is_snapshot() {
                self.apply_snapshot(&mut tx, validated.field, &validated.value, target, actor_id)
                    .await
            } else {
                self.apply_timeline(
                    &mut tx,
                    &batch_id,
                    validated.field,
                    &validated.value,
                    target,
                    current.version,
                    actor_id,
                )
                .await
            }
            .map_err(|e| match e {
                ApplyError::Conflict(item) => FlightBatchCellError::Conflict {
                    message: format!("批量更新冲突: {}", item.flight_id),
                    details: json!({
                        "code": "FLIGHT_BATCH_CONFLICT",
                        "conflicts": [item],
                    }),
                },
                ApplyError::NotFound(id) => FlightBatchCellError::NotFound(format!("航班 {id} 未找到")),
                ApplyError::Validation(message) => FlightBatchCellError::Validation(message),
                ApplyError::Internal(message) => FlightBatchCellError::Internal(message),
            })?;
            results.push(item);
        }

        self.uow
            .commit(tx)
            .await
            .map_err(|e| FlightBatchCellError::Internal(e.to_string()))?;

        // Best-effort post-commit refresh so immediate refreshFlights sees new timeline values.
        // Failure must not undo the committed transaction.
        if validated.field.is_timeline() {
            if let Some(projection_repo) = self.projection_repo.as_ref() {
                for item in &results {
                    if let Err(error) = projection_repo.rebuild_for_flight(&item.flight_id).await {
                        warn!(
                            flight_id = %item.flight_id,
                            error = %error,
                            "failed to rebuild runtime projection after batch-cells timeline write"
                        );
                    }
                }
            }
        }

        Ok(FlightBatchCellUpdateResponse {
            batch_id,
            field: validated.field.as_str().to_string(),
            updated_count: results.len(),
            results,
        })
    }

    async fn apply_snapshot(
        &self,
        tx: &mut U::Tx,
        field: FlightBatchEditableField,
        value: &ParsedBatchValue,
        target: &ValidatedTarget,
        actor_id: &str,
    ) -> Result<FlightBatchCellResultItem, ApplyError> {
        let expected_version = target.expected_version.ok_or_else(|| {
            ApplyError::Validation(format!(
                "targets[{}].expected_version is required for snapshot field {}",
                target.flight_id,
                field.as_str()
            ))
        })?;

        let patch = build_snapshot_patch(field, value, expected_version)?;
        let flight = self
            .tx_repo
            .update_partial_in_tx(tx, &target.flight_id, &patch)
            .await
            .map_err(map_update_error(&target.flight_id, expected_version))?;

        let Some(flight) = flight else {
            return Err(ApplyError::NotFound(target.flight_id.clone()));
        };

        write_flight_update_outbox_events(self.outbox_repo.as_ref(), tx, &target.flight_id, &patch, Some(actor_id))
            .await
            .map_err(|e| ApplyError::Internal(e.to_string()))?;

        Ok(FlightBatchCellResultItem {
            flight_id: target.flight_id.clone(),
            version: flight.version,
            value: snapshot_current_value(field, &flight),
            timeline_id: None,
        })
    }

    async fn apply_timeline(
        &self,
        tx: &mut U::Tx,
        batch_id: &str,
        field: FlightBatchEditableField,
        value: &ParsedBatchValue,
        target: &ValidatedTarget,
        current_version: i32,
        actor_id: &str,
    ) -> Result<FlightBatchCellResultItem, ApplyError> {
        let occurred_at = match value {
            ParsedBatchValue::DateTime(dt) => *dt,
            ParsedBatchValue::Clear => {
                return Err(ApplyError::Validation(format!(
                    "timeline field {} 不支持 null 值（批量撤销请使用单航班时间线 API）",
                    field.as_str()
                )));
            }
            ParsedBatchValue::Text(_) => {
                return Err(ApplyError::Validation(format!(
                    "timeline field {} 需要 datetime 值",
                    field.as_str()
                )));
            }
        };

        // Serialize concurrent milestone writers (shared with single-flight path).
        self.timeline_tx_repo
            .lock_milestone_in_tx(tx, &target.flight_id, field.as_str())
            .await
            .map_err(|e| ApplyError::Internal(e.to_string()))?;

        // Re-read last-write milestone value under the lock and re-check expected_value.
        let expected_value = &target.expected_value;
        let locked_current = self
            .timeline_tx_repo
            .latest_occurred_at_in_tx(tx, &target.flight_id, field.as_str())
            .await
            .map_err(|e| ApplyError::Internal(e.to_string()))?;
        let current_value = match locked_current {
            Some(dt) => json!(dt),
            None => Value::Null,
        };
        if !values_equal_for_field(field, expected_value, &current_value) {
            return Err(ApplyError::Conflict(FlightBatchCellConflictItem {
                flight_id: target.flight_id.clone(),
                reason: "value_mismatch".to_string(),
                expected_version: target.expected_version,
                current_version: Some(current_version),
                expected_value: Some(expected_value.clone()),
                current_value: Some(current_value),
            }));
        }

        // Client recorded_by is intentionally ignored; only JWT actor is stored.
        // Include field in the idempotency key so the same batch_id can still
        // write different milestone codes for the same flight without clashing.
        let client_action_id = format!("{batch_id}:{}:{}", field.as_str(), target.flight_id);
        let timeline_id = Ulid::new().to_string();
        let event = FlightTimelineEvent {
            timeline_id: timeline_id.clone(),
            flight_id: target.flight_id.clone(),
            milestone_code: field.as_str().to_string(),
            occurred_at,
            leg_type: field.timeline_leg_type().map(str::to_string),
            recorded_by: Some(actor_id.to_string()),
            client_action_id: Some(client_action_id.clone()),
            source: MANUAL_BATCH_EDIT_SOURCE.to_string(),
            payload: json!({}),
            created_at: Utc::now(),
        };

        let write = self
            .timeline_tx_repo
            .insert_in_tx(tx, &event, Some(client_action_id.as_str()))
            .await
            .map_err(|e| ApplyError::Internal(e.to_string()))?;

        if !write.inserted {
            // Same client_action_id already exists — only accept if the stored
            // event matches this request (true retry). Otherwise report conflict.
            let existing = &write.event;
            let same_field = existing.milestone_code == field.as_str();
            let same_time = existing.occurred_at.timestamp() == occurred_at.timestamp();
            if !same_field || !same_time {
                return Err(ApplyError::Conflict(FlightBatchCellConflictItem {
                    flight_id: target.flight_id.clone(),
                    reason: "idempotency_conflict".to_string(),
                    expected_version: None,
                    current_version: Some(current_version),
                    expected_value: Some(json!(occurred_at)),
                    current_value: Some(json!(existing.occurred_at)),
                }));
            }
        }

        if write.inserted {
            let timeline_value = json!({
                "timeline_id": write.event.timeline_id,
                "flight_id": write.event.flight_id,
                "milestone_code": write.event.milestone_code,
                "occurred_at": write.event.occurred_at,
                "leg_type": write.event.leg_type,
                "recorded_by": write.event.recorded_by,
                "client_action_id": write.event.client_action_id,
                "source": write.event.source,
                "payload": write.event.payload,
                "created_at": write.event.created_at,
            });

            write_flight_outbox_event(
                self.outbox_repo.as_ref(),
                tx,
                FLIGHT_AGGREGATE_TYPE,
                &target.flight_id,
                FLIGHT_TIMELINE_UPSERTED_EVENT,
                build_timeline_upserted_payload(
                    &target.flight_id,
                    &write.event.milestone_code,
                    &write.event.timeline_id,
                    timeline_value,
                    Some(actor_id),
                ),
            )
            .await
            .map_err(|e| ApplyError::Internal(e.to_string()))?;
        }

        Ok(FlightBatchCellResultItem {
            flight_id: target.flight_id.clone(),
            version: current_version,
            // Always report the value that is actually stored.
            value: json!(write.event.occurred_at),
            timeline_id: Some(write.event.timeline_id),
        })
    }
}

#[async_trait::async_trait]
impl<U: UnitOfWork> FlightBatchCellUpdate for FlightBatchCellUpdateService<U> {
    async fn execute(
        &self,
        request: FlightBatchCellUpdateRequest,
        actor_id: &str,
        is_admin: bool,
        permissions: &[String],
    ) -> Result<FlightBatchCellUpdateResponse, FlightBatchCellError> {
        FlightBatchCellUpdateService::execute(self, request, actor_id, is_admin, permissions).await
    }
}

// ---------------------------------------------------------------------------
// Validation / parsing helpers (unit-testable)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct ValidatedRequest {
    field: FlightBatchEditableField,
    value: ParsedBatchValue,
    targets: Vec<ValidatedTarget>,
}

#[derive(Debug, Clone)]
struct ValidatedTarget {
    flight_id: String,
    expected_version: Option<i32>,
    expected_value: Value,
}

#[derive(Debug, Clone)]
enum ParsedBatchValue {
    Clear,
    Text(String),
    DateTime(DateTime<Utc>),
}

enum ApplyError {
    Conflict(FlightBatchCellConflictItem),
    NotFound(String),
    Validation(String),
    Internal(String),
}

fn map_update_error(flight_id: &str, expected_version: i32) -> impl Fn(DomainError) -> ApplyError + '_ {
    move |error| match error {
        DomainError::ConcurrencyConflict(message) => {
            // Extract current version from message if possible.
            let current_version = message
                .split("current ")
                .nth(1)
                .and_then(|s| s.trim().parse::<i32>().ok());
            ApplyError::Conflict(FlightBatchCellConflictItem {
                flight_id: flight_id.to_string(),
                reason: "version_mismatch".to_string(),
                expected_version: Some(expected_version),
                current_version,
                expected_value: None,
                current_value: None,
            })
        }
        DomainError::ValidationError(message) => ApplyError::Validation(message),
        DomainError::NotFound { .. } => ApplyError::NotFound(flight_id.to_string()),
        other => ApplyError::Internal(other.to_string()),
    }
}

pub fn authorize_field(
    field: FlightBatchEditableField,
    is_admin: bool,
    permissions: &[String],
) -> Result<(), FlightBatchCellError> {
    if !field.is_sync_locked() {
        return Ok(());
    }
    if is_admin || permissions.iter().any(|p| p == "*") {
        return Ok(());
    }
    Err(FlightBatchCellError::Forbidden(format!(
        "权限不足：非管理员禁止批量修改外部同步受控字段: [{}]",
        field.as_str()
    )))
}

pub(crate) fn validate_request(
    request: &FlightBatchCellUpdateRequest,
) -> Result<ValidatedRequest, FlightBatchCellError> {
    if request.targets.is_empty() {
        return Err(FlightBatchCellError::Validation("targets 不能为空".to_string()));
    }
    if request.targets.len() > MAX_BATCH_CELL_TARGETS {
        return Err(FlightBatchCellError::Validation(format!(
            "targets 最多 {MAX_BATCH_CELL_TARGETS} 条，当前 {}",
            request.targets.len()
        )));
    }

    let mut seen = HashSet::with_capacity(request.targets.len());
    let mut targets = Vec::with_capacity(request.targets.len());
    for (index, target) in request.targets.iter().enumerate() {
        let flight_id = target.flight_id.trim().to_string();
        if flight_id.is_empty() {
            return Err(FlightBatchCellError::Validation(format!(
                "targets[{index}].flight_id 不能为空"
            )));
        }
        if !seen.insert(flight_id.clone()) {
            return Err(FlightBatchCellError::Validation(format!(
                "targets 存在重复 flight_id: {flight_id}"
            )));
        }
        if request.field.is_snapshot() && target.expected_version.is_none() {
            return Err(FlightBatchCellError::Validation(format!(
                "targets[{index}].expected_version is required for snapshot field {}",
                request.field.as_str()
            )));
        }
        targets.push(ValidatedTarget {
            flight_id,
            expected_version: target.expected_version,
            expected_value: target.expected_value.clone(),
        });
    }

    let value = parse_batch_value(request.field, &request.value)?;
    Ok(ValidatedRequest {
        field: request.field,
        value,
        targets,
    })
}

const MAX_FLIGHT_REMARKS_LEN: usize = 500;

fn text_max_len(field: FlightBatchEditableField) -> Option<usize> {
    match field {
        FlightBatchEditableField::FlightRemarks => Some(MAX_FLIGHT_REMARKS_LEN),
        _ => None,
    }
}

fn parse_batch_value(field: FlightBatchEditableField, value: &Value) -> Result<ParsedBatchValue, FlightBatchCellError> {
    if value.is_null() {
        if field.is_timeline() {
            return Err(FlightBatchCellError::Validation(format!(
                "timeline field {} 不支持 null 值",
                field.as_str()
            )));
        }
        return Ok(ParsedBatchValue::Clear);
    }

    match field {
        FlightBatchEditableField::FlightRemarks => {
            let text = value
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    FlightBatchCellError::Validation(format!("field {} 需要非空字符串或 null", field.as_str()))
                })?;
            if let Some(max_len) = text_max_len(field) {
                let chars = text.chars().count();
                if chars > max_len {
                    return Err(FlightBatchCellError::Validation(format!(
                        "field {} 长度不能超过 {max_len} 个字符，当前 {chars}",
                        field.as_str()
                    )));
                }
            }
            Ok(ParsedBatchValue::Text(text))
        }
        FlightBatchEditableField::ScheduledDeparture
        | FlightBatchEditableField::ScheduledArrival
        | FlightBatchEditableField::CobtTime
        | FlightBatchEditableField::BoardingAllowedTime
        | FlightBatchEditableField::StartBoardingTime
        | FlightBatchEditableField::EndBoardingTime
        | FlightBatchEditableField::OnBlocksTime
        | FlightBatchEditableField::OffBlocksTime => {
            let dt = parse_datetime_value(value).ok_or_else(|| {
                FlightBatchCellError::Validation(format!("field {} 需要 ISO-8601 datetime 或 null", field.as_str()))
            })?;
            Ok(ParsedBatchValue::DateTime(dt))
        }
    }
}

fn parse_datetime_value(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(s) = value.as_str() {
        return DateTime::parse_from_rfc3339(s.trim())
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| {
                // Accept trailing Z already handled by rfc3339; try chrono default.
                s.trim().parse::<DateTime<Utc>>().ok()
            });
    }
    None
}

fn build_snapshot_patch(
    field: FlightBatchEditableField,
    value: &ParsedBatchValue,
    expected_version: i32,
) -> Result<FlightUpdatePatch, ApplyError> {
    let mut patch = FlightUpdatePatch {
        expected_version: Some(expected_version),
        ..Default::default()
    };

    match field {
        FlightBatchEditableField::FlightRemarks => {
            patch.flight_remarks = match value {
                ParsedBatchValue::Clear => PatchField::Clear,
                ParsedBatchValue::Text(text) => PatchField::Set(text.clone()),
                ParsedBatchValue::DateTime(_) => {
                    return Err(ApplyError::Validation("flight_remarks 需要字符串或 null".into()));
                }
            };
        }
        FlightBatchEditableField::ScheduledDeparture => {
            patch.scheduled_departure = datetime_patch(value, "scheduled_departure")?;
        }
        FlightBatchEditableField::ScheduledArrival => {
            patch.scheduled_arrival = datetime_patch(value, "scheduled_arrival")?;
        }
        FlightBatchEditableField::CobtTime => {
            patch.cobt_time = datetime_patch(value, "cobt_time")?;
        }
        _ => {
            return Err(ApplyError::Validation(format!(
                "{} is not a snapshot field",
                field.as_str()
            )));
        }
    }

    Ok(patch)
}

fn datetime_patch(value: &ParsedBatchValue, field_name: &str) -> Result<PatchField<DateTime<Utc>>, ApplyError> {
    match value {
        ParsedBatchValue::Clear => Ok(PatchField::Clear),
        ParsedBatchValue::DateTime(dt) => Ok(PatchField::Set(*dt)),
        ParsedBatchValue::Text(_) => Err(ApplyError::Validation(format!(
            "{field_name} 需要 ISO-8601 datetime 或 null"
        ))),
    }
}

fn snapshot_current_value(field: FlightBatchEditableField, flight: &Flight) -> Value {
    match field {
        FlightBatchEditableField::ScheduledDeparture => json!(flight.scheduled_departure),
        FlightBatchEditableField::ScheduledArrival => json!(flight.scheduled_arrival),
        FlightBatchEditableField::CobtTime => json!(flight.cobt_time),
        FlightBatchEditableField::FlightRemarks => json!(flight.flight_remarks),
        FlightBatchEditableField::BoardingAllowedTime
        | FlightBatchEditableField::StartBoardingTime
        | FlightBatchEditableField::EndBoardingTime
        | FlightBatchEditableField::OnBlocksTime
        | FlightBatchEditableField::OffBlocksTime => Value::Null,
    }
}

/// Resolve the current value used for optimistic concurrency checks.
/// Snapshot fields come from the flight row; timeline fields from latest events.
fn current_field_value(
    field: FlightBatchEditableField,
    flight: &Flight,
    timeline_latest: Option<DateTime<Utc>>,
) -> Value {
    if field.is_timeline() {
        return match timeline_latest {
            Some(dt) => json!(dt),
            None => Value::Null,
        };
    }
    snapshot_current_value(field, flight)
}

fn values_equal_for_field(field: FlightBatchEditableField, expected: &Value, current: &Value) -> bool {
    if expected.is_null() && current.is_null() {
        return true;
    }
    match field {
        FlightBatchEditableField::ScheduledDeparture
        | FlightBatchEditableField::ScheduledArrival
        | FlightBatchEditableField::CobtTime
        | FlightBatchEditableField::BoardingAllowedTime
        | FlightBatchEditableField::StartBoardingTime
        | FlightBatchEditableField::EndBoardingTime
        | FlightBatchEditableField::OnBlocksTime
        | FlightBatchEditableField::OffBlocksTime => {
            let expected_dt = parse_datetime_value(expected);
            let current_dt = parse_datetime_value(current);
            match (expected_dt, current_dt) {
                // Compare at second precision: list projections and client
                // datetime-local inputs often drop sub-second fractions.
                (Some(a), Some(b)) => a.timestamp() == b.timestamp(),
                (None, None) => expected.is_null() && current.is_null(),
                _ => false,
            }
        }
        FlightBatchEditableField::FlightRemarks => {
            let expected_s = expected.as_str().map(str::trim).filter(|s| !s.is_empty());
            let current_s = current.as_str().map(str::trim).filter(|s| !s.is_empty());
            match (expected_s, current_s) {
                (Some(a), Some(b)) => a == b,
                (None, None) => expected.is_null() && (current.is_null() || current.as_str() == Some("")),
                _ => false,
            }
        }
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::flight_schemas::FlightBatchCellTarget;

    fn sample_request(
        field: FlightBatchEditableField,
        value: Value,
        targets: Vec<FlightBatchCellTarget>,
    ) -> FlightBatchCellUpdateRequest {
        FlightBatchCellUpdateRequest {
            field,
            value,
            client_action_id: Some("batch-1".into()),
            targets,
        }
    }

    #[test]
    fn field_enum_classifies_snapshot_and_timeline() {
        assert!(FlightBatchEditableField::CobtTime.is_snapshot());
        assert!(FlightBatchEditableField::FlightRemarks.is_snapshot());
        assert!(!FlightBatchEditableField::FlightRemarks.is_sync_locked());
        assert!(FlightBatchEditableField::CobtTime.is_sync_locked());
        assert!(FlightBatchEditableField::StartBoardingTime.is_timeline());
        assert_eq!(
            FlightBatchEditableField::OnBlocksTime.timeline_leg_type(),
            Some("inbound")
        );
        assert_eq!(
            FlightBatchEditableField::OffBlocksTime.timeline_leg_type(),
            Some("outbound")
        );
    }

    #[test]
    fn validate_rejects_empty_and_duplicate_and_oversize() {
        let empty = sample_request(FlightBatchEditableField::FlightRemarks, json!("note"), vec![]);
        assert!(matches!(
            validate_request(&empty),
            Err(FlightBatchCellError::Validation(m)) if m.contains("不能为空")
        ));

        let dup = sample_request(
            FlightBatchEditableField::FlightRemarks,
            json!("note"),
            vec![
                FlightBatchCellTarget {
                    flight_id: "F1".into(),
                    expected_version: Some(1),
                    expected_value: Value::Null,
                },
                FlightBatchCellTarget {
                    flight_id: "F1".into(),
                    expected_version: Some(1),
                    expected_value: Value::Null,
                },
            ],
        );
        assert!(matches!(
            validate_request(&dup),
            Err(FlightBatchCellError::Validation(m)) if m.contains("重复")
        ));

        let many: Vec<_> = (0..201)
            .map(|i| FlightBatchCellTarget {
                flight_id: format!("F{i}"),
                expected_version: Some(1),
                expected_value: Value::Null,
            })
            .collect();
        let oversize = sample_request(FlightBatchEditableField::FlightRemarks, json!("x"), many);
        assert!(matches!(
            validate_request(&oversize),
            Err(FlightBatchCellError::Validation(m)) if m.contains("最多")
        ));
    }

    #[test]
    fn validate_requires_expected_version_for_snapshot() {
        let req = sample_request(
            FlightBatchEditableField::FlightRemarks,
            json!("A12"),
            vec![FlightBatchCellTarget {
                flight_id: "F1".into(),
                expected_version: None,
                expected_value: Value::Null,
            }],
        );
        assert!(matches!(
            validate_request(&req),
            Err(FlightBatchCellError::Validation(m)) if m.contains("expected_version")
        ));
    }

    #[test]
    fn validate_accepts_timeline_without_version() {
        let req = sample_request(
            FlightBatchEditableField::StartBoardingTime,
            json!("2026-07-17T10:00:00Z"),
            vec![FlightBatchCellTarget {
                flight_id: "F1".into(),
                expected_version: None,
                expected_value: Value::Null,
            }],
        );
        let validated = validate_request(&req).expect("timeline without version ok");
        assert_eq!(validated.targets.len(), 1);
        assert!(matches!(validated.value, ParsedBatchValue::DateTime(_)));
    }

    #[test]
    fn authorize_denies_sync_locked_for_non_admin() {
        let err = authorize_field(FlightBatchEditableField::CobtTime, false, &[]).unwrap_err();
        assert!(matches!(err, FlightBatchCellError::Forbidden(_)));

        assert!(authorize_field(FlightBatchEditableField::CobtTime, true, &[]).is_ok());
        assert!(authorize_field(FlightBatchEditableField::CobtTime, false, &["*".to_string()]).is_ok());
        assert!(authorize_field(FlightBatchEditableField::FlightRemarks, false, &[]).is_ok());
        assert!(authorize_field(FlightBatchEditableField::StartBoardingTime, false, &[]).is_ok());
    }

    #[test]
    fn parse_rejects_null_for_timeline() {
        let err = parse_batch_value(FlightBatchEditableField::OnBlocksTime, &Value::Null).unwrap_err();
        assert!(matches!(err, FlightBatchCellError::Validation(_)));
    }

    #[test]
    fn parse_enforces_text_length_limits() {
        let long_remarks = "备".repeat(501);
        let err = parse_batch_value(FlightBatchEditableField::FlightRemarks, &json!(long_remarks)).unwrap_err();
        assert!(matches!(err, FlightBatchCellError::Validation(m) if m.contains("500")));

        let ok_remarks = parse_batch_value(FlightBatchEditableField::FlightRemarks, &json!("备".repeat(500))).unwrap();
        assert!(matches!(ok_remarks, ParsedBatchValue::Text(t) if t.chars().count() == 500));
    }

    #[test]
    fn field_enum_serde_roundtrip() {
        let field = FlightBatchEditableField::CobtTime;
        let encoded = serde_json::to_string(&field).unwrap();
        assert_eq!(encoded, "\"cobt_time\"");
        let decoded: FlightBatchEditableField = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, field);
    }

    #[test]
    fn current_field_value_uses_timeline_latest_not_null() {
        let flight = Flight {
            flight_id: fms_domain::models::value_objects::FlightId("f1".into()),
            airline_code: None,
            flight_number: None,
            registration: None,
            aircraft_type_detail: None,
            stand: None,
            gate: None,
            terminal: None,
            position: None,
            baggage_carousel: None,
            scheduled_departure: None,
            scheduled_arrival: None,
            estimated_departure: None,
            estimated_arrival: None,
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            status: fms_domain::models::value_objects::FlightStatus::Scheduled,
            inbound_leg: None,
            outbound_leg: None,
            anomaly_summary: Default::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            labels: vec![],
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
            direction: None,
            flight_kind: "passenger".to_string(),
            is_draft: false,
            divert: false,
        };
        assert!(current_field_value(FlightBatchEditableField::StartBoardingTime, &flight, None).is_null());
        let dt = DateTime::parse_from_rfc3339("2026-07-17T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let value = current_field_value(FlightBatchEditableField::StartBoardingTime, &flight, Some(dt));
        assert_eq!(value, json!(dt));
        let remarks = current_field_value(FlightBatchEditableField::FlightRemarks, &flight, Some(dt));
        assert!(remarks.is_null());
    }

    struct CountingFlightRepo {
        flights: HashMap<String, Flight>,
        find_by_id_calls: std::sync::atomic::AtomicUsize,
        find_by_ids_calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl FlightRepository for CountingFlightRepo {
        async fn find_by_id(&self, flight_id: &str) -> Result<Option<Flight>, DomainError> {
            self.find_by_id_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.flights.get(flight_id).cloned())
        }

        async fn find_by_ids(&self, flight_ids: &[String]) -> Result<Vec<Flight>, DomainError> {
            self.find_by_ids_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(flight_ids
                .iter()
                .filter_map(|id| self.flights.get(id).cloned())
                .collect())
        }

        async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
            unimplemented!("counting repo: find_all")
        }
        async fn find_by_date(&self, _date: chrono::NaiveDate) -> Result<Vec<Flight>, DomainError> {
            unimplemented!("counting repo: find_by_date")
        }
        async fn find_by_flight_number(&self, _flight_no: &str) -> Result<Vec<Flight>, DomainError> {
            unimplemented!("counting repo: find_by_flight_number")
        }
        async fn find_by_status(&self, _status: i32, _limit: i64, _offset: i64) -> Result<Vec<Flight>, DomainError> {
            unimplemented!("counting repo: find_by_status")
        }
        async fn save(&self, _flight: &Flight) -> Result<(), DomainError> {
            unimplemented!("counting repo: save")
        }
        async fn update_partial(
            &self,
            _flight_id: &str,
            _patch: &FlightUpdatePatch,
        ) -> Result<Option<Flight>, DomainError> {
            unimplemented!("counting repo: update_partial")
        }
        async fn save_batch(&self, _flights: &[Flight]) -> Result<usize, DomainError> {
            unimplemented!("counting repo: save_batch")
        }
        async fn update_status(&self, _flight_id: &str, _status: i32) -> Result<bool, DomainError> {
            unimplemented!("counting repo: update_status")
        }
        async fn delete(&self, _flight_id: &str) -> Result<bool, DomainError> {
            unimplemented!("counting repo: delete")
        }
        async fn search(
            &self,
            _criteria: &fms_domain::ports::flight_repository::FlightSearchCriteria,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<Flight>, DomainError> {
            unimplemented!("counting repo: search")
        }
        async fn count_by_date(&self, _date: chrono::NaiveDate) -> Result<i64, DomainError> {
            unimplemented!("counting repo: count_by_date")
        }
    }

    fn counting_sample_flight(flight_id: &str) -> Flight {
        Flight {
            flight_id: fms_domain::models::value_objects::FlightId::from(flight_id),
            airline_code: None,
            flight_number: None,
            registration: None,
            aircraft_type_detail: None,
            stand: None,
            gate: None,
            terminal: None,
            position: None,
            baggage_carousel: None,
            scheduled_departure: None,
            scheduled_arrival: None,
            estimated_departure: None,
            estimated_arrival: None,
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            status: fms_domain::models::value_objects::FlightStatus::Scheduled,
            inbound_leg: None,
            outbound_leg: None,
            anomaly_summary: Default::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            labels: vec![],
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
            direction: None,
            flight_kind: "passenger".to_string(),
            is_draft: false,
            divert: false,
        }
    }

    #[tokio::test]
    async fn batch_preload_uses_find_by_ids_not_per_key_get() {
        let mut flights = HashMap::new();
        flights.insert("F1".to_string(), counting_sample_flight("F1"));
        flights.insert("F2".to_string(), counting_sample_flight("F2"));
        let repo = CountingFlightRepo {
            flights,
            find_by_id_calls: std::sync::atomic::AtomicUsize::new(0),
            find_by_ids_calls: std::sync::atomic::AtomicUsize::new(0),
        };
        let loaded = load_flights_for_batch(&repo, &["F1".to_string(), "F2".to_string()])
            .await
            .expect("load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(repo.find_by_ids_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(repo.find_by_id_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
