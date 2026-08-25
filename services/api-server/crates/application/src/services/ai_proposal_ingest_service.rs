use crate::services::ai_action_proposal_service::GenerateProposalRequest;
use crate::services::ai_job_service::AiJobService;
use crate::services::ai_output_validator::AiOutputValidator;
use crate::types::ConcreteAiActionProposalService;
use fms_domain::models::ai_context_envelope::*;
use fms_domain::models::ai_job::AiJobStatus;
use fms_domain::models::ai_ontology::{OntologyActionDef, OntologySchema};
use fms_domain::models::ai_proposal::{ApprovalPolicy, RiskLevel};
use fms_domain::models::ai_structured_output::*;
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
use std::sync::Arc;

pub struct AiProposalIngestService {
    validator: Arc<AiOutputValidator>,
    ontology_repository: Option<Arc<dyn AiOntologyRepository + Send + Sync>>,
    proposal_service: Arc<ConcreteAiActionProposalService>,
    job_service: Arc<AiJobService>,
}

impl AiProposalIngestService {
    pub fn new(
        validator: Arc<AiOutputValidator>,
        proposal_service: Arc<ConcreteAiActionProposalService>,
        job_service: Arc<AiJobService>,
    ) -> Self {
        Self {
            validator,
            ontology_repository: None,
            proposal_service,
            job_service,
        }
    }

    pub fn with_ontology_repository(
        mut self,
        ontology_repository: Arc<dyn AiOntologyRepository + Send + Sync>,
    ) -> Self {
        self.ontology_repository = Some(ontology_repository);
        self
    }

    pub async fn ingest(&self, output: AiStructuredOutput, envelope: &ContextEnvelope) -> IngestResult {
        let active_schema = self.active_ontology_schema().await;
        let validation = match active_schema.as_ref() {
            Some(schema) => AiOutputValidator::new(schema.clone()).validate(&output, envelope),
            None => self.validator.validate(&output, envelope),
        };

        match validation {
            Ok(validated) => {
                let mut created_proposal_ids = Vec::new();
                let mut rejected_proposals = Vec::new();

                for proposal in &validated.valid_proposals {
                    match self
                        .create_proposal_from_output(proposal, envelope, active_schema.as_ref())
                        .await
                    {
                        Ok(id) => created_proposal_ids.push(id),
                        Err(e) => rejected_proposals.push(format!("{}: {}", proposal.action_name, e)),
                    }
                }

                if let Err(e) = self
                    .job_service
                    .transition_job(&envelope.job_id, AiJobStatus::Succeeded)
                    .await
                {
                    tracing::error!("failed to transition job to succeeded: {:?}", e);
                }

                IngestResult {
                    success: true,
                    created_proposal_ids,
                    rejected_proposals,
                    answer: output.answer.clone(),
                    evidence_count: output.evidence.len(),
                }
            }
            Err(errors) => {
                let error_messages: Vec<String> = errors.iter().map(|e| format!("{:?}", e)).collect();
                tracing::warn!("structured output rejected: {:?}", error_messages);

                if let Err(e) = self
                    .job_service
                    .transition_job(&envelope.job_id, AiJobStatus::FailedTerminal)
                    .await
                {
                    tracing::error!("failed to transition job to failed_terminal: {:?}", e);
                }

                IngestResult {
                    success: false,
                    created_proposal_ids: vec![],
                    rejected_proposals: error_messages,
                    answer: output.answer.clone(),
                    evidence_count: 0,
                }
            }
        }
    }

    async fn active_ontology_schema(&self) -> Option<OntologySchema> {
        if !ontology_registry_enabled() {
            return None;
        }

        let Some(repository) = &self.ontology_repository else {
            return None;
        };

        match repository.load_action_overlays().await {
            Ok(overlays) => Some(fms_domain::ontology::governed::load_governed_schema(&overlays)),
            Err(error) => {
                tracing::warn!(
                    "failed to load AI ontology overlays for proposal ingest: {}",
                    error
                );
                None
            }
        }
    }

    async fn create_proposal_from_output(
        &self,
        proposal: &OutputProposal,
        envelope: &ContextEnvelope,
        active_schema: Option<&OntologySchema>,
    ) -> Result<String, String> {
        let action_def = active_schema
            .and_then(|schema| schema.objects.get(&proposal.object_type))
            .and_then(|object| object.actions.get(&proposal.action_name));
        let governance = action_def.and_then(proposal_governance_from_action);

        let result = self
            .proposal_service
            .generate_proposal(GenerateProposalRequest {
                job_id: envelope.job_id.clone(),
                run_id: envelope.run_id.clone(),
                ontology_version: Some(envelope.ontology.version.clone()),
                object_type: proposal.object_type.clone(),
                object_id: proposal.object_id.clone(),
                action_name: proposal.action_name.clone(),
                arguments: proposal.arguments.clone(),
                reasoning: Some(proposal.reasoning.clone()),
                confidence: Some(proposal.confidence),
                requester_user_id: Some(envelope.requester.user_id.clone()),
                requester_user_roles: envelope.requester.roles.clone(),
                requester_department_id: envelope.requester.department_id.clone(),
                correlation_id: Some(envelope.correlation_id.clone()),
                idempotency_key: None,
                expected_object_version: envelope
                    .context
                    .objects
                    .iter()
                    .find(|object| object.object_type == proposal.object_type && object.object_id == proposal.object_id)
                    .and_then(|object| object.version),
                risk_level: governance.as_ref().map(|item| item.0),
                approval_policy: governance.as_ref().map(|item| item.1),
                required_permissions: action_def.map(|action| action.required_permissions.clone()),
            })
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.proposal_id)
    }
}

fn proposal_governance_from_action(action: &OntologyActionDef) -> Option<(RiskLevel, ApprovalPolicy)> {
    let risk_level = RiskLevel::from_str_loose(&action.risk_level)?;
    let approval_policy = ApprovalPolicy::from_str_loose(&action.approval_policy)
        .or_else(|| ApprovalPolicy::from_str_loose(&action.approval_strategy))
        .unwrap_or(ApprovalPolicy::RequireApproval);
    Some((risk_level, approval_policy))
}

pub struct IngestResult {
    pub success: bool,
    pub created_proposal_ids: Vec<String>,
    pub rejected_proposals: Vec<String>,
    pub answer: String,
    pub evidence_count: usize,
}

fn ontology_registry_enabled() -> bool {
    std::env::var("FMS_AI_ONTOLOGY_REGISTRY_ENABLED")
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}
