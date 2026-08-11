use std::sync::atomic::Ordering;

use serde_json::{json, Map, Value};

use super::case_loader::{round_f64, value_f64, value_str};
use super::service::LLMEvalService;
use super::types::{EvalCaseDefinition, EvalJob, EvalProfileSnapshot};

impl LLMEvalService {
    pub(crate) fn job_snapshot(&self, job: &EvalJob, include_profiles: bool) -> Value {
        if include_profiles {
            json!({
                "job_id": job.job_id,
                "status": job.status,
                "created_at": job.created_at,
                "started_at": job.started_at,
                "finished_at": job.finished_at,
                "suite": job.suite,
                "options": job.options,
                "progress": job.progress,
                "ranking": job.ranking,
                "error_message": job.error_message,
                "profiles": job.profiles,
            })
        } else {
            json!({
                "job_id": job.job_id,
                "status": job.status,
                "created_at": job.created_at,
                "started_at": job.started_at,
                "finished_at": job.finished_at,
                "suite": job.suite,
                "options": job.options,
                "progress": job.progress,
                "ranking": job.ranking,
                "error_message": job.error_message,
                "profiles": job.profiles.iter().map(|item| {
                    json!({
                        "profile_id": item.profile_id,
                        "name": item.name,
                        "model": item.model,
                        "status": item.status,
                        "progress": item.progress,
                        "metrics": item.metrics,
                        "error_message": item.error_message,
                    })
                }).collect::<Vec<_>>(),
            })
        }
    }

    pub(crate) fn prune_jobs(&self) {
        let len = self.state.jobs.len();
        if len <= self.max_retained_jobs {
            return;
        }
        let mut sorted: Vec<_> = self
            .state
            .jobs
            .iter()
            .map(|entry| (entry.value().created_at.clone(), entry.key().clone()))
            .collect();
        sorted.sort_by(|left, right| left.0.cmp(&right.0));
        let removable = len - self.max_retained_jobs;
        let mut removed = 0usize;
        for (_, job_id) in sorted {
            if removed >= removable {
                break;
            }
            if let Some(entry) = self.state.jobs.get(&job_id) {
                if matches!(entry.value().status.as_str(), "pending" | "running" | "cancelling") {
                    continue;
                }
            }
            self.remove_job_runtime_state(&job_id);
            removed += 1;
        }
    }

    pub(crate) fn remove_job_runtime_state(&self, job_id: &str) {
        self.state.jobs.remove(job_id);
        self.state.runtime_profiles.remove(job_id);
        self.state.runtime_cases.remove(job_id);
        self.state.cancel_flags.remove(job_id);
        if let Some((_job_id, handle)) = self.state.tasks.remove(job_id) {
            handle.abort();
        }
    }

    pub(crate) async fn is_cancelled(&self, job_id: &str) -> bool {
        self.state
            .cancel_flags
            .get(job_id)
            .map(|entry| entry.value().load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub(crate) async fn update_profile_status(&self, job_id: &str, profile_id: &str, status: &str) {
        if let Some(mut job_entry) = self.state.jobs.get_mut(job_id) {
            let job = job_entry.value_mut();
            if let Some(profile) = job.profiles.iter_mut().find(|item| item.profile_id == profile_id) {
                profile.status = status.to_string();
            }
        }
    }

    pub(crate) async fn increment_progress(&self, job_id: &str, profile_id: &str) {
        let Some(mut job_entry) = self.state.jobs.get_mut(job_id) else {
            return;
        };
        let job = job_entry.value_mut();
        job.progress.completed_attempts += 1;
        if job.progress.total_attempts > 0 {
            job.progress.percentage = round_f64(
                (job.progress.completed_attempts as f64 / job.progress.total_attempts as f64) * 100.0,
                2,
            );
        }
        if let Some(profile) = job.profiles.iter_mut().find(|item| item.profile_id == profile_id) {
            profile.progress.completed_attempts += 1;
            if profile.progress.total_attempts > 0 {
                profile.progress.percentage = round_f64(
                    (profile.progress.completed_attempts as f64 / profile.progress.total_attempts as f64) * 100.0,
                    2,
                );
            }
        }
    }

    pub(crate) async fn set_profile_result(
        &self,
        job_id: &str,
        profile_id: &str,
        status: &str,
        metrics: Option<Value>,
        case_results: Vec<Value>,
        error_message: Option<String>,
    ) {
        let Some(mut job_entry) = self.state.jobs.get_mut(job_id) else {
            return;
        };
        let job = job_entry.value_mut();
        if let Some(profile) = job.profiles.iter_mut().find(|item| item.profile_id == profile_id) {
            profile.status = status.to_string();
            profile.metrics = metrics;
            profile.cases = case_results;
            profile.error_message = error_message;
            if status == "completed" {
                profile.progress.completed_attempts = profile.progress.total_attempts;
                profile.progress.percentage = 100.0;
            }
        }
    }

    pub(crate) fn tool_names_for_case(&self, case: &EvalCaseDefinition) -> Vec<String> {
        let mut names = case.expected_tools.clone();
        names.push("QUERY".to_string());
        if case.eval_type == "text2sql" {
            names.push("sql_query_readonly".to_string());
        }
        names.sort();
        names.dedup();
        names
    }

    pub(crate) fn chat_tools(&self, case: &EvalCaseDefinition) -> Vec<Value> {
        self.tool_names_for_case(case)
            .into_iter()
            .map(|name| {
                json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": "LLM evaluation routing tool",
                        "parameters": self.tool_parameters(case, &name),
                    }
                })
            })
            .collect()
    }

    pub(crate) fn responses_tools(&self, case: &EvalCaseDefinition) -> Vec<Value> {
        self.tool_names_for_case(case)
            .into_iter()
            .map(|name| {
                json!({
                    "type": "function",
                    "name": name,
                    "description": "LLM evaluation routing tool",
                    "parameters": self.tool_parameters(case, &name),
                })
            })
            .collect()
    }

    pub(crate) fn tool_parameters(&self, case: &EvalCaseDefinition, name: &str) -> Value {
        if name == "sql_query_readonly" {
            return json!({
                "type": "object",
                "properties": { "sql": { "type": "string" } },
                "required": ["sql"],
                "additionalProperties": true
            });
        }
        if name == "QUERY" {
            return json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": [],
                "additionalProperties": true
            });
        }
        let mut properties = Map::new();
        let mut required = Vec::new();
        for expectation in &case.expectations {
            if expectation.key.starts_with("sql_") {
                continue;
            }
            properties.insert(
                expectation.key.clone(),
                json!({ "description": "Auto-inferred parameter for llm-eval" }),
            );
            if expectation.required {
                required.push(expectation.key.clone());
            }
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": true
        })
    }
}
