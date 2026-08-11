//! 航班应用服务
//!
//! 编排航班领域逻辑，面向 API 路由层。

use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use fms_domain::error::DomainError;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::flight_repository::{FlightRepository, FlightSearchCriteria, FlightUpdatePatch};
use sqlx::{PgPool, Postgres, Transaction};

use crate::schemas::flight_schemas::{FlightCreate, FlightListResponse, FlightResponse, FlightUpdate};
use crate::services::flight_command_validator;
use crate::services::flight_commands::{FlightCreateCommand, FlightUpdateCommand};
use crate::services::flight_mappers::{from_create, to_response, update_patch_from_dto};
use crate::sqlx_transactional_repositories::SqlxFlightTransactionalRepository;

const NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(5);

pub use crate::services::flight_domain_events::{
    build_created_payload, build_deleted_payload, build_leg_upserted_payload, build_remarks_updated_payload,
    build_resource_updated_payload, build_status_updated_payload, write_flight_outbox_event,
    write_flight_update_outbox_events, FLIGHT_AGGREGATE_TYPE, FLIGHT_CREATED_EVENT, FLIGHT_DELETED_EVENT,
    FLIGHT_LEG_UPSERTED_EVENT, FLIGHT_REMARKS_UPDATED_EVENT, FLIGHT_RESOURCE_UPDATED_EVENT,
    FLIGHT_STATUS_UPDATED_EVENT,
};

pub struct FlightService {
    repo: Arc<dyn FlightRepository + Send + Sync>,
    tx_repo: Option<Arc<dyn SqlxFlightTransactionalRepository>>,
    /// Optional pool for same-transaction outbox writes on create/update.
    pool: Option<PgPool>,
    hot_list: RwLock<Option<FlightListResponse>>,
    negative_cache: DashMap<String, Instant>,
}

static FLIGHT_SERVICE_LIST_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn perf_trace_enabled() -> bool {
    std::env::var("FMS_PERF_TRACE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn should_emit_perf_trace(counter: &AtomicU64) -> bool {
    if !perf_trace_enabled() {
        return false;
    }
    let sample_rate = std::env::var("FMS_PERF_TRACE_SAMPLE_RATE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1000);
    counter.fetch_add(1, Ordering::Relaxed) % sample_rate == 0
}

impl FlightService {
    pub fn new(repo: Arc<dyn FlightRepository + Send + Sync>) -> Self {
        Self {
            repo,
            tx_repo: None,
            pool: None,
            hot_list: RwLock::new(None),
            negative_cache: DashMap::new(),
        }
    }

    pub fn with_transactional_repository(mut self, tx_repo: Arc<dyn SqlxFlightTransactionalRepository>) -> Self {
        self.tx_repo = Some(tx_repo);
        self
    }

    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn denied_update_fields(&self, dto: &FlightUpdate, is_admin: bool, permissions: &[String]) -> Vec<String> {
        flight_command_validator::denied_update_fields(dto, is_admin, permissions)
    }

    /// 批量查询航班登机口映射。
    pub async fn batch_get_gate_map(
        &self,
        flight_ids: &[String],
    ) -> Result<HashMap<String, Option<String>>, DomainError> {
        let normalized_ids = flight_ids
            .iter()
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .fold(Vec::<String>::new(), |mut acc, item| {
                if !acc.iter().any(|existing| existing == item) {
                    acc.push(item.to_string());
                }
                acc
            });

        if normalized_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut gate_map = HashMap::with_capacity(normalized_ids.len());
        for flight_id in normalized_ids {
            let gate = self
                .repo
                .find_by_id(&flight_id)
                .await?
                .and_then(|flight| flight.gate.map(|gate| gate.0));
            gate_map.insert(flight_id, gate);
        }
        Ok(gate_map)
    }

    /// 查询航班详情
    pub async fn get_flight(&self, flight_id: &str) -> Result<Option<FlightResponse>, DomainError> {
        if let Some(entry) = self.negative_cache.get(flight_id) {
            if entry.elapsed() < NEGATIVE_CACHE_TTL {
                return Ok(None);
            }
            drop(entry);
            self.negative_cache.remove(flight_id);
        }
        let flight = self.repo.find_by_id(flight_id).await?;
        if flight.is_none() {
            self.negative_cache.insert(flight_id.to_string(), Instant::now());
        }
        Ok(flight.map(|f| to_response(&f)))
    }

    /// 航班分页列表
    pub async fn list_flights(
        &self,
        page: i64,
        size: i64,
        has_open_anomaly: Option<bool>,
    ) -> Result<FlightListResponse, DomainError> {
        let trace = should_emit_perf_trace(&FLIGHT_SERVICE_LIST_TRACE_COUNTER);
        let total_start = Instant::now();
        let offset = (page - 1).max(0) * size;
        let hot_path = page == 1 && size == 20 && has_open_anomaly.is_none();
        if hot_path {
            if let Some(response) = self.hot_list.read().await.clone() {
                if trace {
                    tracing::info!(
                        target: "fms_perf",
                        event = "flights_list_service",
                        page,
                        page_size = size,
                        offset,
                        has_open_anomaly = false,
                        repo_items = response.items.len(),
                        repo_ms = 0.0,
                        dto_map_ms = 0.0,
                        total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
                        hot_cache_hit = true,
                    );
                }
                return Ok(response);
            }
        }
        let repo_start = Instant::now();
        let items = if has_open_anomaly.is_some() {
            self.repo
                .search(
                    &FlightSearchCriteria {
                        has_open_anomaly,
                        ..FlightSearchCriteria::default()
                    },
                    size,
                    offset,
                )
                .await?
        } else {
            self.repo.find_all(size, offset).await?
        };
        let repo_ms = repo_start.elapsed().as_secs_f64() * 1000.0;
        let total = items.len() as i64;
        let pages = if size > 0 { (total + size - 1) / size } else { 0 };
        let map_start = Instant::now();
        let response_items = items.iter().map(to_response).collect();
        let map_ms = map_start.elapsed().as_secs_f64() * 1000.0;

        if trace {
            tracing::info!(
                target: "fms_perf",
                event = "flights_list_service",
                page,
                page_size = size,
                offset,
                has_open_anomaly = has_open_anomaly.unwrap_or(false),
                repo_items = items.len(),
                repo_ms,
                dto_map_ms = map_ms,
                total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
            );
        }

        let response = FlightListResponse {
            items: response_items,
            total,
            page,
            size,
            pages,
        };
        if hot_path {
            *self.hot_list.write().await = Some(response.clone());
        }

        Ok(response)
    }

    /// 创建航班（显式命令入口，ADR-0002）。
    pub async fn execute_create(&self, command: FlightCreateCommand) -> Result<FlightResponse, DomainError> {
        self.create_flight_inner(command.dto, command.actor_id).await
    }

    /// 更新航班（显式命令入口，ADR-0002）。
    pub async fn execute_update(&self, command: FlightUpdateCommand) -> Result<Option<FlightResponse>, DomainError> {
        self.update_flight_inner(&command.flight_id, command.dto, command.actor_id)
            .await
    }

    /// 创建航班（兼容入口；请使用 `execute_create` + `FlightCreateCommand`）。
    #[deprecated(note = "use execute_create / execute_update with FlightCreate/UpdateCommand (ADR-0002)")]
    pub(crate) async fn create_flight(
        &self,
        dto: FlightCreate,
        created_by: Option<String>,
    ) -> Result<FlightResponse, DomainError> {
        self.create_flight_inner(dto, created_by).await
    }

    /// 更新航班（兼容入口；请使用 `execute_update` + `FlightUpdateCommand`）。
    #[deprecated(note = "use execute_create / execute_update with FlightCreate/UpdateCommand (ADR-0002)")]
    pub(crate) async fn update_flight(
        &self,
        flight_id: &str,
        dto: FlightUpdate,
        updated_by: Option<String>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        self.update_flight_inner(flight_id, dto, updated_by).await
    }

    async fn create_flight_inner(
        &self,
        dto: FlightCreate,
        created_by: Option<String>,
    ) -> Result<FlightResponse, DomainError> {
        let dto = flight_command_validator::validate_create_payload(self.repo.as_ref(), dto).await?;
        let flight = from_create(dto)?;

        if let (Some(pool), Some(tx_repo)) = (self.pool.as_ref(), self.tx_repo.as_ref()) {
            let mut tx = pool.begin().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            tx_repo.save_in_tx(&mut tx, &flight).await?;
            write_flight_outbox_event(
                &mut tx,
                FLIGHT_AGGREGATE_TYPE,
                flight.flight_id.as_str(),
                FLIGHT_CREATED_EVENT,
                build_created_payload(
                    flight.flight_id.as_str(),
                    &flight.status.to_string(),
                    created_by.as_deref(),
                ),
            )
            .await?;
            tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
        } else {
            self.repo.save(&flight).await?;
        }

        self.invalidate_hot_list().await;
        self.negative_cache.remove(flight.flight_id.as_str());
        let mut response = to_response(&flight);
        response.created_by = created_by.clone();
        response.updated_by = created_by;
        Ok(response)
    }

    async fn update_flight_inner(
        &self,
        flight_id: &str,
        dto: FlightUpdate,
        updated_by: Option<String>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        flight_command_validator::validate_update_payload(&dto)?;
        let patch = update_patch_from_dto(dto)?;
        flight_command_validator::ensure_status_transition(self.repo.as_ref(), flight_id, &patch).await?;

        let flight = if let (Some(pool), Some(tx_repo)) = (self.pool.as_ref(), self.tx_repo.as_ref()) {
            let mut tx = pool.begin().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            let Some(flight) = tx_repo.update_partial_in_tx(&mut tx, flight_id, &patch).await? else {
                return Ok(None);
            };
            write_flight_update_outbox_events(&mut tx, flight_id, &patch, updated_by.as_deref()).await?;
            tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            flight
        } else {
            let Some(flight) = self.repo.update_partial(flight_id, &patch).await? else {
                return Ok(None);
            };
            flight
        };

        self.invalidate_hot_list().await;
        let mut response = to_response(&flight);
        response.updated_by = updated_by;
        Ok(Some(response))
    }

    pub async fn update_flight_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        flight_id: &str,
        dto: FlightUpdate,
        updated_by: Option<String>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        flight_command_validator::validate_update_payload(&dto)?;
        let patch = update_patch_from_dto(dto)?;
        flight_command_validator::ensure_status_transition(self.repo.as_ref(), flight_id, &patch).await?;
        let tx_repo = self.tx_repo.as_ref().ok_or_else(|| {
            DomainError::Internal("FlightService transactional repository is not configured".to_string())
        })?;
        let Some(flight) = tx_repo.update_partial_in_tx(tx, flight_id, &patch).await? else {
            return Ok(None);
        };
        // Domain flight facts for projection/SSE. Callers (e.g. DomainActionExecutor)
        // may also write an action-level outbox row (Flight.update_status) in the same tx.
        write_flight_update_outbox_events(tx, flight_id, &patch, updated_by.as_deref()).await?;
        self.invalidate_hot_list().await;
        let mut response = to_response(&flight);
        response.updated_by = updated_by;
        Ok(Some(response))
    }

    /// 批确认 draft 航班（ONTOLOGY_V1.md §3.3，不变量 5）。
    ///
    /// 仅 `flight_kind == "passenger"` 且 `is_draft == true` 的航班可确认；
    /// 确认后 `is_draft = false`，方允许被正式 StandOccupation 引用。
    /// 乐观并发由 `expected_version` 保证（版本冲突 → Conflict）。
    pub async fn confirm_draft_flight(
        &self,
        flight_id: &str,
        actor_id: Option<String>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        let Some(current) = self.repo.find_by_id(flight_id).await? else {
            return Ok(None);
        };
        if !current.is_draft {
            return Err(DomainError::ValidationError(format!("航班 {flight_id} 不是 draft 状态")));
        }
        if current.flight_kind != "passenger" {
            return Err(DomainError::ValidationError(format!(
                "仅 passenger 航班支持批确认（当前 flight_kind: {}）",
                current.flight_kind
            )));
        }

        let patch = FlightUpdatePatch {
            expected_version: Some(current.version),
            is_draft: Some(false),
            ..FlightUpdatePatch::default()
        };

        let flight = if let (Some(pool), Some(tx_repo)) = (self.pool.as_ref(), self.tx_repo.as_ref()) {
            let mut tx = pool.begin().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            let Some(flight) = tx_repo.update_partial_in_tx(&mut tx, flight_id, &patch).await? else {
                return Ok(None);
            };
            write_flight_update_outbox_events(&mut tx, flight_id, &patch, actor_id.as_deref()).await?;
            tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            flight
        } else {
            let Some(flight) = self.repo.update_partial(flight_id, &patch).await? else {
                return Ok(None);
            };
            flight
        };

        self.invalidate_hot_list().await;
        let mut response = to_response(&flight);
        response.updated_by = actor_id;
        Ok(Some(response))
    }

    /// 删除航班
    pub async fn delete_flight(&self, flight_id: &str) -> Result<bool, DomainError> {
        let deleted = if let (Some(pool), Some(tx_repo)) = (self.pool.as_ref(), self.tx_repo.as_ref()) {
            let mut tx = pool.begin().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            let deleted = tx_repo.delete_in_tx(&mut tx, flight_id).await?;
            if deleted {
                write_flight_outbox_event(
                    &mut tx,
                    FLIGHT_AGGREGATE_TYPE,
                    flight_id,
                    FLIGHT_DELETED_EVENT,
                    build_deleted_payload(flight_id, None),
                )
                .await?;
            }
            tx.commit().await.map_err(|e| DomainError::Internal(e.to_string()))?;
            deleted
        } else {
            self.repo.delete(flight_id).await?
        };
        if deleted {
            self.invalidate_hot_list().await;
            self.negative_cache.remove(flight_id);
        }
        Ok(deleted)
    }

    pub async fn invalidate_hot_list(&self) {
        *self.hot_list.write().await = None;
    }

    /// 搜索航班
    pub async fn search_flights(
        &self,
        flight_no: Option<&str>,
        status: Option<&str>,
        origin: Option<&str>,
        destination: Option<&str>,
        has_open_anomaly: Option<bool>,
        page: i64,
        size: i64,
    ) -> Result<Vec<FlightResponse>, DomainError> {
        let offset = (page - 1).max(0) * size.max(1);
        let flights = self
            .repo
            .search(
                &FlightSearchCriteria {
                    flight_no: flight_no.map(str::to_string),
                    status: status.map(str::to_string),
                    origin: origin.map(str::to_string),
                    destination: destination.map(str::to_string),
                    has_open_anomaly,
                },
                size.max(1),
                offset,
            )
            .await?;
        Ok(flights.iter().map(to_response).collect())
    }

    /// KPI 快照 (stub)
    pub async fn get_kpi_snapshot(&self, _time_range: &str) -> Result<serde_json::Value, DomainError> {
        let all = self.repo.find_all(500, 0).await?;
        let total = all.len() as f64;
        let on_time = all
            .iter()
            .filter(|f| f.status == FlightStatus::Departed || f.status == FlightStatus::Arrived)
            .count() as f64;
        let otp = if total > 0.0 { on_time / total * 100.0 } else { 0.0 };
        Ok(serde_json::json!({
            "total_flights": total as i64,
            "on_time_rate": (otp * 100.0).round() / 100.0,
            "active_flights": all.iter().filter(|f| f.status == FlightStatus::Boarding || f.status == FlightStatus::Departed).count(),
            "anomaly_count": 0,
        }))
    }

    /// KPI 趋势 (stub)
    pub async fn get_kpi_trend(&self, metric: &str, days: i32) -> Result<serde_json::Value, DomainError> {
        Ok(serde_json::json!({
            "metric": metric,
            "days": days,
            "items": [],
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FLIGHT_CREATED_EVENT, FLIGHT_DELETED_EVENT, FLIGHT_LEG_UPSERTED_EVENT, FLIGHT_REMARKS_UPDATED_EVENT,
        FLIGHT_RESOURCE_UPDATED_EVENT, FLIGHT_STATUS_UPDATED_EVENT,
    };
    use fms_domain::models::flight_state::can_transition;
    use fms_domain::models::value_objects::FlightStatus;
    use fms_domain::ports::flight_repository::PatchField;

    use crate::schemas::flight_schemas::FlightUpdate;
    use crate::services::flight_command_validator::update_fields_present;
    use crate::services::flight_mappers::update_patch_from_dto;

    #[test]
    fn update_patch_from_dto_preserves_clear_semantics() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "gate": null,
            "scheduled_departure": null,
            "registration": null,
            "inbound_leg": null,
            "terminal": "T2"
        }))
        .unwrap();

        let patch = update_patch_from_dto(dto).unwrap();

        assert!(matches!(patch.gate, PatchField::Clear));
        assert!(matches!(patch.scheduled_departure, PatchField::Clear));
        assert!(matches!(patch.registration, PatchField::Clear));
        assert!(matches!(patch.inbound_leg, PatchField::Clear));
        assert!(matches!(patch.terminal, PatchField::Set(ref value) if value == "T2"));
    }

    #[test]
    fn flight_outbox_event_types_match_subscriber_contract() {
        assert_eq!(FLIGHT_CREATED_EVENT, "flight.created_v2");
        assert_eq!(FLIGHT_STATUS_UPDATED_EVENT, "flight.status_updated_v2");
        assert_eq!(FLIGHT_RESOURCE_UPDATED_EVENT, "flight.resource_updated_v2");
        assert_eq!(FLIGHT_LEG_UPSERTED_EVENT, "flight.leg_upserted_v2");
        assert_eq!(FLIGHT_REMARKS_UPDATED_EVENT, "flight.remarks_updated_v2");
        assert_eq!(FLIGHT_DELETED_EVENT, "flight.deleted_v2");
    }

    #[test]
    fn update_fields_present_tracks_status_and_resources() {
        let dto: FlightUpdate = serde_json::from_value(serde_json::json!({
            "status": "delayed",
            "gate": "A1",
            "flight_remarks": "note"
        }))
        .unwrap();
        let fields = update_fields_present(&dto);
        assert!(fields.contains(&"status"));
        assert!(fields.contains(&"gate"));
        assert!(fields.contains(&"flight_remarks"));
    }

    #[test]
    fn status_transition_allows_scheduled_to_delayed() {
        assert!(can_transition(FlightStatus::Scheduled, FlightStatus::Delayed));
        assert!(!can_transition(FlightStatus::Cancelled, FlightStatus::Scheduled));
    }
}
