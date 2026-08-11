//! Batch query, status, operational-metrics and failed-batch resolution
//! methods for `AiBusinessCaseCopilotService`.
//!
//! Split from `service.rs` to keep file sizes manageable. These methods only
//! access `pub(crate)` struct fields (`repo`, `business_case_service`), so
//! they can live outside the file that defines the struct.

use chrono::Utc;

use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::ai_copilot::{AiCopilotBatchStatus, AiCopilotBusinessCaseBatch};
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;

use super::access::AiCopilotBatchAccess;
use super::helpers::*;
use super::schemas::{
    AiCopilotBatchListResponse, AiCopilotBatchStatusResponse, AiCopilotFailedBatchResolutionAction,
    AiCopilotFailedBatchResolutionRequest, AiCopilotOperationalMetricsResponse,
};
use super::service::AiBusinessCaseCopilotService;

const BATCH_LIST_ACCESS_SCAN_PAGE_SIZE: i64 = 200;

impl<R> AiBusinessCaseCopilotService<R>
where
    R: AiCopilotBusinessCaseBatchRepository + Send + Sync + ?Sized,
{
    /// Collect all known case ids for a batch (from the batch row, the
    /// created-action index, and the business-case service).
    ///
    /// Widened from private to `pub(super)` so the recovery orchestrator that
    /// remains in `service.rs` (`recover_stale_commits_once`) can call it.
    pub(super) async fn known_commit_case_ids(&self, batch: &AiCopilotBusinessCaseBatch) -> Vec<String> {
        let mut case_ids = Vec::new();
        append_unique_case_ids(&mut case_ids, batch.committed_case_ids.iter().cloned());
        append_unique_case_ids(
            &mut case_ids,
            batch
                .created_action_case_ids
                .as_object()
                .into_iter()
                .flat_map(|values| values.values())
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
        if let Ok(cases) = self.business_case_service.list_by_copilot_batch(&batch.batch_id).await {
            append_unique_case_ids(&mut case_ids, cases.into_iter().map(|case| case.case_id));
        }
        case_ids
    }

    pub async fn list_batches(
        &self,
        status: Option<AiCopilotBatchStatus>,
        workflow_dispatch_status: Option<&str>,
        limit: i64,
        offset: i64,
        access: AiCopilotBatchAccess,
    ) -> Result<AiCopilotBatchListResponse, DomainError> {
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let items = if access.can_access_all() {
            self.repo
                .list(status, workflow_dispatch_status, limit, offset)
                .await?
                .into_iter()
                .map(batch_to_status_response)
                .collect()
        } else {
            let mut visible = Vec::new();
            let mut visible_skipped = 0_i64;
            let mut scan_offset = 0_i64;

            loop {
                let page = self
                    .repo
                    .list(
                        status,
                        workflow_dispatch_status,
                        BATCH_LIST_ACCESS_SCAN_PAGE_SIZE,
                        scan_offset,
                    )
                    .await?;
                let page_len = page.len() as i64;
                if page.is_empty() {
                    break;
                }

                for batch in page {
                    if !access.can_access(&batch) {
                        continue;
                    }
                    if visible_skipped < offset {
                        visible_skipped += 1;
                        continue;
                    }
                    visible.push(batch_to_status_response(batch));
                    if visible.len() >= limit as usize {
                        break;
                    }
                }

                if visible.len() >= limit as usize || page_len < BATCH_LIST_ACCESS_SCAN_PAGE_SIZE {
                    break;
                }
                scan_offset += page_len;
            }

            visible
        };
        Ok(AiCopilotBatchListResponse { items, limit, offset })
    }

    pub async fn get_batch_status(
        &self,
        batch_id: &str,
        access: AiCopilotBatchAccess,
    ) -> Result<AiCopilotBatchStatusResponse, DomainError> {
        let batch = self
            .repo
            .find_by_id(batch_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "ai_copilot_business_case_batch",
                id: batch_id.to_string(),
            })?;
        ensure_batch_visible(&batch, &access)?;

        Ok(batch_to_status_response(batch))
    }

    pub async fn operational_metrics(
        &self,
        max_workflow_dispatch_attempts: i32,
        recent_error_limit: i64,
    ) -> Result<AiCopilotOperationalMetricsResponse, DomainError> {
        self.repo
            .operational_metrics(max_workflow_dispatch_attempts, recent_error_limit)
            .await
    }

    pub async fn resolve_failed_batch(
        &self,
        batch_id: &str,
        request: AiCopilotFailedBatchResolutionRequest,
        actor: &str,
        access: AiCopilotBatchAccess,
    ) -> Result<AiCopilotBatchStatusResponse, DomainError> {
        let batch = self
            .repo
            .find_by_id(batch_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "ai_copilot_business_case_batch",
                id: batch_id.to_string(),
            })?;
        ensure_batch_ops_access(&batch, &access)?;

        if batch.status != AiCopilotBatchStatus::Failed {
            return Err(DomainError::ValidationError(
                "只有 failed 状态的批次允许执行失败处理".into(),
            ));
        }

        let note = request
            .note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let resolution = json!({
            "previous_error": batch.commit_error,
            "resolution": request.action,
            "note": note,
            "resolved_by": actor.trim(),
            "resolved_at": Utc::now(),
        });

        let updated = match request.action {
            AiCopilotFailedBatchResolutionAction::MarkResolved => {
                self.repo.mark_failed_resolved(batch_id, &resolution).await?
            }
            AiCopilotFailedBatchResolutionAction::ResetToDraft => {
                if !batch.committed_case_ids.is_empty() {
                    return Err(DomainError::ValidationError(
                        "该失败批次已有部分业务事项，需人工处理后标记已处理，不能自动重置为草稿".into(),
                    ));
                }
                self.repo.reset_failed_to_draft(batch_id, &resolution).await?
            }
        }
        .ok_or_else(|| DomainError::Internal("failed to update failed copilot batch".into()))?;

        Ok(batch_to_status_response(updated))
    }
}
