//! Flight runtime service: constructors, single/batch enrichment,
//! audit recording, and recent-update / per-flight history state queries.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::warn;
use ulid::Ulid;
use uuid::Uuid;

use fms_domain::error::DomainError;
use fms_domain::ports::audit_log_repository::{AuditLogEntry, AuditLogRepository, NewFlightAuditLog};
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjectionRepository;
use fms_domain::ports::flight_timeline_event_repository::FlightTimelineEventRepository;

use crate::schemas::flight_schemas::FlightResponse;
use crate::sqlx_transactional_repositories::SqlxDomainEventOutboxTransactionalRepository;
use crate::sqlx_transactional_repositories::SqlxFlightTimelineTransactionalRepository;
use crate::types::{ConcreteBusinessCaseService, ConcreteFlightService};

use super::helpers::{
    apply_projection_to_flight, apply_timeline_to_flight, evict_idle_flights, should_emit_perf_trace,
    timestamp_from_value, trim_deque, MAX_RETAINED_FLIGHTS,
};
use super::types::{FlightAuditEntry, FlightRuntimeService, FlightRuntimeState};
use crate::services::ai_runtime_service::AiRuntimeService;
use crate::services::flight_risk_service::apply_flight_risk;

static FLIGHT_RUNTIME_ENRICH_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);
static FLIGHT_RUNTIME_PROJECTION_TRACE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl FlightRuntimeService {
    pub fn new(
        pool: PgPool,
        flight_service: Arc<ConcreteFlightService>,
        outbox_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository>,
    ) -> Self {
        Self {
            pool,
            flight_service,
            business_case_service: None,
            projection_repo: None,
            audit_log_repo: None,
            timeline_repo: None,
            timeline_tx_repo: None,
            outbox_repo,
            ai_runtime_service: None,
            state: RwLock::new(FlightRuntimeState::default()),
        }
    }

    pub fn with_business_case_service(mut self, business_case_service: Arc<ConcreteBusinessCaseService>) -> Self {
        self.business_case_service = Some(business_case_service);
        self
    }

    pub fn with_projection_repository(mut self, projection_repo: Arc<dyn FlightRuntimeProjectionRepository>) -> Self {
        self.projection_repo = Some(projection_repo);
        self
    }

    pub fn with_audit_log_repository(mut self, audit_log_repo: Arc<dyn AuditLogRepository + Send + Sync>) -> Self {
        self.audit_log_repo = Some(audit_log_repo);
        self
    }

    pub fn with_timeline_repository(
        mut self,
        timeline_repo: Arc<dyn FlightTimelineEventRepository + Send + Sync>,
        timeline_tx_repo: Arc<dyn SqlxFlightTimelineTransactionalRepository>,
    ) -> Self {
        self.timeline_repo = Some(timeline_repo);
        self.timeline_tx_repo = Some(timeline_tx_repo);
        self
    }

    pub fn with_ai_runtime_service(mut self, ai_runtime_service: Arc<AiRuntimeService>) -> Self {
        self.ai_runtime_service = Some(ai_runtime_service);
        self
    }

    pub async fn initial_snapshot(&self, limit: i64) -> Result<Vec<FlightResponse>, DomainError> {
        self.initial_snapshot_for_viewer(limit, None, None).await
    }

    pub async fn initial_snapshot_for_viewer(
        &self,
        limit: i64,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightResponse>, DomainError> {
        let flights = self
            .flight_service
            .list_flights(1, limit.clamp(1, 500), None)
            .await?
            .items;
        self.enrich_flights_for_viewer(flights, viewer_department_id, viewer_department_name)
            .await
    }

    pub async fn build_cached_flight(&self, flight_id: &str) -> Result<Option<FlightResponse>, DomainError> {
        self.build_cached_flight_for_viewer(flight_id, None, None).await
    }

    pub async fn build_cached_flight_for_viewer(
        &self,
        flight_id: &str,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Option<FlightResponse>, DomainError> {
        let Some(flight) = self.flight_service.get_flight(flight_id).await? else {
            return Ok(None);
        };

        self.enrich_flight_for_viewer(flight, viewer_department_id, viewer_department_name)
            .await
            .map(Some)
    }

    pub async fn enrich_flight(&self, flight: FlightResponse) -> Result<FlightResponse, DomainError> {
        self.enrich_flight_for_viewer(flight, None, None).await
    }

    pub async fn enrich_flight_for_viewer(
        &self,
        mut flight: FlightResponse,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<FlightResponse, DomainError> {
        let Some(flight_id) = flight.flight_id.as_deref() else {
            return Ok(flight);
        };

        // Enrich business cases
        if let Some(service) = self.business_case_service.as_ref() {
            let cases = service
                .get_by_flight_for_viewer(flight_id, viewer_department_id, viewer_department_name)
                .await?;
            flight.business_cases = cases
                .into_iter()
                .filter_map(|item| serde_json::to_value(item).ok())
                .collect();
        }

        // Enrich timelines
        let mut timelines = self.fetch_timeline_snapshots(&[flight_id.to_string()]).await;
        if let Some(events) = timelines.remove(flight_id) {
            apply_timeline_to_flight(&mut flight, &events);
        }

        apply_flight_risk(&mut flight, Utc::now());
        Ok(flight)
    }

    pub async fn enrich_flights(&self, flights: Vec<FlightResponse>) -> Result<Vec<FlightResponse>, DomainError> {
        self.enrich_flights_for_viewer(flights, None, None).await
    }

    pub async fn enrich_flights_for_viewer(
        &self,
        flights: Vec<FlightResponse>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightResponse>, DomainError> {
        if self.projection_repo.is_some() {
            return self.enrich_flights_from_projection(flights).await;
        }
        self.enrich_flights_from_database(flights, viewer_department_id, viewer_department_name)
            .await
    }

    async fn enrich_flights_from_projection(
        &self,
        flights: Vec<FlightResponse>,
    ) -> Result<Vec<FlightResponse>, DomainError> {
        let Some(projection_repo) = self.projection_repo.as_ref() else {
            return self.enrich_flights_from_database(flights, None, None).await;
        };
        let trace = should_emit_perf_trace(&FLIGHT_RUNTIME_PROJECTION_TRACE_COUNTER);
        let total_start = Instant::now();
        let flight_ids = flights
            .iter()
            .filter_map(|flight| flight.flight_id.clone())
            .collect::<Vec<_>>();

        let load_start = Instant::now();
        let projection_map = projection_repo.find_by_flight_ids(&flight_ids).await?;
        let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

        let missing_ids = flight_ids
            .iter()
            .filter(|flight_id| !projection_map.contains_key(*flight_id))
            .cloned()
            .collect::<HashSet<_>>();

        let fallback_start = Instant::now();
        let fallback_map = if missing_ids.is_empty() {
            HashMap::new()
        } else {
            let missing_flights = flights
                .iter()
                .filter(|flight| {
                    flight
                        .flight_id
                        .as_ref()
                        .map(|flight_id| missing_ids.contains(flight_id))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>();
            let enriched = self.enrich_flights_from_database(missing_flights, None, None).await?;
            for flight_id in &missing_ids {
                if let Err(error) = projection_repo.rebuild_for_flight(flight_id).await {
                    warn!(
                        flight_id = %flight_id,
                        error = %error,
                        "failed to rebuild flight runtime projection after fallback"
                    );
                }
            }
            enriched
                .into_iter()
                .filter_map(|flight| flight.flight_id.clone().map(|flight_id| (flight_id, flight)))
                .collect()
        };
        let fallback_ms = fallback_start.elapsed().as_secs_f64() * 1000.0;

        let merge_start = Instant::now();
        let mut items = Vec::with_capacity(flights.len());
        for mut flight in flights {
            if let Some(flight_id) = flight.flight_id.as_ref() {
                if let Some(fallback) = fallback_map.get(flight_id) {
                    items.push(fallback.clone());
                    continue;
                }
                if let Some(projection) = projection_map.get(flight_id) {
                    apply_projection_to_flight(&mut flight, projection);
                }
            }
            apply_flight_risk(&mut flight, Utc::now());
            items.push(flight);
        }
        let merge_ms = merge_start.elapsed().as_secs_f64() * 1000.0;

        if trace {
            tracing::info!(
                target: "fms_perf",
                event = "flights_runtime_projection_load",
                items = items.len(),
                flight_ids = flight_ids.len(),
                projections = projection_map.len(),
                missing = missing_ids.len(),
                load_ms,
                fallback_ms,
                merge_ms,
                total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
            );
            tracing::info!(
                target: "fms_perf",
                event = "flights_runtime_projection_merge",
                items = items.len(),
                merge_ms,
            );
            if !missing_ids.is_empty() {
                tracing::info!(
                    target: "fms_perf",
                    event = "flights_runtime_projection_fallback",
                    missing = missing_ids.len(),
                    fallback_ms,
                );
            }
        }

        Ok(items)
    }

    async fn enrich_flights_from_database(
        &self,
        flights: Vec<FlightResponse>,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightResponse>, DomainError> {
        let trace = should_emit_perf_trace(&FLIGHT_RUNTIME_ENRICH_TRACE_COUNTER);
        let total_start = Instant::now();
        let flight_ids = flights.iter().filter_map(|f| f.flight_id.clone()).collect::<Vec<_>>();

        let timeline_start = Instant::now();
        let mut timeline_map = self.fetch_timeline_snapshots(&flight_ids).await;
        let timeline_ms = timeline_start.elapsed().as_secs_f64() * 1000.0;
        let timeline_entries = timeline_map.len();
        let business_case_start = Instant::now();
        let mut business_case_map = match self.business_case_service.as_ref() {
            Some(service) => {
                service
                    .get_by_flight_ids_for_viewer(&flight_ids, viewer_department_id, viewer_department_name)
                    .await?
            }
            None => HashMap::new(),
        };
        let business_case_ms = business_case_start.elapsed().as_secs_f64() * 1000.0;
        let business_case_entries = business_case_map.len();

        let merge_start = Instant::now();
        let mut items = Vec::with_capacity(flights.len());
        for mut flight in flights {
            if let Some(flight_id) = flight.flight_id.as_deref() {
                let cases = business_case_map.remove(flight_id).unwrap_or_default();
                flight.business_cases = cases
                    .into_iter()
                    .filter_map(|item| serde_json::to_value(item).ok())
                    .collect();

                if let Some(events) = timeline_map.remove(flight_id) {
                    apply_timeline_to_flight(&mut flight, &events);
                }
            }
            apply_flight_risk(&mut flight, Utc::now());
            items.push(flight);
        }
        let merge_ms = merge_start.elapsed().as_secs_f64() * 1000.0;
        if trace {
            tracing::info!(
                target: "fms_perf",
                event = "flights_runtime_enrich",
                items = items.len(),
                flight_ids = flight_ids.len(),
                timeline_entries,
                business_case_entries,
                timeline_ms,
                business_case_ms,
                merge_risk_ms = merge_ms,
                total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
            );
        }
        Ok(items)
    }

    pub(super) async fn refresh_projection_for_flight(&self, flight_id: &str) {
        if let Some(projection_repo) = self.projection_repo.as_ref() {
            if let Err(error) = projection_repo.rebuild_for_flight(flight_id).await {
                warn!(
                    flight_id = %flight_id,
                    error = %error,
                    "failed to refresh flight runtime list projection"
                );
                if let Err(delete_error) = projection_repo.delete_for_flight(flight_id).await {
                    warn!(
                        flight_id = %flight_id,
                        error = %delete_error,
                        "failed to delete stale flight runtime list projection after refresh failure"
                    );
                }
            }
        }
    }

    pub async fn record_created(&self, actor: &str, flight: &FlightResponse) -> Value {
        let changes = json!({
            "fields": non_null_field_names(response_to_map(flight)),
            "old": {},
            "new": response_to_map(flight),
        });
        self.record_audit_entry("create", actor, flight, changes).await
    }

    pub async fn record_updated(
        &self,
        actor: &str,
        before: Option<&FlightResponse>,
        after: &FlightResponse,
        hinted_fields: &[String],
    ) -> Value {
        let before_map = before.map(response_to_map).unwrap_or_default();
        let after_map = response_to_map(after);
        let fields = changed_fields(&before_map, &after_map, hinted_fields);
        let changes = json!({
            "fields": fields.clone(),
            "old": project_map(&before_map, &fields),
            "new": project_map(&after_map, &fields),
        });
        self.record_audit_entry("update", actor, after, changes).await
    }

    pub async fn record_deleted(&self, actor: &str, before: Option<&FlightResponse>) -> Option<Value> {
        let flight = before?;
        let snapshot = response_to_map(flight);
        let changes = json!({
            "fields": non_null_field_names(snapshot.clone()),
            "old": snapshot,
            "new": {},
        });
        Some(self.record_audit_entry("delete", actor, flight, changes).await)
    }

    pub async fn get_recent_flight_updates(&self, minutes: i64, limit: usize) -> Result<Vec<Value>, DomainError> {
        let threshold = Utc::now() - Duration::minutes(minutes.max(1));
        let mut merged = self.query_recent_updates_from_db(threshold, limit).await;
        let state = self.state.read().await;
        for entry in &state.recent_updates {
            if entry.occurred_at >= threshold {
                merged.push(entry.payload.clone());
            }
        }
        Ok(sort_and_limit_updates(merged, limit))
    }

    pub async fn get_flight_update_history(
        &self,
        flight_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<Value>, DomainError> {
        let bounded_page = page.max(1);
        let bounded_size = page_size.clamp(1, 200);
        let mut merged = self.query_flight_history_from_db(flight_id).await;
        let state = self.state.read().await;
        if let Some(entries) = state.history_by_flight.get(flight_id) {
            merged.extend(entries.iter().map(|entry| entry.payload.clone()));
        }
        let sorted = sort_and_limit_updates(merged, usize::MAX);
        let offset = (bounded_page - 1) * bounded_size;
        Ok(sorted.into_iter().skip(offset).take(bounded_size).collect())
    }

    async fn record_audit_entry(&self, action: &str, actor: &str, flight: &FlightResponse, changes: Value) -> Value {
        let occurred_at = Utc::now();
        let flight_id = flight.flight_id.clone().unwrap_or_else(|| "unknown-flight".to_string());
        let entity_id = flight_id.clone();
        let entry_id = Uuid::new_v4();
        let trace_id = "";
        let changes_for_insert = changes.clone();
        let payload = json!({
            "id": entry_id.to_string(),
            "entity_type": "flight",
            "entity_id": flight_id,
            "operation": action,
            "action": action,
            "changes": changes,
            "user_id": actor,
            "trace_id": trace_id,
            "timestamp": occurred_at.to_rfc3339(),
            "created_at": occurred_at.to_rfc3339(),
        });
        if let Some(audit_log_repo) = self.audit_log_repo.as_ref() {
            if let Err(error) = audit_log_repo
                .insert_flight_audit(&NewFlightAuditLog {
                    id: entry_id,
                    entity_id: entity_id.clone(),
                    action: action.to_string(),
                    changes: changes_for_insert,
                    user_id: actor.to_string(),
                    trace_id: trace_id.to_string(),
                    created_at: occurred_at,
                })
                .await
            {
                warn!(
                    audit_id = %entry_id,
                    flight_id = %entity_id,
                    action = %action,
                    error = %error,
                    "failed to persist flight audit entry"
                );
            }
        }
        let entry = FlightAuditEntry {
            flight_id: entity_id,
            occurred_at,
            payload: payload.clone(),
        };

        let mut state = self.state.write().await;
        state.recent_updates.push_front(entry.clone());
        trim_deque(&mut state.recent_updates, 2000);

        let queue = state.history_by_flight.entry(entry.flight_id.clone()).or_default();
        queue.push_front(entry);
        trim_deque(queue, 500);

        // Evict idle flights to cap resident state
        if state.history_by_flight.len() > MAX_RETAINED_FLIGHTS {
            evict_idle_flights(&mut state);
        }

        payload
    }

    async fn query_recent_updates_from_db(&self, threshold: DateTime<Utc>, limit: usize) -> Vec<Value> {
        let Some(audit_log_repo) = self.audit_log_repo.as_ref() else {
            return Vec::new();
        };
        match audit_log_repo.list_recent_flight_updates(threshold, limit as i64).await {
            Ok(rows) => rows.iter().map(audit_entry_to_value).collect(),
            Err(_) => Vec::new(),
        }
    }

    async fn query_flight_history_from_db(&self, flight_id: &str) -> Vec<Value> {
        let Some(audit_log_repo) = self.audit_log_repo.as_ref() else {
            return Vec::new();
        };
        match audit_log_repo.list_flight_history(flight_id, 500).await {
            Ok(rows) => rows.iter().map(audit_entry_to_value).collect(),
            Err(_) => Vec::new(),
        }
    }

    async fn fetch_timeline_snapshots(&self, flight_ids: &[String]) -> HashMap<String, HashMap<String, DateTime<Utc>>> {
        let Some(timeline_repo) = self.timeline_repo.as_ref() else {
            return HashMap::new();
        };
        timeline_repo.latest_snapshots(flight_ids).await.unwrap_or_default()
    }
}

fn audit_entry_to_value(entry: &AuditLogEntry) -> Value {
    json!({
        "id": if entry.id.is_empty() { Ulid::new().to_string() } else { entry.id.clone() },
        "entity_type": entry.entity_type,
        "entity_id": entry.entity_id,
        "operation": entry.action,
        "action": entry.action,
        "changes": entry.changes,
        "user_id": entry.user_id.clone().unwrap_or_default(),
        "trace_id": entry.trace_id.clone().unwrap_or_default(),
        "timestamp": entry.created_at.map(|ts| ts.to_rfc3339()).unwrap_or_default(),
        "created_at": entry.created_at.map(|ts| ts.to_rfc3339()).unwrap_or_default(),
    })
}

fn response_to_map(response: &FlightResponse) -> Map<String, Value> {
    serde_json::to_value(response)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn non_null_field_names(map: Map<String, Value>) -> Vec<String> {
    map.into_iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, _)| key)
        .collect()
}

fn changed_fields(before: &Map<String, Value>, after: &Map<String, Value>, hinted_fields: &[String]) -> Vec<String> {
    let hinted = hinted_fields
        .iter()
        .map(|field| field.trim())
        .filter(|field| !field.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !hinted.is_empty() {
        return hinted;
    }

    let mut keys = before.keys().chain(after.keys()).cloned().collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| before.get(key) != after.get(key))
        .collect()
}

fn project_map(source: &Map<String, Value>, fields: &[String]) -> Value {
    let mut map = Map::new();
    let null_value = serde_json::Value::Null;
    for field in fields {
        map.insert(field.clone(), source.get(field).unwrap_or(&null_value).clone());
    }
    Value::Object(map)
}

fn sort_and_limit_updates(items: Vec<Value>, limit: usize) -> Vec<Value> {
    let mut dedup = HashMap::<String, Value>::new();
    for item in items {
        let key = item
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| Ulid::new().to_string());
        dedup.entry(key).or_insert(item);
    }
    let mut values = dedup.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| timestamp_from_value(right).cmp(&timestamp_from_value(left)));
    if limit == usize::MAX {
        return values;
    }
    values.into_iter().take(limit).collect()
}
