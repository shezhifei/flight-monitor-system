//! End-to-end integration test for Wave 1 + Wave 2.
//!
//! Drives the full `ai.runtime.events` flow through a real
//! `AiEventConsumer` wired into `fms_infrastructure::messaging::MemoryPushConsumer`.
//! Uses the in-memory repositories so no Postgres / RocketMQ is
//! required.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use fms_application::services::ai_runtime_service::ai_event_consumer::AiEventConsumer;
use fms_application::services::ai_runtime_service::ai_execution_control_service::{
    AiExecutionControlService, ControlServiceError, ProposalIngestHook,
};
use fms_application::services::ai_runtime_service::in_memory_repos::{
    InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
};
use fms_application::services::ai_runtime_service::tool_authorization_service::{
    StaticFeatureFlagSource, ToolAuthorizationService,
};
use fms_domain::ai_runtime_event::{
    AiRuntimeEventEnvelope, AiRuntimeEventType, ToolAuthorizationMode, ToolExecutionStatus,
};
use fms_domain::models::ai_execution::{AiRuntimeCommandType, AiToolCallStatus};
use fms_domain::models::tool_authorization::ToolAuthorizationContext;
use fms_domain::models::tool_governance::ToolGovernancePreset;
use fms_domain::ports::ai_auth_context_loader::{AuthContextLoaderError, RunAuthorizationContextLoader};
use fms_domain::ports::ai_execution_repository::AiToolCallRepository as _;
use fms_infrastructure::messaging::{MemoryPushConsumer, PushConsumer, SubscriberMessage};
use serde_json::json;

struct CountingIngest {
    counter: std::sync::atomic::AtomicUsize,
}

struct StaticAuthContextLoader {
    context: ToolAuthorizationContext,
}

#[async_trait::async_trait]
impl RunAuthorizationContextLoader for StaticAuthContextLoader {
    async fn load_context(
        &self,
        _run_id: &str,
        _job_id: &str,
        _tool_call_pk: &str,
        _tool_name: &str,
        _tool_args: &serde_json::Value,
    ) -> Result<ToolAuthorizationContext, AuthContextLoaderError> {
        Ok(self.context.clone())
    }
}

#[async_trait::async_trait]
impl ProposalIngestHook for CountingIngest {
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

fn envelope(
    event_type: AiRuntimeEventType,
    run_id: &str,
    job_id: &str,
    sequence: u64,
    idempotency_key: &str,
    payload: serde_json::Value,
) -> AiRuntimeEventEnvelope {
    AiRuntimeEventEnvelope::new(event_type, run_id, job_id, 0, sequence, idempotency_key, payload)
}

fn subscriber(envelope: AiRuntimeEventEnvelope) -> SubscriberMessage {
    SubscriberMessage {
        message_id: ulid::Ulid::new().to_string(),
        topic: "ai.runtime.events".into(),
        tag: Some(envelope.event_type.as_str().to_string()),
        key: Some(envelope.run_id.clone()),
        body: serde_json::to_value(&envelope).unwrap(),
        properties: BTreeMap::new(),
    }
}

async fn build_consumer() -> (
    MemoryPushConsumer,
    Arc<InMemoryToolCallRepository>,
    Arc<InMemoryRuntimeCommandRepository>,
) {
    let tool_call_repo = Arc::new(InMemoryToolCallRepository::new());
    let command_repo = Arc::new(InMemoryRuntimeCommandRepository::new());
    let authorization = Arc::new(ToolAuthorizationService::new(
        Arc::new(StaticFeatureFlagSource::empty()),
    ));
    let ingest = Arc::new(CountingIngest {
        counter: std::sync::atomic::AtomicUsize::new(0),
    });
    let mut governance = ToolGovernancePreset::InternalWorkspaceWrite.default_governance("book_flight");
    governance.required_account_permissions = vec!["booking:write".into()];
    governance.execution_mode = fms_domain::models::tool_governance::ExecutionMode::Direct;
    let auth_context_loader = Arc::new(StaticAuthContextLoader {
        context: ToolAuthorizationContext {
            requester_user_id: "user-1".into(),
            requester_user_roles: vec!["dispatcher".into()],
            requester_permissions: vec!["booking:write".into()],
            requester_object_policies: Vec::new(),
            entity_tool_allowlist: vec!["book_flight".into()],
            tool_governance: governance,
            tool_call_pk: "tpc-1".into(),
            tool_args: json!({"flight": "CA1234"}),
            feature_flags: HashMap::new(),
        },
    });
    let control = Arc::new(
        AiExecutionControlService::new(tool_call_repo.clone(), command_repo.clone(), authorization)
            .with_auth_context_loader(auth_context_loader)
            .with_proposal_ingest(ingest),
    );
    let consumer = AiEventConsumer::new(control);
    let push = MemoryPushConsumer::new();
    push.subscribe("ai.runtime.events", "fms-ai-runtime", Some("*"), Arc::new(consumer))
        .await
        .unwrap();
    push.start().await.unwrap();
    (push, tool_call_repo, command_repo)
}

async fn wait_for<F, Fut>(predicate: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..50 {
        if predicate().await {
            return;
        }
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("predicate did not become true within 500ms");
}

#[tokio::test]
async fn protected_tool_request_creates_lease_and_result_marks_succeeded() {
    let (push, tool_call_repo, command_repo) = build_consumer().await;

    let mut governance = ToolGovernancePreset::InternalWorkspaceWrite.default_governance("book_flight");
    governance.required_account_permissions = vec!["booking:write".into()];
    governance.execution_mode = fms_domain::models::tool_governance::ExecutionMode::Direct;
    let governance_value = serde_json::to_value(&governance).unwrap();

    let requested = envelope(
        AiRuntimeEventType::ToolCallRequested,
        "run-1",
        "job-1",
        1,
        "run-1:0:call-tpc-1:book_flight:abc",
        json!({
            "tool_call_pk": "tpc-1",
            "tool_call_id": "call-tpc-1",
            "tool_name": "book_flight",
            "tool_type": "builtin",
            "parent_tool_call_pk": null,
            "depth": 0,
            "args_hash": "abc",
            "args_summary": {"flight": "CA1234"},
            "authorization_mode": ToolAuthorizationMode::RustPdp,
            "max_retries": 2,
            "timeout_seconds": 30,
            "requester": {
                "user_id": "user-1",
                "roles": ["dispatcher"],
                "permissions": ["booking:write"],
                "object_policies": [],
            },
            "governance": governance_value,
            "entity_allowlist": ["book_flight"],
        }),
    );
    push.inject(
        "ai.runtime.events",
        Some("tool.call.requested"),
        vec![subscriber(requested)],
    );

    wait_for(|| async {
        command_repo.len() == 1
            && tool_call_repo.get("tpc-1").await.unwrap().unwrap().status == AiToolCallStatus::Authorized
    })
    .await;
    let commands = command_repo.snapshot();
    assert_eq!(commands[0].command_type, AiRuntimeCommandType::ToolLease);
    assert!(commands[0].payload.get("lease_id").is_some());

    let result = envelope(
        AiRuntimeEventType::ToolResult,
        "run-1",
        "job-1",
        2,
        "run-1:0:result-tpc-1",
        json!({
            "tool_call_pk": "tpc-1",
            "tool_call_id": "call-tpc-1",
            "tool_name": "book_flight",
            "status": ToolExecutionStatus::Succeeded,
            "result_hash": "rh",
            "result_summary": {"ok": true},
            "error_code": null,
            "error_message": null,
            "retry_count": 0,
            "proposal_ids": [],
            "duration_ms": 12,
        }),
    );
    push.inject("ai.runtime.events", Some("tool.result"), vec![subscriber(result)]);

    wait_for(|| async { tool_call_repo.get("tpc-1").await.unwrap().unwrap().status == AiToolCallStatus::Succeeded })
        .await;
    let row = tool_call_repo.get("tpc-1").await.unwrap().unwrap();
    assert_eq!(row.status, AiToolCallStatus::Succeeded);
}

#[tokio::test]
async fn duplicate_tool_request_results_in_single_ledger_row() {
    let (push, tool_call_repo, _) = build_consumer().await;

    let body = json!({
        "tool_call_pk": "tpc-dup",
        "tool_call_id": "call-tpc-dup",
        "tool_name": "weather_at_airport",
        "tool_type": "builtin",
        "parent_tool_call_pk": null,
        "depth": 0,
        "args_hash": "abc",
        "args_summary": {"airport_code": "PEK"},
        "authorization_mode": ToolAuthorizationMode::PublicDirect,
        "max_retries": 2,
        "timeout_seconds": 30,
    });
    let first = envelope(
        AiRuntimeEventType::ToolCallRequested,
        "run-1",
        "job-1",
        1,
        "run-1:0:call-tpc-dup:weather_at_airport:abc",
        body.clone(),
    );
    let second = envelope(
        AiRuntimeEventType::ToolCallRequested,
        "run-1",
        "job-1",
        2,
        "run-1:0:call-tpc-dup:weather_at_airport:abc",
        body,
    );
    push.inject(
        "ai.runtime.events",
        Some("tool.call.requested"),
        vec![subscriber(first)],
    );
    push.inject(
        "ai.runtime.events",
        Some("tool.call.requested"),
        vec![subscriber(second)],
    );

    wait_for(|| async { tool_call_repo.len() == 1 }).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(tool_call_repo.len(), 1);
}
