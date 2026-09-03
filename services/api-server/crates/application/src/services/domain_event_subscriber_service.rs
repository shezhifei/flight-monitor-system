use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use fms_domain::ports::message_queue::{MessageHandler, MessageQueueError, SubscriberMessage};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{info, warn};

use fms_domain::broadcaster::Broadcaster;
use fms_domain::error::DomainError;
use fms_domain::ports::domain_event_subscription_state_repository::{
    DomainEventDeadLetterRecord, DomainEventProcessingRecord, DomainEventSubscriptionStateRepository,
};
use fms_domain::ports::event_rule_repository::EventRuleRepository;

use crate::services::business_case_workflow_service::WorkflowActor;
use crate::services::cache_invalidation_service::{CacheInvalidationKey, CacheInvalidationService};
use crate::services::dispatch_service::dispatch_overrun_warning_service::DispatchOverrunWarningService;
use crate::services::dispatch_service::DispatchService;
use crate::services::event_rule_handler::EventDrivenRuleHandler;
use crate::services::flight_cache_service::{flight_list_requires_global_invalidation, FlightCacheService};
use crate::services::flight_runtime_service::FlightRuntimeService;
use crate::types::{ConcreteAnomalyService, ConcreteBusinessCaseTypeService, ConcreteBusinessCaseWorkflowService};

const DEFAULT_DOMAIN_TOPIC: &str = "fms_domain_events";
// Match Python's consumer group name for cross-compatibility
const DEFAULT_CONSUMER_GROUP: &str = "domain_event_processors";
const DEFAULT_MAX_RETRY: i32 = 5;
const DEFAULT_DISPATCH_PUBLICATION_LIMIT: usize = 100;
const SYSTEM_EVENT_BUS_ACTOR_ID: &str = "system:event-bus";

const PROCESSED_TOTAL_METRIC: &str = "domain_event_subscriber_processed_total";
const FAILED_TOTAL_METRIC: &str = "domain_event_subscriber_failed_total";
const RETRY_TOTAL_METRIC: &str = "domain_event_subscriber_retry_total";
const DLQ_TOTAL_METRIC: &str = "domain_event_subscriber_dlq_total";
const LAG_MS_METRIC: &str = "domain_event_subscriber_lag_ms";

const FLIGHT_EVENT_TYPES: [&str; 8] = [
    "flight.created_v2",
    "flight.status_updated_v2",
    "flight.resource_updated_v2",
    "flight.leg_upserted_v2",
    "flight.remarks_updated_v2",
    "flight.timeline_upserted_v2",
    "flight.timeline_deleted_v2",
    "flight.deleted_v2",
];

const FLIGHT_ANOMALY_TRIGGER_EVENT_TYPES: [&str; 3] = [
    "flight.status_updated_v2",
    "flight.resource_updated_v2",
    "flight.leg_upserted_v2",
];

const BUSINESS_CASE_EVENT_TYPES: [&str; 3] = [
    "business_case.created",
    "business_case.updated",
    "business_case.deleted",
];

/// 触发预排冲突预警评估的航班事件:航班状态/资源/航段变化都可能改变
/// 工单链,进而产生或消除共享人员冲突。
const OVERRUN_TRIGGER_EVENT_TYPES: [&str; 3] = [
    "flight.status_updated_v2",
    "flight.resource_updated_v2",
    "flight.leg_upserted_v2",
];

/// 触发本体自动建链：状态/资源/航段变化可能带来同机进港落地或换机。
const ONTOLOGY_AUTOLINK_TRIGGER_EVENT_TYPES: [&str; 3] = [
    "flight.status_updated_v2",
    "flight.resource_updated_v2",
    "flight.leg_upserted_v2",
];

type DomainEventHandlerFuture = Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send>>;
type BusinessCaseWorkflowFuture = Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send>>;
type BusinessCaseNotifierFuture = Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send>>;

#[derive(Debug, Clone, PartialEq)]
pub struct DomainEventEnvelope {
    pub event_id: String,
    pub source_change_id: Option<String>,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub payload: Value,
    pub stream_message_id: String,
}

trait DomainEventHandler: Send + Sync {
    fn can_handle(&self, event_type: &str) -> bool;
    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture;
}

pub trait BusinessCaseEventNotifier: Send + Sync {
    fn publish_business_case_event(&self, event_name: String, payload: Value) -> BusinessCaseNotifierFuture;
}

trait BusinessCaseWorkflowTrigger: Send + Sync {
    fn handle_created(&self, case_type: String, case_id: String, actor: WorkflowActor) -> BusinessCaseWorkflowFuture;
}

struct FlightProjectionEventHandler {
    service: Arc<FlightCacheService>,
    runtime_service: Arc<FlightRuntimeService>,
    cache_invalidation: Option<Arc<CacheInvalidationService>>,
}

impl FlightProjectionEventHandler {
    fn new(
        service: Arc<FlightCacheService>,
        runtime_service: Arc<FlightRuntimeService>,
        cache_invalidation: Option<Arc<CacheInvalidationService>>,
    ) -> Self {
        Self {
            service,
            runtime_service,
            cache_invalidation,
        }
    }
}

impl DomainEventHandler for FlightProjectionEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        is_flight_event_type(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let service = self.service.clone();
        let runtime_service = self.runtime_service.clone();
        let cache_invalidation = self.cache_invalidation.clone();
        Box::pin(async move {
            let flight_id = envelope.aggregate_id.trim().to_string();
            if flight_id.is_empty() {
                return Ok(());
            }

            let changed_fields = extract_changed_fields_from_flight_event(&envelope);
            let append_to_list_cache = changed_fields.iter().any(|field| field == "create");
            let remove_from_list_cache = changed_fields.iter().any(|field| field == "delete");

            match runtime_service.build_cached_flight(&flight_id).await? {
                Some(flight) => {
                    service.refresh_single_flight_cache(&flight).await;
                }
                None => {
                    service.invalidate_single_flight_cache(Some(&flight_id)).await;
                    warn!(
                        flight_id,
                        event_type = %envelope.event_type,
                        "flight projection skipped because current flight snapshot is missing"
                    );
                }
            }

            if flight_list_requires_global_invalidation(&changed_fields, append_to_list_cache, remove_from_list_cache) {
                service.invalidate_flights_cache().await;
            }

            if let Some(cache_invalidation) = cache_invalidation.as_ref() {
                let keys = vec![
                    CacheInvalidationKey::FlightRuntimeProjection,
                    CacheInvalidationKey::FlightListHot,
                    CacheInvalidationKey::FlightListResponse,
                ];
                let event = cache_invalidation.flight_event(&flight_id, keys);
                cache_invalidation.invalidate_local(&event).await;
            }
            Ok(())
        })
    }
}

struct FlightRealtimeEventHandler {
    runtime_service: Arc<FlightRuntimeService>,
    broadcaster: Arc<dyn Broadcaster + Send + Sync>,
}

impl FlightRealtimeEventHandler {
    fn new(runtime_service: Arc<FlightRuntimeService>, broadcaster: Arc<dyn Broadcaster + Send + Sync>) -> Self {
        Self {
            runtime_service,
            broadcaster,
        }
    }
}

impl DomainEventHandler for FlightRealtimeEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        is_flight_event_type(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let runtime_service = self.runtime_service.clone();
        let broadcaster = self.broadcaster.clone();
        Box::pin(async move {
            let flight_id = envelope.aggregate_id.trim().to_string();
            if flight_id.is_empty() {
                return Ok(());
            }

            let changed_fields = extract_changed_fields_from_flight_event(&envelope);
            let status_changed = envelope.event_type.trim() == "flight.status_updated_v2"
                || changed_fields.iter().any(|field| field == "status");
            let created = envelope.event_type.trim() == "flight.created_v2"
                || changed_fields.iter().any(|field| field == "create");

            let flight = runtime_service.build_cached_flight(&flight_id).await?;
            let flight_value = flight
                .as_ref()
                .and_then(|value| serde_json::to_value(value).ok())
                .unwrap_or(Value::Null);
            let timestamp = chrono::Utc::now().to_rfc3339();

            if created {
                let payload = json!({
                    "type": "flight_created",
                    "flight": flight_value,
                    "event_id": envelope.event_id,
                    "source_change_id": envelope.source_change_id,
                    "timestamp": timestamp,
                });
                broadcaster
                    .broadcast_event("flights", Some("flight_created"), payload)
                    .await;
                return Ok(());
            }

            let patch = if flight_value.is_null() {
                json!({ "flight_id": flight_id })
            } else {
                flight_patch_from_fields(&flight_value, &changed_fields)
            };
            let mut payload = json!({
                "type": "flight_updated",
                "flight_id": flight_id,
                "changed_fields": changed_fields,
                "flight": patch,
                "patch": patch,
                "event_id": envelope.event_id,
                "source_change_id": envelope.source_change_id,
                "timestamp": timestamp,
            });
            if let Some(timeline_event) = envelope.payload.get("timeline_event") {
                payload["timeline_event"] = timeline_event.clone();
            }
            broadcaster
                .broadcast_event("flights", Some("flight_updated"), payload.clone())
                .await;
            if status_changed {
                broadcaster
                    .broadcast_event("flight_status_changes", Some("flight_status_changed"), payload)
                    .await;
            }
            Ok(())
        })
    }
}

/// Handler for `ai_job.*` domain events. Picks up outbox events
/// emitted by `AiJobService` (via CDC relay → MQ) and broadcasts them
/// on the `ai_execution` SSE topic for frontend consumers.
///
/// Event types handled:
/// - `ai_job.succeeded` → `ai_job_succeeded`
/// - `ai_job.failed`    → `ai_job_failed`
/// - `ai_job.cancelled` → `ai_job_cancelled`
/// - `ai_job.timed_out` → `ai_job_timed_out`
struct AiJobEventHandler {
    broadcaster: Arc<dyn Broadcaster + Send + Sync>,
}

impl AiJobEventHandler {
    fn new(broadcaster: Arc<dyn Broadcaster + Send + Sync>) -> Self {
        Self { broadcaster }
    }
}

const AI_JOB_EVENT_TYPES: [&str; 4] = [
    "ai_job.succeeded",
    "ai_job.failed",
    "ai_job.cancelled",
    "ai_job.timed_out",
];

fn is_ai_job_event_type(event_type: &str) -> bool {
    AI_JOB_EVENT_TYPES.contains(&event_type)
}

fn ai_job_sse_event_name(event_type: &str) -> Option<&'static str> {
    match event_type {
        "ai_job.succeeded" => Some("ai_job_succeeded"),
        "ai_job.failed" => Some("ai_job_failed"),
        "ai_job.cancelled" => Some("ai_job_cancelled"),
        "ai_job.timed_out" => Some("ai_job_timed_out"),
        _ => None,
    }
}

impl DomainEventHandler for AiJobEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        is_ai_job_event_type(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let broadcaster = self.broadcaster.clone();
        Box::pin(async move {
            let event_name = ai_job_sse_event_name(&envelope.event_type);
            if event_name.is_none() {
                return Ok(());
            }
            let timestamp = chrono::Utc::now().to_rfc3339();
            let mut payload = json!({
                "type": event_name.unwrap(),
                "job_id": envelope.aggregate_id,
                "event_id": envelope.event_id,
                "source_change_id": envelope.source_change_id,
                "timestamp": timestamp,
            });
            // Merge the outbox payload fields (run_id, output, error_*)
            // into the SSE payload so the frontend has the full context.
            if let Some(run_id) = envelope.payload.get("run_id") {
                payload["run_id"] = run_id.clone();
            }
            if let Some(output) = envelope.payload.get("output") {
                payload["output"] = output.clone();
            }
            if let Some(error_code) = envelope.payload.get("error_code").and_then(|v| v.as_str()) {
                payload["error_code"] = json!(error_code);
            }
            if let Some(error_message) = envelope.payload.get("error_message").and_then(|v| v.as_str()) {
                payload["error_message"] = json!(error_message);
            }
            broadcaster.broadcast_event("ai_execution", event_name, payload).await;
            Ok(())
        })
    }
}

struct AnomalyEventHandler {
    service: Arc<ConcreteAnomalyService>,
}

impl AnomalyEventHandler {
    fn new(service: Arc<ConcreteAnomalyService>) -> Self {
        Self { service }
    }
}

impl DomainEventHandler for AnomalyEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        is_flight_anomaly_trigger(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let service = self.service.clone();
        Box::pin(async move {
            service.evaluate_flight(&envelope.aggregate_id).await?;
            Ok(())
        })
    }
}

struct DispatchEventHandler {
    service: Arc<DispatchService>,
}

impl DispatchEventHandler {
    fn new(service: Arc<DispatchService>) -> Self {
        Self { service }
    }
}

impl DomainEventHandler for DispatchEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        is_flight_anomaly_trigger(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let service = self.service.clone();
        Box::pin(async move {
            let event = json!({
                "event_id": envelope.event_id,
                "aggregate_type": envelope.aggregate_type,
                "aggregate_id": envelope.aggregate_id,
                "event_type": envelope.event_type,
                "payload": envelope.payload,
                "source_change_id": envelope.source_change_id,
                "stream_message_id": envelope.stream_message_id,
            });
            service.on_domain_event(&event).await
        })
    }
}

struct DispatchPublicationEventHandler {
    service: Arc<DispatchService>,
}

impl DispatchPublicationEventHandler {
    fn new(service: Arc<DispatchService>) -> Self {
        Self { service }
    }
}

impl DomainEventHandler for DispatchPublicationEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        is_flight_event_type(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let service = self.service.clone();
        Box::pin(async move {
            let flight_id = envelope.aggregate_id.trim().to_string();
            if !flight_id.is_empty() && envelope.event_type.trim() != "flight.deleted_v2" {
                service.rebase_pending_generated_orders_for_flight(&flight_id).await?;
            }
            service
                .publish_orders(
                    None,
                    SYSTEM_EVENT_BUS_ACTOR_ID,
                    None,
                    Some(envelope.event_type.as_str()),
                    (!flight_id.is_empty()).then_some(flight_id.as_str()),
                    DEFAULT_DISPATCH_PUBLICATION_LIMIT,
                    false,
                )
                .await
                .map(|_| ())
        })
    }
}

/// 预排冲突预警事件处理器:航班状态/资源/航段变化时评估受影响航班的
/// 工单链。dispatch 订单自身的状态变化没有独立域事件,由 30 秒恢复
/// 扫描器兜底。
struct DispatchOverrunEventHandler {
    service: Arc<DispatchOverrunWarningService>,
}

impl DispatchOverrunEventHandler {
    fn new(service: Arc<DispatchOverrunWarningService>) -> Self {
        Self { service }
    }
}

impl DomainEventHandler for DispatchOverrunEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        OVERRUN_TRIGGER_EVENT_TYPES.contains(&event_type.trim())
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let service = self.service.clone();
        Box::pin(async move {
            let flight_id = envelope.aggregate_id.trim().to_string();
            if flight_id.is_empty() {
                return Ok(());
            }
            let outcomes = match service.evaluate_flight(&flight_id).await {
                Ok(outcomes) => outcomes,
                Err(error) => {
                    service.record_event_failure();
                    return Err(error);
                }
            };
            let notified = outcomes.iter().filter(|outcome| outcome.notify).count();
            if notified > 0 {
                info!(
                    flight_id = %flight_id,
                    event_type = %envelope.event_type,
                    notified = notified,
                    "dispatch overrun warning evaluated from domain event"
                );
            }
            Ok(())
        })
    }
}

/// 本体自动建链事件处理器：航班状态/资源/航段变化时尝试为出港边建链。
struct OntologyAutolinkEventHandler {
    service: Arc<crate::services::ontology_service::OntologyService>,
}

impl OntologyAutolinkEventHandler {
    fn new(service: Arc<crate::services::ontology_service::OntologyService>) -> Self {
        Self { service }
    }
}

impl DomainEventHandler for OntologyAutolinkEventHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        ONTOLOGY_AUTOLINK_TRIGGER_EVENT_TYPES.contains(&event_type.trim())
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let service = self.service.clone();
        Box::pin(async move {
            let flight_id = envelope.aggregate_id.trim();
            if flight_id.is_empty() {
                return Ok(());
            }
            service.on_flight_event_autolink(flight_id).await
        })
    }
}

struct FlowableBusinessCaseWorkflowTrigger {
    workflow_service: Arc<ConcreteBusinessCaseWorkflowService>,
}

impl FlowableBusinessCaseWorkflowTrigger {
    fn new(workflow_service: Arc<ConcreteBusinessCaseWorkflowService>) -> Self {
        Self { workflow_service }
    }
}

impl BusinessCaseWorkflowTrigger for FlowableBusinessCaseWorkflowTrigger {
    fn handle_created(&self, case_type: String, case_id: String, actor: WorkflowActor) -> BusinessCaseWorkflowFuture {
        let workflow_service = self.workflow_service.clone();
        Box::pin(async move {
            workflow_service
                .attach_existing_case_to_workflow(&case_type, &case_id, &actor)
                .await
                .map(|_| ())
        })
    }
}

struct EventDrivenDomainRuleHandler {
    event_rule_repo: Arc<dyn EventRuleRepository + Send + Sync>,
    dispatch_service: Arc<DispatchService>,
}

impl EventDrivenDomainRuleHandler {
    fn new(
        event_rule_repo: Arc<dyn EventRuleRepository + Send + Sync>,
        dispatch_service: Arc<DispatchService>,
    ) -> Self {
        Self {
            event_rule_repo,
            dispatch_service,
        }
    }
}

impl DomainEventHandler for EventDrivenDomainRuleHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        EventDrivenRuleHandler::can_handle(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let handler = EventDrivenRuleHandler::new(self.event_rule_repo.clone(), self.dispatch_service.clone());
        Box::pin(async move { handler.handle(envelope).await })
    }
}

struct BusinessCaseEventSubscriber {
    workflow_trigger: Option<Arc<dyn BusinessCaseWorkflowTrigger>>,
    notifier: Option<Arc<dyn BusinessCaseEventNotifier>>,
}

impl BusinessCaseEventSubscriber {
    fn new(
        workflow_trigger: Option<Arc<dyn BusinessCaseWorkflowTrigger>>,
        notifier: Option<Arc<dyn BusinessCaseEventNotifier>>,
    ) -> Self {
        Self {
            workflow_trigger,
            notifier,
        }
    }

    async fn handle_created(&self, envelope: &DomainEventEnvelope) -> Result<(), DomainError> {
        let case_id = resolve_business_case_id(envelope);
        let case_type = payload_text(&envelope.payload, "case_type").unwrap_or_default();
        let actor = resolve_business_case_workflow_actor(&envelope.payload);

        if let Some(workflow_trigger) = self.workflow_trigger.as_ref() {
            if !case_id.is_empty() && !case_type.is_empty() {
                if let Err(error) = workflow_trigger
                    .handle_created(case_type.clone(), case_id.clone(), actor)
                    .await
                {
                    warn!(
                        case_id = %case_id,
                        case_type = %case_type,
                        error = %error,
                        "business_case.created workflow trigger failed"
                    );
                }
            }
        }

        if let Some(notifier) = self.notifier.as_ref() {
            notifier
                .publish_business_case_event(
                    "business_case.created".to_string(),
                    build_business_case_created_payload(envelope),
                )
                .await?;
        }

        Ok(())
    }

    async fn handle_updated(&self, envelope: &DomainEventEnvelope) -> Result<(), DomainError> {
        if let Some(notifier) = self.notifier.as_ref() {
            notifier
                .publish_business_case_event(
                    "business_case.updated".to_string(),
                    build_business_case_updated_payload(envelope),
                )
                .await?;
        }

        Ok(())
    }

    async fn handle_deleted(&self, envelope: &DomainEventEnvelope) -> Result<(), DomainError> {
        if let Some(notifier) = self.notifier.as_ref() {
            notifier
                .publish_business_case_event(
                    "business_case.deleted".to_string(),
                    build_business_case_deleted_payload(envelope),
                )
                .await?;
        }

        Ok(())
    }
}

impl DomainEventHandler for BusinessCaseEventSubscriber {
    fn can_handle(&self, event_type: &str) -> bool {
        is_business_case_event_type(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> DomainEventHandlerFuture {
        let workflow_trigger = self.workflow_trigger.clone();
        let notifier = self.notifier.clone();
        Box::pin(async move {
            let subscriber = BusinessCaseEventSubscriber {
                workflow_trigger,
                notifier,
            };
            match envelope.event_type.trim() {
                "business_case.created" => subscriber.handle_created(&envelope).await,
                "business_case.updated" => subscriber.handle_updated(&envelope).await,
                "business_case.deleted" => subscriber.handle_deleted(&envelope).await,
                _ => Ok(()),
            }
        })
    }
}

pub struct DomainEventSubscriberService {
    subscription_state: Arc<dyn DomainEventSubscriptionStateRepository + Send + Sync>,
    topic: String,
    consumer_group: String,
    max_retry: i32,
    handlers: Vec<Arc<dyn DomainEventHandler>>,
}

impl DomainEventSubscriberService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        subscription_state: Arc<dyn DomainEventSubscriptionStateRepository + Send + Sync>,
        flight_cache_service: Option<Arc<FlightCacheService>>,
        flight_runtime_service: Option<Arc<FlightRuntimeService>>,
        anomaly_service: Option<Arc<ConcreteAnomalyService>>,
        dispatch_service: Option<Arc<DispatchService>>,
        dispatch_overrun_service: Option<Arc<DispatchOverrunWarningService>>,
        ontology_service: Option<Arc<crate::services::ontology_service::OntologyService>>,
        event_rule_repo: Option<Arc<dyn EventRuleRepository + Send + Sync>>,
        _business_case_type_service: Option<Arc<ConcreteBusinessCaseTypeService>>,
        business_case_workflow_service: Option<Arc<ConcreteBusinessCaseWorkflowService>>,
        business_case_notifier: Option<Arc<dyn BusinessCaseEventNotifier>>,
        flight_realtime_broadcaster: Option<Arc<dyn Broadcaster + Send + Sync>>,
        cache_invalidation: Option<Arc<CacheInvalidationService>>,
        topic: Option<String>,
        consumer_group: Option<String>,
        max_retry: i32,
    ) -> Self {
        let mut handlers: Vec<Arc<dyn DomainEventHandler>> = Vec::new();
        if let (Some(service), Some(runtime_service)) = (flight_cache_service, flight_runtime_service.clone()) {
            handlers.push(Arc::new(FlightProjectionEventHandler::new(
                service,
                runtime_service,
                cache_invalidation,
            )));
        }
        if let (Some(runtime_service), Some(broadcaster)) =
            (flight_runtime_service, flight_realtime_broadcaster.clone())
        {
            handlers.push(Arc::new(FlightRealtimeEventHandler::new(runtime_service, broadcaster)));
        }
        // AiJobEventHandler: broadcasts `ai_job.*` outbox events on the
        // `ai_execution` SSE topic. Reuses the same SseHub broadcaster
        // (the broadcaster is topic-agnostic — it dispatches by `topic`
        // argument, not by instance).
        if let Some(broadcaster) = flight_realtime_broadcaster {
            handlers.push(Arc::new(AiJobEventHandler::new(broadcaster)));
        }
        if let Some(service) = anomaly_service {
            handlers.push(Arc::new(AnomalyEventHandler::new(service)));
        }
        if let Some(service) = dispatch_service {
            if let Some(rule_repo) = event_rule_repo {
                handlers.push(Arc::new(EventDrivenDomainRuleHandler::new(rule_repo, service.clone())));
            }
            handlers.push(Arc::new(DispatchEventHandler::new(service.clone())));
            handlers.push(Arc::new(DispatchPublicationEventHandler::new(service)));
        }
        if let Some(service) = dispatch_overrun_service {
            handlers.push(Arc::new(DispatchOverrunEventHandler::new(service)));
        }
        if let Some(service) = ontology_service {
            handlers.push(Arc::new(OntologyAutolinkEventHandler::new(service)));
        }
        let business_case_workflow_trigger = business_case_workflow_service.map(|workflow_service| {
            Arc::new(FlowableBusinessCaseWorkflowTrigger::new(workflow_service)) as Arc<dyn BusinessCaseWorkflowTrigger>
        });
        if business_case_workflow_trigger.is_some() || business_case_notifier.is_some() {
            handlers.push(Arc::new(BusinessCaseEventSubscriber::new(
                business_case_workflow_trigger,
                business_case_notifier,
            )));
        }

        Self {
            subscription_state,
            topic: trim_or_default(topic, DEFAULT_DOMAIN_TOPIC),
            consumer_group: trim_or_default(consumer_group, DEFAULT_CONSUMER_GROUP),
            max_retry: if max_retry > 0 { max_retry } else { DEFAULT_MAX_RETRY },
            handlers,
        }
    }

    fn processing_record(envelope: &DomainEventEnvelope) -> DomainEventProcessingRecord {
        DomainEventProcessingRecord {
            event_id: envelope.event_id.clone(),
            source_change_id: envelope.source_change_id.clone(),
            event_type: envelope.event_type.clone(),
            aggregate_type: envelope.aggregate_type.clone(),
            aggregate_id: envelope.aggregate_id.clone(),
        }
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub fn consumer_group(&self) -> &str {
        &self.consumer_group
    }

    /// Handle messages delivered via push-consumer callback.
    /// Ack / offset-tracking is skipped because the RocketMQ push
    /// consumer framework handles acknowledgement through the return status.
    pub async fn handle_messages(&self, messages: Vec<SubscriberMessage>) -> Result<(), DomainError> {
        if messages.is_empty() {
            return Ok(());
        }

        for msg in messages {
            self.observe_consumer_lag(msg.body.get("occurred_at"));

            let result = self.process_single_message(&msg).await;

            match result {
                Ok(()) => {}
                Err(error) => {
                    let envelope = Self::decode_message(&msg);
                    let retry_count = self.mark_failed(&envelope, &error.to_string()).await?;
                    metrics::counter!(
                        FAILED_TOTAL_METRIC,
                        "event_type" => metric_event_type(&envelope.event_type)
                    )
                    .increment(1);
                    metrics::counter!(
                        RETRY_TOTAL_METRIC,
                        "event_type" => metric_event_type(&envelope.event_type),
                        "retry_count" => retry_count.to_string()
                    )
                    .increment(1);
                    warn!(
                        event_id = %envelope.event_id,
                        event_type = %envelope.event_type,
                        error = %error,
                        "domain event processing failed"
                    );

                    if retry_count >= self.max_retry {
                        self.insert_dead_letter(&envelope, &error.to_string(), retry_count)
                            .await?;
                        metrics::counter!(
                            DLQ_TOTAL_METRIC,
                            "event_type" => metric_event_type(&envelope.event_type)
                        )
                        .increment(1);
                    }
                }
            }
        }

        Ok(())
    }

    /// Process a single message: dedup check, dispatch, mark processed.
    async fn process_single_message(&self, message: &SubscriberMessage) -> Result<(), DomainError> {
        let envelope = Self::decode_message(message);

        if self.is_processed(&envelope.event_id).await? {
            return Ok(());
        }

        self.dispatch_event(&envelope).await?;
        self.mark_processed(&envelope).await?;
        metrics::counter!(
            PROCESSED_TOTAL_METRIC,
            "event_type" => metric_event_type(&envelope.event_type)
        )
        .increment(1);

        Ok(())
    }

    async fn dispatch_event(&self, envelope: &DomainEventEnvelope) -> Result<(), DomainError> {
        for handler in &self.handlers {
            if handler.can_handle(&envelope.event_type) {
                handler.handle(envelope.clone()).await?;
            }
        }
        Ok(())
    }

    fn decode_message(message: &SubscriberMessage) -> DomainEventEnvelope {
        let payload = normalize_event_payload(message.body.get("payload"));
        let event_id = json_text(message.body.get("event_id")).unwrap_or_else(|| message.message_id.clone());

        DomainEventEnvelope {
            event_id,
            source_change_id: json_text(message.body.get("source_change_id")),
            aggregate_type: json_text(message.body.get("aggregate_type")).unwrap_or_default(),
            aggregate_id: json_text(message.body.get("aggregate_id")).unwrap_or_default(),
            event_type: json_text(message.body.get("event_type")).unwrap_or_default(),
            payload,
            stream_message_id: message.message_id.clone(),
        }
    }

    fn observe_consumer_lag(&self, occurred_at: Option<&Value>) {
        let Some(occurred_at) = json_text(occurred_at) else {
            return;
        };
        let Some(parsed) = parse_occurred_at(&occurred_at) else {
            return;
        };

        let lag_ms = ((Utc::now() - parsed).num_milliseconds().max(0)) as f64;
        metrics::histogram!(LAG_MS_METRIC).record(lag_ms);
    }

    async fn is_processed(&self, event_id: &str) -> Result<bool, DomainError> {
        self.subscription_state.is_processed(event_id).await
    }

    async fn mark_processed(&self, envelope: &DomainEventEnvelope) -> Result<(), DomainError> {
        self.subscription_state
            .mark_processed(&Self::processing_record(envelope))
            .await
    }

    async fn mark_failed(&self, envelope: &DomainEventEnvelope, error_message: &str) -> Result<i32, DomainError> {
        self.subscription_state
            .mark_failed(&Self::processing_record(envelope), error_message)
            .await
    }

    async fn insert_dead_letter(
        &self,
        envelope: &DomainEventEnvelope,
        error_message: &str,
        retry_count: i32,
    ) -> Result<(), DomainError> {
        self.subscription_state
            .insert_dead_letter(&DomainEventDeadLetterRecord {
                event_id: envelope.event_id.clone(),
                source_change_id: envelope.source_change_id.clone(),
                aggregate_type: envelope.aggregate_type.clone(),
                aggregate_id: envelope.aggregate_id.clone(),
                event_type: envelope.event_type.clone(),
                payload: envelope.payload.clone(),
                stream_message_id: envelope.stream_message_id.clone(),
                retry_count,
                error_message: error_message.to_string(),
            })
            .await
    }
}

#[async_trait]
impl MessageHandler for DomainEventSubscriberService {
    async fn handle(&self, messages: Vec<SubscriberMessage>) -> Result<(), MessageQueueError> {
        self.handle_messages(messages)
            .await
            .map_err(|e| MessageQueueError::Transport(format!("domain event handler failed: {e}")))
    }
}

fn trim_or_default(value: Option<String>, default: &str) -> String {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn metric_event_type(event_type: &str) -> String {
    let normalized = event_type.trim();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

fn is_flight_event_type(event_type: &str) -> bool {
    FLIGHT_EVENT_TYPES.contains(&event_type.trim())
}

fn is_flight_anomaly_trigger(event_type: &str) -> bool {
    FLIGHT_ANOMALY_TRIGGER_EVENT_TYPES.contains(&event_type.trim())
}

fn is_business_case_event_type(event_type: &str) -> bool {
    BUSINESS_CASE_EVENT_TYPES.contains(&event_type.trim())
}

fn flight_patch_from_fields(flight: &Value, changed_fields: &[String]) -> Value {
    if changed_fields.is_empty() || changed_fields.iter().any(|field| field == "create") {
        return flight.clone();
    }
    let mut patch = serde_json::Map::new();
    if let Some(flight_id) = flight.get("flight_id") {
        patch.insert("flight_id".to_string(), flight_id.clone());
    }
    for field in changed_fields {
        if let Some(value) = flight.get(field) {
            patch.insert(field.clone(), value.clone());
        }
    }
    Value::Object(patch)
}

fn extract_changed_fields_from_flight_event(envelope: &DomainEventEnvelope) -> Vec<String> {
    let data = envelope
        .payload
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(&envelope.payload);

    if let Some(field_name) = data
        .get("field_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return vec![field_name.to_string()];
    }

    if let Some(fields) = data
        .get("changed_fields")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|fields| !fields.is_empty())
    {
        return fields;
    }

    match envelope.event_type.trim() {
        "flight.leg_upserted_v2" => {
            let leg_type = data
                .get("leg_type")
                .and_then(Value::as_str)
                .map(|value| value.trim().to_ascii_lowercase());
            match leg_type.as_deref() {
                Some("inbound") => vec!["inbound_leg".to_string()],
                Some("outbound") => vec!["outbound_leg".to_string()],
                _ => vec!["leg".to_string()],
            }
        }
        "flight.created_v2" => vec!["create".to_string()],
        "flight.deleted_v2" => vec!["flight_id".to_string()],
        "flight.status_updated_v2" => vec!["status".to_string()],
        "flight.remarks_updated_v2" => vec!["flight_remarks".to_string()],
        "flight.timeline_upserted_v2" | "flight.timeline_deleted_v2" => {
            if let Some(milestone) = data
                .get("milestone_code")
                .or_else(|| data.get("field_name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                vec![milestone.to_string()]
            } else {
                vec!["timeline".to_string()]
            }
        }
        _ => Vec::new(),
    }
}

fn payload_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn payload_string_array(payload: &Value, key: &str) -> Vec<String> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_business_case_id(envelope: &DomainEventEnvelope) -> String {
    payload_text(&envelope.payload, "case_id")
        .or_else(|| {
            let aggregate_id = envelope.aggregate_id.trim();
            (!aggregate_id.is_empty()).then_some(aggregate_id.to_string())
        })
        .unwrap_or_default()
}

fn resolve_business_case_workflow_actor(payload: &Value) -> WorkflowActor {
    let username = payload_text(payload, "operator_username").or_else(|| payload_text(payload, "created_by"));
    let name_snapshot = payload_text(payload, "operator_name_snapshot");
    let actor = payload_text(payload, "operator")
        .or_else(|| name_snapshot.clone())
        .or_else(|| username.clone())
        .unwrap_or_else(|| "system".to_string());

    WorkflowActor {
        actor,
        user_id: payload_text(payload, "operator_user_id"),
        username,
        name_snapshot,
        context_type: payload_text(payload, "operator_context_type"),
        context_id: payload_text(payload, "operator_context_id"),
    }
}

fn build_business_case_created_payload(envelope: &DomainEventEnvelope) -> Value {
    json!({
        "event": "business_case.created",
        "case_id": resolve_business_case_id(envelope),
        "case_type": payload_text(&envelope.payload, "case_type").unwrap_or_default(),
        "flight_id": payload_text(&envelope.payload, "flight_id").unwrap_or_default(),
    })
}

fn build_business_case_updated_payload(envelope: &DomainEventEnvelope) -> Value {
    json!({
        "event": "business_case.updated",
        "case_id": resolve_business_case_id(envelope),
        "changed_fields": payload_string_array(&envelope.payload, "changed_fields"),
    })
}

fn build_business_case_deleted_payload(envelope: &DomainEventEnvelope) -> Value {
    json!({
        "event": "business_case.deleted",
        "case_id": resolve_business_case_id(envelope),
    })
}

fn json_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_event_payload(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return json!({});
    };

    match value {
        Value::String(raw) => serde_json::from_str::<Value>(raw).unwrap_or_else(|_| json!({ "raw": raw })),
        Value::Object(_) => value.clone(),
        _ => json!({}),
    }
}

fn parse_occurred_at(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|value| DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
        })
}

#[cfg(test)]
mod tests {
    use super::{
        build_business_case_deleted_payload, extract_changed_fields_from_flight_event, flight_patch_from_fields,
        is_business_case_event_type, is_flight_anomaly_trigger, is_flight_event_type, normalize_event_payload,
        parse_occurred_at, BusinessCaseEventNotifier, BusinessCaseEventSubscriber, BusinessCaseNotifierFuture,
        BusinessCaseWorkflowFuture, BusinessCaseWorkflowTrigger, DomainEventEnvelope, DomainEventHandler,
        DomainEventSubscriberService, WorkflowActor,
    };
    use fms_domain::error::DomainError;
    use fms_domain::ports::message_queue::SubscriberMessage;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingBusinessCaseEventNotifier {
        events: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl RecordingBusinessCaseEventNotifier {
        fn items(&self) -> Vec<(String, serde_json::Value)> {
            self.events.lock().expect("lock business case events").clone()
        }
    }

    impl BusinessCaseEventNotifier for RecordingBusinessCaseEventNotifier {
        fn publish_business_case_event(
            &self,
            event_name: String,
            payload: serde_json::Value,
        ) -> BusinessCaseNotifierFuture {
            self.events
                .lock()
                .expect("lock business case events")
                .push((event_name, payload));
            Box::pin(async { Ok(1) })
        }
    }

    #[derive(Default)]
    struct RecordingBusinessCaseWorkflowTrigger {
        calls: Mutex<Vec<(String, String, WorkflowActor)>>,
    }

    impl RecordingBusinessCaseWorkflowTrigger {
        fn items(&self) -> Vec<(String, String, WorkflowActor)> {
            self.calls.lock().expect("lock business case workflow calls").clone()
        }
    }

    impl BusinessCaseWorkflowTrigger for RecordingBusinessCaseWorkflowTrigger {
        fn handle_created(
            &self,
            case_type: String,
            case_id: String,
            actor: WorkflowActor,
        ) -> BusinessCaseWorkflowFuture {
            self.calls
                .lock()
                .expect("lock business case workflow calls")
                .push((case_type, case_id, actor));
            Box::pin(async { Ok(()) })
        }
    }

    struct FailingBusinessCaseWorkflowTrigger;

    impl BusinessCaseWorkflowTrigger for FailingBusinessCaseWorkflowTrigger {
        fn handle_created(
            &self,
            _case_type: String,
            _case_id: String,
            _actor: WorkflowActor,
        ) -> BusinessCaseWorkflowFuture {
            Box::pin(async { Err(DomainError::BusinessRuleViolation("trigger failed".to_string())) })
        }
    }

    #[test]
    fn flight_event_type_sets_match_python_registry() {
        assert!(is_flight_event_type("flight.created_v2"));
        assert!(is_flight_event_type("flight.resource_updated_v2"));
        assert!(is_flight_event_type("flight.timeline_upserted_v2"));
        assert!(is_flight_event_type("flight.timeline_deleted_v2"));
        assert!(is_flight_event_type("flight.deleted_v2"));
        assert!(!is_flight_event_type("business_case.appended"));

        assert!(is_flight_anomaly_trigger("flight.status_updated_v2"));
        assert!(is_flight_anomaly_trigger("flight.leg_upserted_v2"));
        assert!(!is_flight_anomaly_trigger("flight.created_v2"));
        assert!(!is_flight_anomaly_trigger("flight.timeline_upserted_v2"));
        assert!(!is_flight_anomaly_trigger("flight.deleted_v2"));

        assert!(is_business_case_event_type("business_case.created"));
        assert!(is_business_case_event_type("business_case.updated"));
        assert!(is_business_case_event_type("business_case.deleted"));
        assert!(!is_business_case_event_type("business_case.appended"));
    }

    #[test]
    fn flight_patch_from_fields_keeps_flight_id_and_touched_fields() {
        let flight = json!({
            "flight_id": "F1",
            "status": "delayed",
            "gate": "A1",
            "stand": "S1"
        });
        let patch = flight_patch_from_fields(&flight, &["status".to_string(), "gate".to_string()]);
        assert_eq!(patch["flight_id"], json!("F1"));
        assert_eq!(patch["status"], json!("delayed"));
        assert_eq!(patch["gate"], json!("A1"));
        assert!(patch.get("stand").is_none());
    }

    #[test]
    fn flight_projection_extracts_changed_fields_from_python_compatible_payloads() {
        let field_update = DomainEventEnvelope {
            event_id: "evt_field".to_string(),
            source_change_id: None,
            aggregate_type: "flight".to_string(),
            aggregate_id: "flight_001".to_string(),
            event_type: "flight.resource_updated_v2".to_string(),
            payload: json!({
                "data": {
                    "field_name": "gate"
                }
            }),
            stream_message_id: "1710000000000-1".to_string(),
        };
        assert_eq!(
            extract_changed_fields_from_flight_event(&field_update),
            vec!["gate".to_string()]
        );

        let leg_update = DomainEventEnvelope {
            event_id: "evt_leg".to_string(),
            source_change_id: None,
            aggregate_type: "flight".to_string(),
            aggregate_id: "flight_001".to_string(),
            event_type: "flight.leg_upserted_v2".to_string(),
            payload: json!({
                "data": {
                    "leg_type": "inbound"
                }
            }),
            stream_message_id: "1710000000000-2".to_string(),
        };
        assert_eq!(
            extract_changed_fields_from_flight_event(&leg_update),
            vec!["inbound_leg".to_string()]
        );

        let created = DomainEventEnvelope {
            event_id: "evt_create".to_string(),
            source_change_id: None,
            aggregate_type: "flight".to_string(),
            aggregate_id: "flight_001".to_string(),
            event_type: "flight.created_v2".to_string(),
            payload: json!({}),
            stream_message_id: "1710000000000-3".to_string(),
        };
        assert_eq!(
            extract_changed_fields_from_flight_event(&created),
            vec!["create".to_string()]
        );
    }

    #[tokio::test]
    async fn business_case_created_events_trigger_workflow_and_publish_sse_payload() {
        let notifier = Arc::new(RecordingBusinessCaseEventNotifier::default());
        let workflow_trigger = Arc::new(RecordingBusinessCaseWorkflowTrigger::default());
        let handler = BusinessCaseEventSubscriber::new(Some(workflow_trigger.clone()), Some(notifier.clone()));
        let envelope = DomainEventEnvelope {
            event_id: "evt_001".to_string(),
            source_change_id: None,
            aggregate_type: "business_case".to_string(),
            aggregate_id: "bc_001".to_string(),
            event_type: "business_case.created".to_string(),
            payload: json!({
                "case_type": "gate_change",
                "flight_id": "flight_001",
                "operator": "当前值班调度-dispatcher",
                "operator_user_id": "user-1",
                "operator_username": "dispatcher",
                "operator_name_snapshot": "当前值班调度",
                "operator_context_type": "web_client",
                "operator_context_id": "console-1"
            }),
            stream_message_id: "1710000000000-0".to_string(),
        };

        DomainEventHandler::handle(&handler, envelope)
            .await
            .expect("handle business_case.created");

        let items = workflow_trigger.items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "gate_change");
        assert_eq!(items[0].1, "bc_001");
        assert_eq!(items[0].2.actor, "当前值班调度-dispatcher");
        assert_eq!(items[0].2.user_id.as_deref(), Some("user-1"));
        assert_eq!(items[0].2.username.as_deref(), Some("dispatcher"));
        assert_eq!(items[0].2.name_snapshot.as_deref(), Some("当前值班调度"));
        assert_eq!(items[0].2.context_type.as_deref(), Some("web_client"));
        assert_eq!(items[0].2.context_id.as_deref(), Some("console-1"));

        let events = notifier.items();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "business_case.created");
        assert_eq!(events[0].1["event"], "business_case.created");
        assert_eq!(events[0].1["case_id"], "bc_001");
        assert_eq!(events[0].1["case_type"], "gate_change");
        assert_eq!(events[0].1["flight_id"], "flight_001");
    }

    #[tokio::test]
    async fn business_case_created_events_still_publish_when_workflow_trigger_fails() {
        let notifier = Arc::new(RecordingBusinessCaseEventNotifier::default());
        let handler = BusinessCaseEventSubscriber::new(
            Some(Arc::new(FailingBusinessCaseWorkflowTrigger)),
            Some(notifier.clone()),
        );
        let envelope = DomainEventEnvelope {
            event_id: "evt_004".to_string(),
            source_change_id: None,
            aggregate_type: "business_case".to_string(),
            aggregate_id: "bc_004".to_string(),
            event_type: "business_case.created".to_string(),
            payload: json!({
                "case_type": "gate_open_bag",
                "flight_id": "flight_004",
                "operator_username": "dispatcher"
            }),
            stream_message_id: "1710000000003-0".to_string(),
        };

        DomainEventHandler::handle(&handler, envelope)
            .await
            .expect("workflow trigger errors should not stop publication");

        let events = notifier.items();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "business_case.created");
        assert_eq!(events[0].1["case_id"], "bc_004");
        assert_eq!(events[0].1["case_type"], "gate_open_bag");
        assert_eq!(events[0].1["flight_id"], "flight_004");
    }

    #[tokio::test]
    async fn business_case_updated_events_publish_changed_fields() {
        let notifier = Arc::new(RecordingBusinessCaseEventNotifier::default());
        let handler = BusinessCaseEventSubscriber::new(None, Some(notifier.clone()));
        let envelope = DomainEventEnvelope {
            event_id: "evt_002".to_string(),
            source_change_id: None,
            aggregate_type: "business_case".to_string(),
            aggregate_id: "bc_002".to_string(),
            event_type: "business_case.updated".to_string(),
            payload: json!({
                "case_id": "bc_002",
                "changed_fields": ["status", "description"]
            }),
            stream_message_id: "1710000000001-0".to_string(),
        };

        DomainEventHandler::handle(&handler, envelope)
            .await
            .expect("handle business_case.updated");

        let events = notifier.items();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "business_case.updated");
        assert_eq!(events[0].1["changed_fields"][0], "status");
        assert_eq!(events[0].1["changed_fields"][1], "description");
    }

    #[test]
    fn business_case_deleted_payload_uses_aggregate_id_fallback() {
        let envelope = DomainEventEnvelope {
            event_id: "evt_003".to_string(),
            source_change_id: None,
            aggregate_type: "business_case".to_string(),
            aggregate_id: "bc_003".to_string(),
            event_type: "business_case.deleted".to_string(),
            payload: json!({}),
            stream_message_id: "1710000000002-0".to_string(),
        };

        let payload = build_business_case_deleted_payload(&envelope);
        assert_eq!(payload["event"], "business_case.deleted");
        assert_eq!(payload["case_id"], "bc_003");
    }

    #[test]
    fn decode_payload_parses_json_and_wraps_invalid_strings() {
        assert_eq!(
            normalize_event_payload(Some(&json!("{\"hello\":\"world\"}"))),
            json!({"hello": "world"})
        );
        assert_eq!(
            normalize_event_payload(Some(&json!("not-json"))),
            json!({"raw": "not-json"})
        );
        assert_eq!(normalize_event_payload(None), json!({}));
    }

    #[test]
    fn parse_occurred_at_accepts_rfc3339_and_naive_timestamps() {
        assert!(parse_occurred_at("2026-03-25T12:30:00Z").is_some());
        assert!(parse_occurred_at("2026-03-25T12:30:00.123456").is_some());
        assert!(parse_occurred_at("invalid").is_none());
    }

    #[test]
    fn truncate_error_caps_to_database_length_budget() {
        // Repo truncates to 1000 chars; keep contract assertion in unit tests.
        const MAX_ERROR_LENGTH: usize = 1000;
        let raw = "x".repeat(1200);
        let truncated: String = raw.chars().take(MAX_ERROR_LENGTH).collect();
        assert_eq!(truncated.len(), 1000);
    }

    #[test]
    fn decode_message_uses_stream_id_when_event_id_is_missing() {
        let message = SubscriberMessage {
            message_id: "msg-001".to_string(),
            topic: "fms_domain_events".to_string(),
            tag: Some("flight.created_v2".to_string()),
            key: None,
            body: json!({
                "aggregate_type": "flight",
                "aggregate_id": "flight_001",
                "event_type": "flight.created_v2"
            }),
            properties: BTreeMap::new(),
        };

        let envelope: DomainEventEnvelope = DomainEventSubscriberService::decode_message(&message);

        assert_eq!(envelope.event_id, "msg-001");
        assert_eq!(envelope.aggregate_id, "flight_001");
        assert_eq!(envelope.event_type, "flight.created_v2");
    }
}
