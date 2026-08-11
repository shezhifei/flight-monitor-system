use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use fms_domain::ports::ai_run_event_repository::AiRunEventRepository;

#[derive(Debug, Clone)]
pub struct ProposalAuditEvent {
    pub proposal_id: String,
    pub job_id: String,
    pub run_id: String,
    pub event_type: String,
    pub payload: Option<Value>,
}

#[async_trait]
pub trait AiProposalAuditEventRecorder: Send + Sync {
    async fn record_execution_event(&self, event: &ProposalAuditEvent) -> Result<(), String>;
}

pub struct NoopAiProposalAuditEventRecorder;

#[async_trait]
impl AiProposalAuditEventRecorder for NoopAiProposalAuditEventRecorder {
    async fn record_execution_event(&self, _event: &ProposalAuditEvent) -> Result<(), String> {
        Ok(())
    }
}

pub struct PgAiProposalAuditEventRecorder {
    event_repo: Arc<dyn AiRunEventRepository + Send + Sync>,
}

impl PgAiProposalAuditEventRecorder {
    pub fn new(event_repo: Arc<dyn AiRunEventRepository + Send + Sync>) -> Self {
        Self { event_repo }
    }
}

#[async_trait]
impl AiProposalAuditEventRecorder for PgAiProposalAuditEventRecorder {
    async fn record_execution_event(&self, event: &ProposalAuditEvent) -> Result<(), String> {
        let payload = event
            .payload
            .clone()
            .unwrap_or_else(|| serde_json::json!({ "proposal_id": event.proposal_id }));
        self.event_repo
            .insert_fire_and_forget(&event.job_id, &event.run_id, &event.event_type, Some(payload))
            .await
            .map_err(|e| format!("failed to record proposal audit event: {e}"))?;

        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct InMemoryAiProposalAuditEventRecorder {
    events: Arc<tokio::sync::RwLock<Vec<ProposalAuditEvent>>>,
}

impl InMemoryAiProposalAuditEventRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn contains(&self, event_type: &str) -> bool {
        let events = self.events.read().await;
        events.iter().any(|e| e.event_type == event_type)
    }

    pub async fn events(&self) -> Vec<ProposalAuditEvent> {
        self.events.read().await.clone()
    }
}

#[async_trait]
impl AiProposalAuditEventRecorder for InMemoryAiProposalAuditEventRecorder {
    async fn record_execution_event(&self, event: &ProposalAuditEvent) -> Result<(), String> {
        self.events.write().await.push(event.clone());
        Ok(())
    }
}
