use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::sync::RwLock;

use fms_domain::ports::audit_log_repository::AuditLogRepository;
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjectionRepository;
use fms_domain::ports::flight_timeline_event_repository::FlightTimelineEventRepository;

use crate::schemas::flight_schemas::DispatchTimelineEventResponse;
use crate::types::{ConcreteBusinessCaseService, ConcreteFlightService};

use crate::services::ai_runtime_service::AiRuntimeService;

use super::timeline::DispatchTimelineWriter;

pub struct FlightRuntimeService {
    pub(super) flight_service: Arc<ConcreteFlightService>,
    pub(super) business_case_service: Option<Arc<ConcreteBusinessCaseService>>,
    pub(super) projection_repo: Option<Arc<dyn FlightRuntimeProjectionRepository>>,
    pub(super) audit_log_repo: Option<Arc<dyn AuditLogRepository + Send + Sync>>,
    pub(super) timeline_repo: Option<Arc<dyn FlightTimelineEventRepository + Send + Sync>>,
    pub(super) timeline_writer: Option<Arc<dyn DispatchTimelineWriter>>,
    pub(super) ai_runtime_service: Option<Arc<AiRuntimeService>>,
    pub(super) state: RwLock<FlightRuntimeState>,
}

pub struct DispatchTimelineEventWriteResult {
    pub event: DispatchTimelineEventResponse,
    pub inserted: bool,
}

#[derive(Default)]
pub(super) struct FlightRuntimeState {
    pub(super) recent_updates: VecDeque<FlightAuditEntry>,
    pub(super) history_by_flight: HashMap<String, VecDeque<FlightAuditEntry>>,
    pub(super) timeline_by_flight: HashMap<String, VecDeque<DispatchTimelineEventResponse>>,
}

#[derive(Clone)]
pub(super) struct FlightAuditEntry {
    pub(super) flight_id: String,
    pub(super) occurred_at: DateTime<Utc>,
    pub(super) payload: Value,
}
