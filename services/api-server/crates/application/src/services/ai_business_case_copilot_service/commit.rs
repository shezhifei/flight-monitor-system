//! Commit-case creation helpers for `AiBusinessCaseCopilotService`.
//!
//! Split from `service.rs` to keep file sizes manageable. These methods handle
//! creating or reusing business cases when a copilot batch is committed.
//! They only access `pub(crate)` struct fields, so they can live outside the
//! file that defines the struct.

use std::collections::HashMap;

use serde_json::Value;

use fms_domain::error::DomainError;
use fms_domain::models::business_case::{FlightBusinessCase, VisibilityScope};
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;

use crate::services::business_case_workflow_service::WorkflowActor;

use super::config::PreparedCommitAction;
use super::service::AiBusinessCaseCopilotService;

impl<R> AiBusinessCaseCopilotService<R>
where
    R: AiCopilotBusinessCaseBatchRepository + Send + Sync + ?Sized,
{
    /// Create new business cases for each prepared action, or reuse an existing
    /// case when one is already linked to the same `(batch_id, action_id)` pair.
    ///
    /// Widened from private to `pub(super)` so the commit orchestrators that
    /// remain in `service.rs` (`commit_batch`, `recover_one_stale_commit`)
    /// can call it across module boundaries.
    pub(super) async fn create_or_reuse_commit_cases(
        &self,
        batch_id: &str,
        prepared_actions: &[PreparedCommitAction],
        actor: &WorkflowActor,
        visibility_scope: VisibilityScope,
        viewer_department_id: Option<&str>,
        viewer_department_name: Option<&str>,
    ) -> Result<Vec<FlightBusinessCase>, DomainError> {
        let existing_cases = self.business_case_service.list_by_copilot_batch(batch_id).await?;
        let mut existing_by_action = HashMap::new();
        for case in existing_cases {
            if let Some(action_id) = case
                .context
                .get("copilot_action_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                existing_by_action.entry(action_id.to_string()).or_insert(case);
            }
        }

        let mut cases = Vec::with_capacity(prepared_actions.len());
        for prepared in prepared_actions {
            let action_id = prepared.action.action_id.trim();
            if let Some(case) = existing_by_action.get(action_id).cloned() {
                self.record_created_action_case_strict(batch_id, action_id, &case.case_id)
                    .await?;
                cases.push(case);
                continue;
            }

            if let Some(case) = self
                .business_case_service
                .find_by_copilot_batch_action(batch_id, action_id)
                .await?
            {
                self.record_created_action_case_strict(batch_id, action_id, &case.case_id)
                    .await?;
                cases.push(case);
                continue;
            }

            let case = self
                .business_case_service
                .create_for_viewer(
                    &prepared.action.case_type,
                    &prepared.flight_id,
                    &prepared.flight_no,
                    &prepared.description,
                    prepared.context.clone(),
                    prepared.status.as_deref(),
                    &actor.actor,
                    visibility_scope,
                    viewer_department_id,
                    viewer_department_name,
                )
                .await?;
            self.record_created_action_case_strict(batch_id, action_id, &case.case_id)
                .await?;
            cases.push(case);
        }

        Ok(cases)
    }

    /// Record that a business case was created (or reused) for a given action.
    /// Stays private because it is only called from `create_or_reuse_commit_cases`
    /// within this file.
    async fn record_created_action_case_strict(
        &self,
        batch_id: &str,
        action_id: &str,
        case_id: &str,
    ) -> Result<(), DomainError> {
        self.repo
            .record_created_action_case(batch_id, action_id, case_id)
            .await?
            .ok_or_else(|| {
                DomainError::Conflict(format!(
                    "copilot batch {batch_id} could not record action {action_id} case {case_id}"
                ))
            })?;
        Ok(())
    }
}
