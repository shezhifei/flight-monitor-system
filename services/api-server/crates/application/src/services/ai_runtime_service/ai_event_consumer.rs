//! `AiEventConsumer` — the RocketMQ push consumer for
//! `ai.runtime.events` (Phase 1 of the AI agent resilient tool
//! architecture).
//!
//! The consumer is a [`MessageHandler`] implementation; it receives
//! batches of [`SubscriberMessage`] from the
//! `fms_infrastructure::messaging` push consumer wiring, parses each
//! payload as an [`AiRuntimeEventEnvelope`], and dispatches it to the
//! [`AiExecutionControlService`] for durable ledger work.
//!
//! # Ordering
//!
//! Phase 1 runs a **single** consumer instance per deployment. Same
//! `run_id` events are routed to a stable RocketMQ queue by the
//! sidecar (using `run_id` as the MQ Message Key) and processed
//! serially in the order the consumer receives them. The DB
//! `UNIQUE(run_id, idempotency_key)` constraint provides at-least-
//! once → effectively-once deduplication on retries, and the per-run
//! `command_sequence` keeps `ai_runtime_commands` ordered for the
//! Python worker.
//!
//! Multi-worker fan-out (which makes per-run ordering a hard
//! requirement) lands in Phase 4; the `AiExecutionControlService`
//! sequence counter and the `Message Key` based queue selector are
//! already in place to support that move.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use fms_domain::ai_runtime_event::{AiRuntimeEventEnvelope, AiRuntimeEventType};
use fms_infrastructure::messaging::{MessageHandler, MessageQueueError, SubscriberMessage};

use crate::services::ai_runtime_service::ai_execution_control_service::{
    AiExecutionControlService, ControlServiceError,
};

pub struct AiEventConsumer {
    control_service: Arc<AiExecutionControlService>,
}

impl std::fmt::Debug for AiEventConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiEventConsumer")
            .field("control_service", &"Arc<AiExecutionControlService>")
            .finish()
    }
}

impl AiEventConsumer {
    pub fn new(control_service: Arc<AiExecutionControlService>) -> Self {
        Self { control_service }
    }
}

#[async_trait]
impl MessageHandler for AiEventConsumer {
    async fn handle(&self, messages: Vec<SubscriberMessage>) -> Result<(), MessageQueueError> {
        for message in messages {
            if let Err(error) = dispatch(&self.control_service, &message).await {
                match error {
                    ControlServiceError::PayloadParse(_) => {
                        tracing::warn!(
                            target: "ai_event_consumer",
                            message_id = %message.message_id,
                            tag = ?message.tag,
                            error = %error,
                            "poison ai.runtime.events message; acking without retry"
                        );
                    }
                    other => {
                        return Err(MessageQueueError::Gateway(other.to_string()));
                    }
                }
            }
        }
        Ok(())
    }
}

async fn dispatch(
    control_service: &AiExecutionControlService,
    message: &SubscriberMessage,
) -> Result<(), ControlServiceError> {
    let envelope = match parse_envelope(&message.body) {
        Ok(env) => env,
        Err(error) => {
            tracing::warn!(
                target: "ai_event_consumer",
                message_id = %message.message_id,
                error = %error,
                "failed to parse ai.runtime.events envelope; acking without retry"
            );
            return Err(ControlServiceError::PayloadParse(error));
        }
    };

    match envelope.event_type {
        AiRuntimeEventType::ToolCallRequested => control_service.handle_tool_call_requested(envelope).await,
        AiRuntimeEventType::ToolResult => control_service.handle_tool_result(envelope).await,
        AiRuntimeEventType::Checkpoint => control_service.handle_checkpoint(envelope).await,
        AiRuntimeEventType::Heartbeat => control_service.update_heartbeat(envelope).await,
        AiRuntimeEventType::RunComplete => control_service.handle_run_complete(envelope).await,
        AiRuntimeEventType::RunFail => control_service.handle_run_fail(envelope).await,
    }
}

fn parse_envelope(body: &Value) -> Result<AiRuntimeEventEnvelope, String> {
    if !body.is_object() {
        return Err(format!("expected object body, got {}", body));
    }
    serde_json::from_value::<AiRuntimeEventEnvelope>(body.clone())
        .map_err(|error| format!("failed to decode AiRuntimeEventEnvelope: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai_runtime_service::ai_execution_control_service::{
        AiExecutionControlService, LoggingProposalIngestHook, ProposalIngestHook,
    };
    use crate::services::ai_runtime_service::in_memory_repos::{
        InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
    };
    use crate::services::ai_runtime_service::tool_authorization_service::{
        StaticFeatureFlagSource, ToolAuthorizationService,
    };
    use async_trait::async_trait;
    use fms_domain::ai_runtime_event::{
        AiRuntimeEventEnvelope, AiRuntimeEventType, ToolAuthorizationMode, ToolExecutionStatus,
    };
    use fms_domain::models::ai_execution::{AiRuntimeCommandType, AiToolCallStatus};
    use fms_domain::models::tool_authorization::ToolAuthorizationContext;
    use fms_domain::models::tool_governance::ToolGovernancePreset;
    use fms_domain::ports::ai_auth_context_loader::{AuthContextLoaderError, RunAuthorizationContextLoader};
    use fms_domain::ports::ai_execution_repository::AiToolCallRepository as _;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    /// Mock loader that returns a pre-configured context.
    struct MockAuthContextLoader {
        context: ToolAuthorizationContext,
    }

    #[async_trait]
    impl RunAuthorizationContextLoader for MockAuthContextLoader {
        async fn load_context(
            &self,
            _run_id: &str,
            _job_id: &str,
            _tool_call_pk: &str,
            _tool_name: &str,
            _tool_args: &Value,
        ) -> Result<ToolAuthorizationContext, AuthContextLoaderError> {
            Ok(self.context.clone())
        }
    }

    fn harness() -> (
        AiEventConsumer,
        Arc<InMemoryToolCallRepository>,
        Arc<InMemoryRuntimeCommandRepository>,
    ) {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let authorization = Arc::new(ToolAuthorizationService::new(
            Arc::new(StaticFeatureFlagSource::empty()),
        ));
        let svc = Arc::new(AiExecutionControlService::new(
            tool_call_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiToolCallRepository>,
            command_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiRuntimeCommandRepository>,
            authorization,
        ));
        let consumer = AiEventConsumer::new(svc);
        (consumer, tool_call_repo, command_repo)
    }

    fn harness_with_auth(
        auth_context: ToolAuthorizationContext,
    ) -> (
        AiEventConsumer,
        Arc<InMemoryToolCallRepository>,
        Arc<InMemoryRuntimeCommandRepository>,
    ) {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let authorization = Arc::new(ToolAuthorizationService::new(
            Arc::new(StaticFeatureFlagSource::empty()),
        ));
        let svc = Arc::new(
            AiExecutionControlService::new(
                tool_call_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiToolCallRepository>,
                command_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiRuntimeCommandRepository>,
                authorization,
            )
            .with_auth_context_loader(Arc::new(MockAuthContextLoader { context: auth_context })),
        );
        let consumer = AiEventConsumer::new(svc);
        (consumer, tool_call_repo, command_repo)
    }

    fn subscriber_message(topic: &str, tag: Option<&str>, body: Value) -> SubscriberMessage {
        SubscriberMessage {
            message_id: ulid::Ulid::new().to_string(),
            topic: topic.to_string(),
            tag: tag.map(str::to_string),
            key: None,
            body,
            properties: BTreeMap::new(),
        }
    }

    fn requested_envelope(tool_call_pk: &str, mode: ToolAuthorizationMode) -> Value {
        serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::ToolCallRequested,
            "run-1",
            "job-1",
            0,
            1,
            format!("run-1:0:call-{tool_call_pk}:weather_at_airport:abc"),
            json!({
                "tool_call_pk": tool_call_pk,
                "tool_call_id": format!("call-{tool_call_pk}"),
                "tool_name": "weather_at_airport",
                "tool_type": "builtin",
                "parent_tool_call_pk": null,
                "depth": 0,
                "args_hash": "abc",
                "args_summary": {"airport_code": "PEK"},
                "authorization_mode": mode,
                "max_retries": 2,
                "timeout_seconds": 30,
            }),
        ))
        .unwrap()
    }

    fn protected_envelope(tool_call_pk: &str, requester_permissions: Vec<String>) -> Value {
        let mut governance = ToolGovernancePreset::InternalWorkspaceWrite.default_governance("book_flight");
        governance.required_account_permissions = vec!["booking:write".into()];
        let governance_value = serde_json::to_value(&governance).unwrap();
        serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::ToolCallRequested,
            "run-1",
            "job-1",
            0,
            1,
            format!("run-1:0:call-{tool_call_pk}:book_flight:abc"),
            json!({
                "tool_call_pk": tool_call_pk,
                "tool_call_id": format!("call-{tool_call_pk}"),
                "tool_name": "book_flight",
                "tool_type": "builtin",
                "parent_tool_call_pk": null,
                "depth": 0,
                "args_hash": "abc",
                "args_summary": {"flight_id": "CA123"},
                "authorization_mode": ToolAuthorizationMode::RustPdp,
                "max_retries": 2,
                "timeout_seconds": 30,
                "requester": {
                    "user_id": "user-1",
                    "roles": ["dispatcher"],
                    "permissions": requester_permissions,
                    "object_policies": [],
                },
                "governance": governance_value,
                "entity_allowlist": ["book_flight"],
            }),
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn parses_valid_envelope_and_dispatches() {
        let (consumer, tool_call_repo, _) = harness();
        let body = requested_envelope("tpc-1", ToolAuthorizationMode::PublicDirect);
        let message = subscriber_message("ai.runtime.events", Some("tool.call.requested"), body);
        consumer.handle(vec![message]).await.unwrap();
        assert_eq!(tool_call_repo.len(), 1);
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Running);
    }

    #[tokio::test]
    async fn poison_message_with_bad_json_is_acked_not_retried() {
        let (consumer, tool_call_repo, _) = harness();
        let message = subscriber_message(
            "ai.runtime.events",
            Some("tool.call.requested"),
            json!({ "this": "is not a valid envelope" }),
        );
        let result = consumer.handle(vec![message]).await;
        assert!(result.is_ok(), "poison message must be acked, not retry");
        assert!(tool_call_repo.is_empty());
    }

    #[tokio::test]
    async fn poison_message_with_non_object_body_is_acked_not_retried() {
        let (consumer, tool_call_repo, _) = harness();
        let message = subscriber_message("ai.runtime.events", Some("tool.call.requested"), json!("just a string"));
        let result = consumer.handle(vec![message]).await;
        assert!(result.is_ok());
        assert!(tool_call_repo.is_empty());
    }

    #[tokio::test]
    async fn duplicate_event_idempotency_key_is_skipped() {
        let (consumer, tool_call_repo, _) = harness();
        let body = requested_envelope("tpc-1", ToolAuthorizationMode::PublicDirect);
        let m1 = subscriber_message("ai.runtime.events", Some("tool.call.requested"), body.clone());
        let m2 = subscriber_message("ai.runtime.events", Some("tool.call.requested"), body);
        consumer.handle(vec![m1]).await.unwrap();
        consumer.handle(vec![m2]).await.unwrap();
        assert_eq!(tool_call_repo.len(), 1);
    }

    #[tokio::test]
    async fn dispatches_protected_request_to_authorization_path() {
        // Build a governance with execution_mode=ProposalOnly, matching
        // the InternalWorkspaceWrite preset used by protected_envelope.
        let mut governance = ToolGovernancePreset::InternalWorkspaceWrite.default_governance("book_flight");
        governance.required_account_permissions = vec!["booking:write".into()];
        let auth_context = ToolAuthorizationContext {
            requester_user_id: "user-1".to_string(),
            requester_user_roles: vec!["dispatcher".to_string()],
            requester_permissions: vec!["weather:read".to_string()],
            requester_object_policies: Vec::new(),
            entity_tool_allowlist: vec!["book_flight".to_string()],
            tool_governance: governance,
            tool_call_pk: "tpc-1".to_string(),
            tool_args: json!({"flight_id": "CA123"}),
            feature_flags: std::collections::HashMap::new(),
        };
        let (consumer, _, command_repo) = harness_with_auth(auth_context);
        let body = protected_envelope("tpc-1", vec!["weather:read".into()]);
        let message = subscriber_message("ai.runtime.events", Some("tool.call.requested"), body);
        consumer.handle(vec![message]).await.unwrap();
        let commands = command_repo.snapshot();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command_type, AiRuntimeCommandType::ToolProposalOnly);
    }

    #[tokio::test]
    async fn dispatches_tool_result_to_mark_succeeded() {
        let (consumer, tool_call_repo, _) = harness();
        let req = requested_envelope("tpc-1", ToolAuthorizationMode::PublicDirect);
        let req_msg = subscriber_message("ai.runtime.events", Some("tool.call.requested"), req);
        consumer.handle(vec![req_msg]).await.unwrap();

        let result_body = serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::ToolResult,
            "run-1",
            "job-1",
            0,
            2,
            "run-1:0:result-tpc-1",
            json!({
                "tool_call_pk": "tpc-1",
                "tool_call_id": "call-tpc-1",
                "tool_name": "weather_at_airport",
                "status": ToolExecutionStatus::Succeeded,
                "result_hash": "rh",
                "result_summary": {"ok": true},
                "error_code": null,
                "error_message": null,
                "retry_count": 0,
                "proposal_ids": [],
                "duration_ms": 12,
            }),
        ))
        .unwrap();
        let result_msg = subscriber_message("ai.runtime.events", Some("tool.result"), result_body);
        consumer.handle(vec![result_msg]).await.unwrap();
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert_eq!(row.status, AiToolCallStatus::Succeeded);
    }

    #[tokio::test]
    async fn dispatches_heartbeat_to_control_service() {
        let (consumer, tool_call_repo, _) = harness();
        let req = requested_envelope("tpc-1", ToolAuthorizationMode::PublicDirect);
        let req_msg = subscriber_message("ai.runtime.events", Some("tool.call.requested"), req);
        consumer.handle(vec![req_msg]).await.unwrap();

        let hb_body = serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::Heartbeat,
            "run-1",
            "job-1",
            0,
            3,
            "run-1:0:hb-tpc-1",
            json!({ "tool_call_pk": "tpc-1", "progress_pct": 50, "note": null }),
        ))
        .unwrap();
        let hb_msg = subscriber_message("ai.runtime.events", Some("heartbeat"), hb_body);
        consumer.handle(vec![hb_msg]).await.unwrap();
        let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
        assert!(row.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn dispatches_checkpoint_and_run_lifecycle_events_without_error() {
        let (consumer, _, _) = harness();
        let cp_body = serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::Checkpoint,
            "run-1",
            "job-1",
            0,
            4,
            "run-1:0:cp-1",
            json!({
                "checkpoint_id": "cp-1",
                "sequence_no": 1,
                "checkpoint_type": "before_tool",
                "tool_call_pk": "tpc-1",
                "proposal_id": null,
                "snapshot_hash": "h",
                "snapshot": {},
                "snapshot_size_bytes": 2,
            }),
        ))
        .unwrap();
        consumer
            .handle(vec![subscriber_message(
                "ai.runtime.events",
                Some("checkpoint"),
                cp_body,
            )])
            .await
            .unwrap();

        let complete_body = serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::RunComplete,
            "run-1",
            "job-1",
            0,
            5,
            "run-1:0:complete",
            json!({
                "output_raw": {"answer": "ok"},
                "token_usage": null,
                "proposal_ids": [],
                "terminal_event_id": null,
            }),
        ))
        .unwrap();
        consumer
            .handle(vec![subscriber_message(
                "ai.runtime.events",
                Some("run.complete"),
                complete_body,
            )])
            .await
            .unwrap();
    }

    struct CountingProposalIngest {
        counter: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl ProposalIngestHook for CountingProposalIngest {
        async fn ingest(
            &self,
            _run_id: &str,
            _job_id: &str,
            _tool_call_pk: &str,
            proposal_ids: &[String],
        ) -> Result<(), ControlServiceError> {
            self.counter
                .fetch_add(proposal_ids.len(), std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn tool_result_with_proposal_ids_invokes_ingest_hook() {
        let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
        let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
        let authorization = Arc::new(ToolAuthorizationService::new(
            Arc::new(StaticFeatureFlagSource::empty()),
        ));
        let ingest = Arc::new(CountingProposalIngest {
            counter: std::sync::atomic::AtomicUsize::new(0),
        });
        let svc = Arc::new(
            AiExecutionControlService::new(
                tool_call_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiToolCallRepository>,
                command_repo.clone() as Arc<dyn fms_domain::ports::ai_execution_repository::AiRuntimeCommandRepository>,
                authorization,
            )
            .with_proposal_ingest(ingest.clone()),
        );
        let consumer = AiEventConsumer::new(svc);

        let req = requested_envelope("tpc-1", ToolAuthorizationMode::PublicDirect);
        consumer
            .handle(vec![subscriber_message(
                "ai.runtime.events",
                Some("tool.call.requested"),
                req,
            )])
            .await
            .unwrap();

        let result_body = serde_json::to_value(AiRuntimeEventEnvelope::new(
            AiRuntimeEventType::ToolResult,
            "run-1",
            "job-1",
            0,
            2,
            "run-1:0:result-tpc-1",
            json!({
                "tool_call_pk": "tpc-1",
                "tool_call_id": "call-tpc-1",
                "tool_name": "weather_at_airport",
                "status": ToolExecutionStatus::Succeeded,
                "result_hash": "rh",
                "result_summary": {"ok": true},
                "error_code": null,
                "error_message": null,
                "retry_count": 0,
                "proposal_ids": ["p-1", "p-2"],
                "duration_ms": 12,
            }),
        ))
        .unwrap();
        consumer
            .handle(vec![subscriber_message(
                "ai.runtime.events",
                Some("tool.result"),
                result_body,
            )])
            .await
            .unwrap();
        assert_eq!(ingest.counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn default_ingest_hook_is_logging() {
        let hook = LoggingProposalIngestHook;
        let _ = hook;
    }
}
