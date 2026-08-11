use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;

use super::service::LLMEvalService;
use super::types::{EvalJob, EvalProgress};
use crate::schemas::llm_eval_schemas::EvalRunOptionsRequest;

fn completed_job(job_id: &str, created_at: &str) -> EvalJob {
    EvalJob {
        job_id: job_id.to_string(),
        status: "completed".to_string(),
        created_at: created_at.to_string(),
        started_at: Some(created_at.to_string()),
        finished_at: Some(created_at.to_string()),
        owner: json!({ "user_id": "test-user", "roles": [] }),
        options: EvalRunOptionsRequest::default(),
        suite: json!({ "suite_id": "quick", "total_cases": 0, "case_ids": [] }),
        progress: EvalProgress {
            completed_attempts: 0,
            total_attempts: 0,
            percentage: 100.0,
        },
        profiles: Vec::new(),
        ranking: Vec::new(),
        error_message: None,
    }
}

#[tokio::test]
async fn prune_jobs_removes_runtime_state_and_task_handles_for_pruned_jobs() {
    let service = LLMEvalService::new(5, None);
    let pruned_job_id = "eval_00";

    for index in 0..6 {
        let job_id = format!("eval_{index:02}");
        service.state.jobs.insert(
            job_id.clone(),
            completed_job(&job_id, &format!("2026-06-14T00:00:0{index}Z")),
        );
        service.state.runtime_profiles.insert(job_id.clone(), Vec::new());
        service.state.runtime_cases.insert(job_id.clone(), Vec::new());
        service
            .state
            .cancel_flags
            .insert(job_id.clone(), Arc::new(AtomicBool::new(false)));
        service
            .state
            .tasks
            .insert(job_id, tokio::spawn(async { std::future::pending::<()>().await }));
    }

    service.prune_jobs();

    assert!(!service.state.jobs.contains_key(pruned_job_id));
    assert!(!service.state.runtime_profiles.contains_key(pruned_job_id));
    assert!(!service.state.runtime_cases.contains_key(pruned_job_id));
    assert!(!service.state.cancel_flags.contains_key(pruned_job_id));
    assert!(
        !service.state.tasks.contains_key(pruned_job_id),
        "pruned jobs should not leave task handles behind"
    );
}
