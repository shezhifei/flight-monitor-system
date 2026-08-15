//! Integration test for AI run concurrency limits on the run creation
//! path (`AiJobService::create_run`).
//!
//! Covers the hybrid agent architecture plan (Task B4) performance
//! contract: per-entity ceiling (default 4) x global ceiling
//! (default 32), enforced before the `ai_runs` row is inserted, with an
//! explicit `concurrency_limit_exceeded` error distinguishing the
//! entity vs global scope and carrying current/limit values.
//!
//! Uses an in-memory `AiRunRepository` with the same active-status and
//! entity-extraction semantics as `PgAiRunRepository::count_active`, so
//! no live database is required.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};

use fms_application::services::ai_job_service::{
    AiJobService, AiJobServiceError, AiRunConcurrencyLimits, ConcurrencyLimitScope,
};
use fms_domain::models::ai_job::{AiJobRecord, AiJobStatusCount, AiRunEventRecord, AiRunRecord, AiRunStatus};
use fms_domain::ports::ai_job_repository::{AiJobRepository, AiJobRepositoryError};
use fms_domain::ports::ai_run_event_repository::{AiRunEventRepository, AiRunEventRepositoryError};
use fms_domain::ports::ai_run_repository::{AiRunRepository, AiRunRepositoryError};

/// In-memory run repository mirroring the `count_active` semantics of
/// `PgAiRunRepository`: active = pending/claimed/running; entity id from
/// `input_envelope.entity_id` or `input_envelope.context.entity_id`.
struct InMemoryRunRepository {
    runs: Mutex<Vec<AiRunRecord>>,
}

impl InMemoryRunRepository {
    fn new() -> Self {
        Self {
            runs: Mutex::new(Vec::new()),
        }
    }
}

fn envelope_entity_id(envelope: &Value) -> Option<&str> {
    envelope.get("entity_id").and_then(|v| v.as_str()).or_else(|| {
        envelope
            .get("context")
            .and_then(|c| c.get("entity_id"))
            .and_then(|v| v.as_str())
    })
}

#[async_trait]
impl AiRunRepository for InMemoryRunRepository {
    async fn insert(
        &self,
        run_id: &str,
        job_id: &str,
        runtime_engine: &str,
        model_id: Option<&str>,
        input_envelope: Option<Value>,
    ) -> Result<AiRunRecord, AiRunRepositoryError> {
        let record = AiRunRecord {
            run_id: run_id.to_string(),
            job_id: job_id.to_string(),
            runtime_engine: runtime_engine.to_string(),
            model_id: model_id.map(str::to_string),
            status: AiRunStatus::Pending.as_str().to_string(),
            input_envelope,
            output_raw: None,
            output_validated: None,
            token_usage: None,
            started_at: None,
            finished_at: None,
            error_code: None,
            error_message: None,
            created_at: Utc::now(),
        };
        self.runs.lock().unwrap().push(record.clone());
        Ok(record)
    }

    async fn find_by_id(&self, run_id: &str) -> Result<Option<AiRunRecord>, AiRunRepositoryError> {
        Ok(self.runs.lock().unwrap().iter().find(|r| r.run_id == run_id).cloned())
    }

    async fn list_for_job(&self, job_id: &str) -> Result<Vec<AiRunRecord>, AiRunRepositoryError> {
        Ok(self
            .runs
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.job_id == job_id)
            .cloned()
            .collect())
    }

    async fn count_active(&self, entity_id: Option<&str>) -> Result<i64, AiRunRepositoryError> {
        let runs = self.runs.lock().unwrap();
        let count = runs
            .iter()
            .filter(|r| AiRunStatus::from_str(&r.status).map(|s| s.is_active()).unwrap_or(false))
            .filter(|r| match entity_id {
                Some(entity_id) => r
                    .input_envelope
                    .as_ref()
                    .and_then(envelope_entity_id)
                    .map(|eid| eid == entity_id)
                    .unwrap_or(false),
                None => true,
            })
            .count();
        Ok(count as i64)
    }

    async fn update_status(&self, run_id: &str, new_status: &str) -> Result<AiRunRecord, AiRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        let record = runs
            .iter_mut()
            .find(|r| r.run_id == run_id)
            .ok_or_else(|| AiRunRepositoryError::not_found(run_id))?;
        record.status = new_status.to_string();
        Ok(record.clone())
    }

    async fn update_input_envelope(&self, run_id: &str, input_envelope: Value) -> Result<(), AiRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        let record = runs
            .iter_mut()
            .find(|r| r.run_id == run_id)
            .ok_or_else(|| AiRunRepositoryError::not_found(run_id))?;
        record.input_envelope = Some(input_envelope);
        Ok(())
    }

    async fn fill_terminal_outputs(
        &self,
        _run_id: &str,
        _output_raw: Option<Value>,
        _output_validated: Option<Value>,
        _token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }

    async fn complete(
        &self,
        run_id: &str,
        output_raw: Option<Value>,
        output_validated: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        let record = runs
            .iter_mut()
            .find(|r| r.run_id == run_id)
            .ok_or_else(|| AiRunRepositoryError::not_found(run_id))?;
        record.status = AiRunStatus::Succeeded.as_str().to_string();
        record.output_raw = output_raw;
        record.output_validated = output_validated;
        record.token_usage = token_usage;
        record.finished_at = Some(Utc::now());
        Ok(())
    }

    async fn fill_terminal_error(
        &self,
        _run_id: &str,
        _error_code: Option<&str>,
        _error_message: Option<&str>,
        _output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        Ok(())
    }

    async fn fail(
        &self,
        run_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        output_raw: Option<Value>,
    ) -> Result<(), AiRunRepositoryError> {
        let mut runs = self.runs.lock().unwrap();
        let record = runs
            .iter_mut()
            .find(|r| r.run_id == run_id)
            .ok_or_else(|| AiRunRepositoryError::not_found(run_id))?;
        record.status = AiRunStatus::FailedTerminal.as_str().to_string();
        record.error_code = error_code.map(str::to_string);
        record.error_message = error_message.map(str::to_string);
        record.output_raw = output_raw;
        record.finished_at = Some(Utc::now());
        Ok(())
    }
}

/// Unused by `create_run`; every method fails loudly if reached.
struct StubJobRepository;

#[async_trait]
impl AiJobRepository for StubJobRepository {
    async fn insert(
        &self,
        _job_id: &str,
        _job_type: &str,
        _requester_user_id: Option<&str>,
        _correlation_id: Option<&str>,
        _ontology_version: Option<&str>,
        _risk_ceiling: Option<&str>,
    ) -> Result<AiJobRecord, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn find_by_id(&self, _job_id: &str) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn list(
        &self,
        _status_filter: Option<&str>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn update_status(&self, _job_id: &str, _new_status: &str) -> Result<AiJobRecord, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn set_error_message(&self, _job_id: &str, _error_message: &str) -> Result<(), AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn claim_pending(&self, _job_type: Option<&str>) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn lease_pending(
        &self,
        _job_type: Option<&str>,
        _lease_owner: &str,
        _lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn list_expired_leases(
        &self,
        _now: chrono::DateTime<Utc>,
        _limit: i64,
    ) -> Result<Vec<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn heartbeat(
        &self,
        _job_id: &str,
        _lease_owner: &str,
        _lease_seconds: i64,
    ) -> Result<bool, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn take_over(
        &self,
        _job_id: &str,
        _new_owner: &str,
        _lease_seconds: i64,
    ) -> Result<Option<AiJobRecord>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }

    async fn count_by_status(&self) -> Result<Vec<AiJobStatusCount>, AiJobRepositoryError> {
        Err(AiJobRepositoryError::Database("stub".into()))
    }
}

/// Unused by `create_run`; every method fails loudly if reached.
struct StubRunEventRepository;

#[async_trait]
impl AiRunEventRepository for StubRunEventRepository {
    async fn insert(
        &self,
        _job_id: &str,
        _run_id: &str,
        _event_type: &str,
        _payload: Option<Value>,
    ) -> Result<AiRunEventRecord, AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("stub".into()))
    }

    async fn insert_fire_and_forget(
        &self,
        _job_id: &str,
        _run_id: &str,
        _event_type: &str,
        _payload: Option<Value>,
    ) -> Result<(), AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("stub".into()))
    }

    async fn list_for_run(
        &self,
        _run_id: &str,
        _limit: i64,
    ) -> Result<Vec<AiRunEventRecord>, AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("stub".into()))
    }

    async fn count_by_job_ids_before(
        &self,
        _job_ids: &[String],
        _older_than: chrono::DateTime<Utc>,
    ) -> Result<i64, AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("stub".into()))
    }

    async fn delete_by_job_ids_before(
        &self,
        _job_ids: &[String],
        _older_than: chrono::DateTime<Utc>,
    ) -> Result<u64, AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("stub".into()))
    }

    async fn count_smoke_readiness_blocks(&self, _event_type: &str) -> Result<i64, AiRunEventRepositoryError> {
        Err(AiRunEventRepositoryError::Database("stub".into()))
    }
}

fn build_service(limits: AiRunConcurrencyLimits) -> (AiJobService, Arc<InMemoryRunRepository>) {
    let run_repo = Arc::new(InMemoryRunRepository::new());
    let service = AiJobService::new(
        Arc::new(StubJobRepository),
        run_repo.clone() as Arc<dyn AiRunRepository + Send + Sync>,
        Arc::new(StubRunEventRepository),
    )
    .with_concurrency_limits(limits);
    (service, run_repo)
}

fn entity_envelope(entity_id: &str) -> Value {
    json!({ "entity_id": entity_id })
}

#[tokio::test]
async fn per_entity_limit_rejects_run_beyond_ceiling() {
    let (service, _runs) = build_service(AiRunConcurrencyLimits {
        max_concurrent_runs_per_entity: 2,
        max_concurrent_runs_global: 32,
    });

    for i in 0..2 {
        service
            .create_run(
                &format!("job-{i}"),
                "python-ai-runtime",
                None,
                Some(entity_envelope("anomaly_ops")),
            )
            .await
            .expect("runs under the per-entity ceiling are created");
    }

    let err = service
        .create_run("job-2", "python-ai-runtime", None, Some(entity_envelope("anomaly_ops")))
        .await
        .expect_err("third run for the same entity must be rejected");
    match err {
        AiJobServiceError::ConcurrencyLimitExceeded { scope, current, limit } => {
            assert_eq!(scope, ConcurrencyLimitScope::Entity);
            assert_eq!(current, 2);
            assert_eq!(limit, 2);
        }
        other => panic!("expected ConcurrencyLimitExceeded, got {other}"),
    }
    assert!(err.to_string().contains("concurrency_limit_exceeded"));

    // A different entity is unaffected by the per-entity ceiling.
    service
        .create_run("job-3", "python-ai-runtime", None, Some(entity_envelope("query_ops")))
        .await
        .expect("other entities still create runs");
}

#[tokio::test]
async fn nested_context_entity_id_counts_towards_per_entity_limit() {
    let (service, _runs) = build_service(AiRunConcurrencyLimits {
        max_concurrent_runs_per_entity: 1,
        max_concurrent_runs_global: 32,
    });

    service
        .create_run(
            "job-1",
            "python-ai-runtime",
            None,
            Some(json!({ "context": { "entity_id": "dispatch_ops" } })),
        )
        .await
        .unwrap();

    let err = service
        .create_run(
            "job-2",
            "python-ai-runtime",
            None,
            Some(entity_envelope("dispatch_ops")),
        )
        .await
        .expect_err("nested context.entity_id must count towards the same entity bucket");
    assert!(matches!(
        err,
        AiJobServiceError::ConcurrencyLimitExceeded {
            scope: ConcurrencyLimitScope::Entity,
            ..
        }
    ));
}

#[tokio::test]
async fn global_limit_rejects_run_beyond_ceiling() {
    let (service, _runs) = build_service(AiRunConcurrencyLimits {
        max_concurrent_runs_per_entity: 4,
        max_concurrent_runs_global: 2,
    });

    service
        .create_run("job-1", "python-ai-runtime", None, Some(entity_envelope("anomaly_ops")))
        .await
        .unwrap();
    service
        .create_run("job-2", "python-ai-runtime", None, Some(entity_envelope("query_ops")))
        .await
        .unwrap();

    // Runs without an entity id are still bounded by the global ceiling.
    let err = service
        .create_run("job-3", "python-ai-runtime", None, None)
        .await
        .expect_err("third active run must hit the global ceiling");
    match err {
        AiJobServiceError::ConcurrencyLimitExceeded { scope, current, limit } => {
            assert_eq!(scope, ConcurrencyLimitScope::Global);
            assert_eq!(current, 2);
            assert_eq!(limit, 2);
        }
        other => panic!("expected ConcurrencyLimitExceeded, got {other}"),
    }
}

#[tokio::test]
async fn runs_under_limits_create_and_terminal_runs_free_capacity() {
    let (service, run_repo) = build_service(AiRunConcurrencyLimits {
        max_concurrent_runs_per_entity: 1,
        max_concurrent_runs_global: 4,
    });

    let run = service
        .create_run("job-1", "python-ai-runtime", None, Some(entity_envelope("anomaly_ops")))
        .await
        .expect("first run under the ceiling is created");
    assert_eq!(run.status, "pending");

    // Terminal runs no longer occupy capacity.
    run_repo.complete(&run.run_id, None, None, None).await.unwrap();

    service
        .create_run("job-2", "python-ai-runtime", None, Some(entity_envelope("anomaly_ops")))
        .await
        .expect("capacity freed by the terminal run allows a new run");
}

#[tokio::test]
async fn default_limits_match_plan_budget() {
    let limits = AiRunConcurrencyLimits::default();
    assert_eq!(limits.max_concurrent_runs_per_entity, 4);
    assert_eq!(limits.max_concurrent_runs_global, 32);
}
