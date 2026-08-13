//! `CompensationPlanner` — derives an `AiCompensationPlanRecord` for
//! an executed domain action.
//!
//! The planner is pure: given the receipt, the ontology's
//! `CompensationMetadata`, and the before snapshot, it produces a
//! plan (or `None` for irreversible actions). It does not touch the
//! database; the `AiActionProposalService::execute_proposal_with_receipt`
//! wrapper persists the result. The planner refuses to build a
//! `restore_snapshot` plan if the object's current version has drifted
//! past the snapshot — that surfaces as `CompensationError::ObjectVersionConflict`.

use chrono::Utc;
use serde_json::{json, Value};
use thiserror::Error;
use ulid::Ulid;

use fms_domain::models::ai_execution::{
    AiActionReceiptRecord, AiCompensationMode, AiCompensationPlanRecord, AiCompensationStatus,
};
use fms_domain::models::ai_ontology::CompensationMetadata;
use fms_domain::ports::ai_execution_repository::AiExecutionRepositoryError;

#[derive(Debug, Error)]
pub enum CompensationError {
    #[error("compensation is not allowed for mode {mode}: {reason}")]
    NotAllowed { mode: String, reason: String },
    #[error("before_snapshot_required for mode {mode} but no snapshot was supplied")]
    SnapshotMissing { mode: String },
    #[error("invalid mode string: {0}")]
    InvalidMode(String),
    #[error("invalid inverse action metadata: {0}")]
    InvalidInverse(String),
    #[error("object version conflict: expected {expected_version}, current {current_version}")]
    ObjectVersionConflict {
        object_type: String,
        object_id: String,
        expected_version: i64,
        current_version: i64,
    },
    #[error("repository error: {0}")]
    Repository(#[from] AiExecutionRepositoryError),
}

impl CompensationError {
    pub fn is_version_conflict(&self) -> bool {
        matches!(self, Self::ObjectVersionConflict { .. })
    }
}

/// Read-side hook used by the planner to fetch the current object
/// version. The in-memory implementation is straightforward; the
/// Postgres implementation will use a read-only `SELECT version FROM
/// <table>`.
#[async_trait::async_trait]
pub trait ObjectVersionLookup: Send + Sync {
    async fn current_version(
        &self,
        object_type: &str,
        object_id: &str,
    ) -> Result<Option<i64>, AiExecutionRepositoryError>;
}

/// Default in-memory implementation backed by a static map; used by
/// tests. The production `server` composition root wires the real
/// Postgres-backed implementation.
#[derive(Debug, Default)]
pub struct InMemoryObjectVersionLookup {
    versions: std::sync::Mutex<std::collections::HashMap<(String, String), i64>>,
}

impl InMemoryObjectVersionLookup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, object_type: &str, object_id: &str, version: i64) {
        let mut map = self.versions.lock().expect("version map poisoned");
        map.insert((object_type.to_string(), object_id.to_string()), version);
    }
}

#[async_trait::async_trait]
impl ObjectVersionLookup for InMemoryObjectVersionLookup {
    async fn current_version(
        &self,
        object_type: &str,
        object_id: &str,
    ) -> Result<Option<i64>, AiExecutionRepositoryError> {
        let map = self.versions.lock().expect("version map poisoned");
        Ok(map.get(&(object_type.to_string(), object_id.to_string())).copied())
    }
}

pub struct CompensationPlanner {
    version_lookup: std::sync::Arc<dyn ObjectVersionLookup>,
}

impl std::fmt::Debug for CompensationPlanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompensationPlanner").finish_non_exhaustive()
    }
}

impl CompensationPlanner {
    pub fn new(version_lookup: std::sync::Arc<dyn ObjectVersionLookup>) -> Self {
        Self { version_lookup }
    }

    /// Build a compensation plan for `receipt`. Returns `Ok(None)` for
    /// actions whose compensation metadata marks them as
    /// `irreversible` (e.g. `Notification.send`) — the caller records
    /// the lack of a plan in the audit trail but does not surface a
    /// `CompensationError` for those.
    pub async fn plan(
        &self,
        receipt: &AiActionReceiptRecord,
        metadata: &CompensationMetadata,
        before_snapshot: &Value,
    ) -> Result<Option<AiCompensationPlanRecord>, CompensationError> {
        let mode = parse_mode(&metadata.mode).ok_or_else(|| CompensationError::InvalidMode(metadata.mode.clone()))?;

        if matches!(mode, AiCompensationMode::Irreversible) {
            return Ok(None);
        }

        if matches!(mode, AiCompensationMode::RestoreSnapshot)
            && metadata.before_snapshot_required
            && (before_snapshot.is_null() || !before_snapshot.is_object())
        {
            return Err(CompensationError::SnapshotMissing {
                mode: mode.as_str().to_string(),
            });
        }

        if matches!(mode, AiCompensationMode::RestoreSnapshot) {
            let expected = before_snapshot.get("version").and_then(Value::as_i64).unwrap_or(0);
            if expected > 0 {
                let current = self
                    .version_lookup
                    .current_version(&receipt.object_type, &receipt.object_id)
                    .await?;
                if let Some(actual) = current {
                    if actual != expected {
                        return Err(CompensationError::ObjectVersionConflict {
                            object_type: receipt.object_type.clone(),
                            object_id: receipt.object_id.clone(),
                            expected_version: expected,
                            current_version: actual,
                        });
                    }
                }
            }
        }

        let plan_json = self.build_plan(mode, metadata, receipt, before_snapshot)?;
        let now = Utc::now();
        let plan = AiCompensationPlanRecord {
            compensation_id: format!("cmp_{}", Ulid::new()),
            receipt_id: receipt.receipt_id.clone(),
            proposal_id: receipt.proposal_id.clone(),
            status: AiCompensationStatus::Planned,
            mode,
            plan: plan_json,
            requires_approval: metadata.requires_approval,
            approved_by: None,
            approved_at: None,
            executed_by: None,
            executed_at: None,
            execution_result: None,
            execution_error: None,
            created_at: now,
            updated_at: now,
        };
        Ok(Some(plan))
    }

    fn build_plan(
        &self,
        mode: AiCompensationMode,
        metadata: &CompensationMetadata,
        receipt: &AiActionReceiptRecord,
        before_snapshot: &Value,
    ) -> Result<Value, CompensationError> {
        match mode {
            AiCompensationMode::RestoreSnapshot => {
                let expected_version = before_snapshot.get("version").and_then(Value::as_i64).unwrap_or(0);
                Ok(json!({
                    "object_type": receipt.object_type,
                    "object_id": receipt.object_id,
                    "expected_version": expected_version,
                    "before_snapshot": before_snapshot,
                    "irreversible_fields": metadata.irreversible_fields,
                }))
            }
            AiCompensationMode::InverseAction => {
                let inverse = metadata.inverse_action_name.as_deref().ok_or_else(|| {
                    CompensationError::InvalidInverse(format!(
                        "{}.{} marked as inverse_action without inverse_action_name",
                        receipt.object_type, receipt.action_name
                    ))
                })?;
                Ok(json!({
                    "inverse_action_name": inverse,
                    "object_type": receipt.object_type,
                    "object_id": receipt.object_id,
                    "args_from_receipt": receipt.execution_result,
                }))
            }
            AiCompensationMode::FollowupAction => {
                let corrective = metadata.followup_action_name.as_deref().ok_or_else(|| {
                    CompensationError::InvalidInverse(format!(
                        "{}.{} marked as followup_action without followup_action_name",
                        receipt.object_type, receipt.action_name
                    ))
                })?;
                let args = metadata.followup_args.clone().unwrap_or_else(|| {
                    json!({
                        "object_type": receipt.object_type,
                        "object_id": receipt.object_id,
                        "original_receipt_id": receipt.receipt_id,
                    })
                });
                Ok(json!({
                    "corrective_action_name": corrective,
                    "object_type": receipt.object_type,
                    "object_id": receipt.object_id,
                    "followup_args": args,
                }))
            }
            AiCompensationMode::Irreversible => Ok(Value::Null),
        }
    }
}

fn parse_mode(raw: &str) -> Option<AiCompensationMode> {
    AiCompensationMode::from_str(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn receipt() -> AiActionReceiptRecord {
        AiActionReceiptRecord {
            receipt_id: "rcp-1".into(),
            proposal_id: "prop-1".into(),
            job_id: "job-1".into(),
            run_id: "run-1".into(),
            tool_call_pk: Some("tpc-1".into()),
            object_type: "Flight".into(),
            object_id: "flt-1".into(),
            action_name: "update_status".into(),
            idempotency_key: "idem-1".into(),
            before_checkpoint_id: Some("cp-before".into()),
            after_checkpoint_id: Some("cp-after".into()),
            outbox_event_id: Some("evt-1".into()),
            execution_result: json!({"status": "BOARDING"}),
            executed_by: "executor-1".into(),
            executed_at: Utc::now(),
        }
    }

    fn restore_metadata(requires_approval: bool) -> CompensationMetadata {
        CompensationMetadata {
            mode: "restore_snapshot".to_string(),
            requires_approval,
            irreversible_fields: Vec::new(),
            inverse_action_name: None,
            before_snapshot_required: true,
            followup_action_name: None,
            followup_args: None,
        }
    }

    #[tokio::test]
    async fn restore_snapshot_emits_plan_with_expected_version() {
        let lookup = std::sync::Arc::new(InMemoryObjectVersionLookup::new());
        lookup.set("Flight", "flt-1", 7);
        let planner = CompensationPlanner::new(lookup);
        let snapshot = json!({"version": 7, "status": "PLAN"});
        let plan = planner
            .plan(&receipt(), &restore_metadata(true), &snapshot)
            .await
            .unwrap()
            .expect("plan should exist");
        assert_eq!(plan.mode, AiCompensationMode::RestoreSnapshot);
        assert!(plan.requires_approval);
        assert_eq!(plan.plan["expected_version"], 7);
        assert_eq!(plan.plan["object_type"], "Flight");
    }

    #[tokio::test]
    async fn restore_snapshot_fails_with_object_version_conflict_when_drifted() {
        let lookup = std::sync::Arc::new(InMemoryObjectVersionLookup::new());
        lookup.set("Flight", "flt-1", 9);
        let planner = CompensationPlanner::new(lookup);
        let snapshot = json!({"version": 7, "status": "PLAN"});
        let err = planner
            .plan(&receipt(), &restore_metadata(true), &snapshot)
            .await
            .expect_err("drift must surface ObjectVersionConflict");
        assert!(err.is_version_conflict());
        match err {
            CompensationError::ObjectVersionConflict {
                expected_version,
                current_version,
                ..
            } => {
                assert_eq!(expected_version, 7);
                assert_eq!(current_version, 9);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn restore_snapshot_requires_non_empty_snapshot() {
        let planner = CompensationPlanner::new(std::sync::Arc::new(InMemoryObjectVersionLookup::new()));
        let err = planner
            .plan(&receipt(), &restore_metadata(true), &Value::Null)
            .await
            .expect_err("missing snapshot must error");
        assert!(matches!(err, CompensationError::SnapshotMissing { .. }));
    }

    #[tokio::test]
    async fn inverse_action_emits_plan_with_inverse_name_and_args_from_receipt() {
        let planner = CompensationPlanner::new(std::sync::Arc::new(InMemoryObjectVersionLookup::new()));
        let mut r = receipt();
        r.object_type = "Todo".into();
        r.action_name = "complete".into();
        let metadata = CompensationMetadata {
            mode: "inverse_action".to_string(),
            requires_approval: false,
            irreversible_fields: Vec::new(),
            inverse_action_name: Some("Todo.reopen".to_string()),
            before_snapshot_required: false,
            followup_action_name: None,
            followup_args: None,
        };
        let plan = planner
            .plan(&r, &metadata, &Value::Null)
            .await
            .unwrap()
            .expect("plan should exist");
        assert_eq!(plan.mode, AiCompensationMode::InverseAction);
        assert_eq!(plan.plan["inverse_action_name"], "Todo.reopen");
        assert_eq!(plan.plan["object_type"], "Todo");
        assert!(!plan.requires_approval);
    }

    #[tokio::test]
    async fn inverse_action_errors_when_inverse_name_missing() {
        let planner = CompensationPlanner::new(std::sync::Arc::new(InMemoryObjectVersionLookup::new()));
        let mut r = receipt();
        r.object_type = "Todo".into();
        r.action_name = "complete".into();
        let metadata = CompensationMetadata {
            mode: "inverse_action".to_string(),
            requires_approval: false,
            irreversible_fields: Vec::new(),
            inverse_action_name: None,
            before_snapshot_required: false,
            followup_action_name: None,
            followup_args: None,
        };
        let err = planner
            .plan(&r, &metadata, &Value::Null)
            .await
            .expect_err("missing inverse name must error");
        assert!(matches!(err, CompensationError::InvalidInverse(_)));
    }

    #[tokio::test]
    async fn irreversible_metadata_returns_none_plan() {
        let planner = CompensationPlanner::new(std::sync::Arc::new(InMemoryObjectVersionLookup::new()));
        let mut r = receipt();
        r.object_type = "Notification".into();
        r.action_name = "send".into();
        let metadata = CompensationMetadata {
            mode: "irreversible".to_string(),
            requires_approval: true,
            irreversible_fields: vec!["body".to_string()],
            inverse_action_name: None,
            before_snapshot_required: false,
            followup_action_name: None,
            followup_args: None,
        };
        let plan = planner.plan(&r, &metadata, &Value::Null).await.unwrap();
        assert!(plan.is_none());
    }

    #[tokio::test]
    async fn followup_action_emits_plan_with_corrective_action() {
        let planner = CompensationPlanner::new(std::sync::Arc::new(InMemoryObjectVersionLookup::new()));
        let metadata = CompensationMetadata {
            mode: "followup_action".to_string(),
            requires_approval: true,
            irreversible_fields: Vec::new(),
            inverse_action_name: None,
            before_snapshot_required: false,
            followup_action_name: Some("Anomaly.create_correction".to_string()),
            followup_args: Some(json!({"reason": "compensate"})),
        };
        let plan = planner
            .plan(&receipt(), &metadata, &Value::Null)
            .await
            .unwrap()
            .expect("plan should exist");
        assert_eq!(plan.mode, AiCompensationMode::FollowupAction);
        assert_eq!(plan.plan["corrective_action_name"], "Anomaly.create_correction");
        assert_eq!(plan.plan["followup_args"]["reason"], "compensate");
    }

    #[tokio::test]
    async fn invalid_mode_string_is_rejected() {
        let planner = CompensationPlanner::new(std::sync::Arc::new(InMemoryObjectVersionLookup::new()));
        let metadata = CompensationMetadata {
            mode: "nonsense".to_string(),
            requires_approval: false,
            irreversible_fields: Vec::new(),
            inverse_action_name: None,
            before_snapshot_required: false,
            followup_action_name: None,
            followup_args: None,
        };
        let err = planner
            .plan(&receipt(), &metadata, &Value::Null)
            .await
            .expect_err("unknown mode must error");
        assert!(matches!(err, CompensationError::InvalidMode(_)));
    }
}
