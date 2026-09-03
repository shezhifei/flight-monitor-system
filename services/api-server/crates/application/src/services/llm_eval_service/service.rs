use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Map, Value};

use super::case_loader::{
    has_value, mask_api_key, mean_f64, mean_i64, percentile_i64, round_f64, value_bool, value_f64, value_str,
    value_to_string, value_to_upper,
};
use super::error::LLMEvalServiceError;
use super::types::{EvalCaseDefinition, EvalJob, EvalProfileSnapshot, EvalProgress, LLMEvalState};
use crate::schemas::llm_eval_schemas::{
    EvalProfileRequest, EvalRunOptionsRequest, LLMEvalCompareResponse, LLMEvalJobCreateResponse,
};

#[derive(Clone)]
pub struct LLMEvalService {
    pub(crate) state: Arc<LLMEvalState>,
    pub(crate) http: reqwest::Client,
    pub(crate) max_retained_jobs: usize,
    pub(crate) cases_file_path: PathBuf,
}

impl LLMEvalService {
    pub fn new(max_retained_jobs: usize, cases_file_path: Option<String>) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let default_cases = root
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("config").join("llm_eval_cases.yaml"))
            .unwrap_or_else(|| PathBuf::from("config/llm_eval_cases.yaml"));

        Self {
            state: Arc::new(LLMEvalState::default()),
            http: Self::build_pooled_reqwest_client(),
            max_retained_jobs: max_retained_jobs.max(5),
            cases_file_path: cases_file_path.map(PathBuf::from).unwrap_or(default_cases),
        }
    }

    pub async fn create_job(
        &self,
        profiles: Vec<EvalProfileRequest>,
        options: EvalRunOptionsRequest,
        owner_id: Option<String>,
        owner_roles: Vec<String>,
    ) -> Result<LLMEvalJobCreateResponse, LLMEvalServiceError> {
        let normalized_profiles = self.normalize_profiles(profiles)?;
        let normalized_options = self.normalize_options(options)?;
        let cases = self.build_suite(&normalized_options.suite)?;
        if normalized_profiles.is_empty() {
            return Err(LLMEvalServiceError::Validation("profiles cannot be empty".to_string()));
        }
        if cases.is_empty() {
            return Err(LLMEvalServiceError::Validation(
                "no evaluation cases available for selected suite".to_string(),
            ));
        }

        let attempts_per_profile = cases.len() as i32 * normalized_options.repeat_count;
        let total_attempts = attempts_per_profile * normalized_profiles.len() as i32;
        let job_id = format!("eval_{}", ulid::Ulid::new());
        let created_at = Utc::now().to_rfc3339();

        let public_profiles = normalized_profiles
            .iter()
            .map(|profile| EvalProfileSnapshot {
                profile_id: profile.profile_id.clone(),
                name: profile.name.clone(),
                base_url: profile.base_url.clone(),
                model: profile.model.clone(),
                timeout: profile.timeout,
                max_retries: profile.max_retries,
                retry_delay: profile.retry_delay,
                api_key_masked: mask_api_key(&profile.api_key),
                status: "pending".to_string(),
                progress: EvalProgress {
                    completed_attempts: 0,
                    total_attempts: attempts_per_profile,
                    percentage: 0.0,
                },
                metrics: None,
                cases: Vec::new(),
                error_message: None,
            })
            .collect::<Vec<_>>();

        let job = EvalJob {
            job_id: job_id.clone(),
            status: "pending".to_string(),
            created_at: created_at.clone(),
            started_at: None,
            finished_at: None,
            owner: json!({ "user_id": owner_id, "roles": owner_roles }),
            options: normalized_options.clone(),
            suite: json!({
                "suite_id": normalized_options.suite,
                "total_cases": cases.len(),
                "case_ids": cases.iter().map(|item| item.case_id.clone()).collect::<Vec<_>>(),
            }),
            progress: EvalProgress {
                completed_attempts: 0,
                total_attempts,
                percentage: 0.0,
            },
            profiles: public_profiles,
            ranking: Vec::new(),
            error_message: None,
        };

        self.state.jobs.insert(job_id.clone(), job);
        self.prune_jobs();
        self.state.runtime_profiles.insert(job_id.clone(), normalized_profiles);
        self.state.runtime_cases.insert(job_id.clone(), cases);
        self.state
            .cancel_flags
            .insert(job_id.clone(), Arc::new(AtomicBool::new(false)));

        let service = self.clone();
        let task_job_id = job_id.clone();
        let handle = tokio::spawn(async move {
            service.run_job(task_job_id).await;
        });
        self.state.tasks.insert(job_id.clone(), handle);

        Ok(LLMEvalJobCreateResponse {
            job_id,
            status: "pending".to_string(),
            created_at,
        })
    }

    pub async fn list_jobs(&self, limit: i32) -> Vec<Value> {
        let safe_limit = limit.clamp(1, 100) as usize;
        let mut jobs = self
            .state
            .jobs
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();
        jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        jobs.into_iter()
            .take(safe_limit)
            .map(|job| self.job_snapshot(&job, false))
            .collect()
    }

    pub async fn get_job(&self, job_id: &str) -> Option<Value> {
        self.state.jobs.get(job_id).map(|job| self.job_snapshot(&job, true))
    }

    pub async fn cancel_job(&self, job_id: &str) -> bool {
        let cancel_flag = self.state.cancel_flags.get(job_id).map(|entry| entry.value().clone());
        let Some(mut job_entry) = self.state.jobs.get_mut(job_id) else {
            return false;
        };
        let job = job_entry.value_mut();
        if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
            return false;
        }
        job.status = "cancelling".to_string();
        if let Some(flag) = cancel_flag {
            flag.store(true, Ordering::SeqCst);
        }
        true
    }

    pub async fn compare_job_profiles(
        &self,
        job_id: &str,
        left_profile_id: Option<&str>,
        right_profile_id: Option<&str>,
    ) -> Result<Option<LLMEvalCompareResponse>, LLMEvalServiceError> {
        let Some(job_entry) = self.state.jobs.get(job_id) else {
            return Ok(None);
        };
        let job = job_entry.value();
        if job.profiles.len() < 2 {
            return Err(LLMEvalServiceError::Validation(
                "at least two profiles are required for comparison".to_string(),
            ));
        }

        let ranking_ids = job
            .ranking
            .iter()
            .filter_map(|row| row.get("profile_id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let resolved_left = left_profile_id
            .map(str::to_string)
            .or_else(|| ranking_ids.first().map(|value| (*value).to_string()))
            .unwrap_or_else(|| job.profiles[0].profile_id.clone());
        let resolved_right = right_profile_id
            .map(str::to_string)
            .or_else(|| ranking_ids.get(1).map(|value| (*value).to_string()))
            .unwrap_or_else(|| job.profiles[1].profile_id.clone());

        let Some(left) = job.profiles.iter().find(|item| item.profile_id == resolved_left) else {
            return Err(LLMEvalServiceError::Validation(format!(
                "left profile not found: {resolved_left}"
            )));
        };
        let Some(right) = job.profiles.iter().find(|item| item.profile_id == resolved_right) else {
            return Err(LLMEvalServiceError::Validation(format!(
                "right profile not found: {resolved_right}"
            )));
        };
        Ok(Some(self.build_compare_payload(left, right)))
    }

    pub(crate) fn evaluate_case(
        &self,
        case: &EvalCaseDefinition,
        observed_tool: Option<&str>,
        observed_arguments: &Value,
    ) -> Value {
        if case.expected_behavior == "fallback" {
            let success = observed_tool.is_none();
            return json!({
                "success": success,
                "case_score": if success { 1.0 } else { 0.0 },
                "tool_match": success,
                "arg_required_score": if success { 1.0 } else { 0.0 },
                "arg_value_score": if success { 1.0 } else { 0.0 },
            });
        }

        let tool_match = observed_tool
            .map(|tool| case.expected_tools.iter().any(|candidate| candidate == tool))
            .unwrap_or(false);
        if case.eval_type == "text2sql" {
            let sql = observed_arguments
                .get("sql")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_uppercase();
            let mut sql_score: f64 = if tool_match && !sql.is_empty() { 1.0 } else { 0.0 };
            if sql_score > 0.0 {
                for expectation in &case.expectations {
                    if expectation.key == "sql_not_contains" {
                        if let Some(contains) = expectation.contains.as_ref() {
                            if sql.contains(&contains.to_uppercase()) {
                                sql_score -= 0.5;
                            }
                        }
                    } else {
                        if let Some(contains) = expectation.contains.as_ref() {
                            if !sql.contains(&contains.to_uppercase()) {
                                sql_score -= 0.2;
                            }
                        }
                        if let Some(expected) = expectation.expected.as_ref() {
                            if !sql.contains(&value_to_upper(expected)) {
                                sql_score -= 0.2;
                            }
                        }
                    }
                }
            }
            sql_score = sql_score.max(0.0);
            return json!({
                "success": tool_match && sql_score >= 0.85,
                "case_score": round_f64(sql_score, 4),
                "tool_match": tool_match,
                "arg_required_score": round_f64(sql_score, 4),
                "arg_value_score": round_f64(sql_score, 4),
            });
        }
        let args_object = observed_arguments.as_object();
        let required_expectations = case
            .expectations
            .iter()
            .filter(|item| item.required)
            .collect::<Vec<_>>();
        let required_hits = required_expectations
            .iter()
            .filter(|expectation| {
                args_object
                    .and_then(|object| object.get(&expectation.key))
                    .map(has_value)
                    .unwrap_or(false)
            })
            .count() as f64;
        let value_expectations = case
            .expectations
            .iter()
            .filter(|item| {
                item.expected.is_some() || item.contains.is_some() || item.one_of.is_some() || item.min_value.is_some()
            })
            .collect::<Vec<_>>();
        let value_hits = value_expectations
            .iter()
            .filter(|expectation| {
                args_object
                    .and_then(|object| object.get(&expectation.key))
                    .map(|value| self.match_expectation(expectation, value))
                    .unwrap_or(false)
            })
            .count() as f64;
        let arg_required_score = if required_expectations.is_empty() {
            1.0
        } else {
            required_hits / required_expectations.len() as f64
        };
        let arg_value_score = if value_expectations.is_empty() {
            1.0
        } else {
            value_hits / value_expectations.len() as f64
        };
        let case_score = round_f64(
            (if tool_match { 0.6 } else { 0.0 }) + (0.25 * arg_required_score) + (0.15 * arg_value_score),
            4,
        );
        json!({
            "success": case_score >= 0.85,
            "case_score": case_score,
            "tool_match": tool_match,
            "arg_required_score": round_f64(arg_required_score, 4),
            "arg_value_score": round_f64(arg_value_score, 4),
        })
    }

    pub(crate) fn match_expectation(&self, expectation: &super::types::ArgExpectation, value: &Value) -> bool {
        if !has_value(value) {
            return false;
        }
        if let Some(expected) = expectation.expected.as_ref() {
            if value != expected {
                return false;
            }
        }
        if let Some(contains) = expectation.contains.as_ref() {
            if !value_to_string(value).contains(contains) {
                return false;
            }
        }
        if let Some(one_of) = expectation.one_of.as_ref() {
            if !one_of.iter().any(|candidate| candidate == value) {
                return false;
            }
        }
        if let Some(min_value) = expectation.min_value {
            let Some(number) = value.as_f64().or_else(|| value_to_string(value).parse::<f64>().ok()) else {
                return false;
            };
            if number < min_value {
                return false;
            }
        }
        true
    }

    pub(crate) fn aggregate_case_results(&self, case: &EvalCaseDefinition, mut attempts: Vec<Value>) -> Value {
        attempts.sort_by_key(|item| item.get("attempt_index").and_then(Value::as_i64).unwrap_or(0));
        let success_count = attempts.iter().filter(|item| value_bool(item, "success")).count();
        let error_count = attempts
            .iter()
            .filter(|item| item.get("error_message").map(has_value).unwrap_or(false))
            .count();
        let fallback_count = attempts.iter().filter(|item| value_bool(item, "fallback_used")).count();
        let score_values = attempts
            .iter()
            .map(|item| value_f64(item, "case_score"))
            .collect::<Vec<_>>();
        let latency_values = attempts
            .iter()
            .map(|item| item.get("latency_ms").and_then(Value::as_i64).unwrap_or(0))
            .collect::<Vec<_>>();
        let successful_attempts = attempts
            .iter()
            .filter(|item| value_bool(item, "success"))
            .collect::<Vec<_>>();
        let mut signature_counter: HashMap<String, usize> = HashMap::new();
        for attempt in &successful_attempts {
            let signature = value_str(attempt, "signature");
            *signature_counter.entry(signature.to_string()).or_default() += 1;
        }
        let consistency = if successful_attempts.is_empty() {
            0.0
        } else {
            signature_counter
                .values()
                .max()
                .map(|count| *count as f64 / successful_attempts.len() as f64)
                .unwrap_or(0.0)
        };
        json!({
            "case_id": case.case_id,
            "prompt": case.prompt,
            "expected_tools": case.expected_tools,
            "tags": case.tags,
            "suites": case.suites,
            "attempts": attempts,
            "summary": {
                "attempt_count": score_values.len(),
                "success_count": success_count,
                "error_count": error_count,
                "fallback_count": fallback_count,
                "success_rate": if score_values.is_empty() { 0.0 } else { round_f64(success_count as f64 / score_values.len() as f64, 4) },
                "error_rate": if score_values.is_empty() { 0.0 } else { round_f64(error_count as f64 / score_values.len() as f64, 4) },
                "fallback_rate": if score_values.is_empty() { 0.0 } else { round_f64(fallback_count as f64 / score_values.len() as f64, 4) },
                "avg_score": round_f64(mean_f64(&score_values), 4),
                "avg_latency_ms": round_f64(mean_i64(&latency_values), 2),
                "p95_latency_ms": percentile_i64(&latency_values, 95),
                "consistency": round_f64(consistency, 4),
            }
        })
    }

    pub(crate) fn aggregate_profile_metrics(&self, case_results: &[Value], repeat_count: i32) -> Value {
        let mut all_attempts = Vec::new();
        for case in case_results {
            if let Some(items) = case.get("attempts").and_then(Value::as_array) {
                all_attempts.extend(items.iter().cloned());
            }
        }
        if all_attempts.is_empty() {
            return json!({
                "generalization_score": 0.0,
                "stability_score": 0.0,
                "final_score": 0.0,
                "success_rate": 0.0,
                "tool_selection_accuracy": 0.0,
                "arg_accuracy": 0.0,
                "avg_latency_ms": 0.0,
                "p95_latency_ms": 0,
                "consistency": 0.0,
                "total_attempts": 0,
                "successful_attempts": 0,
                "error_attempts": 0,
                "error_rate": 0.0,
                "fallback_attempts": 0,
                "fallback_rate": 0.0,
                "repeat_count": repeat_count,
                "latency_target_ms": 8000,
                "has_reasoning": false,
                "total_reasoning_tokens": 0,
            });
        }
        let total_attempts = all_attempts.len() as f64;
        let successful_attempts = all_attempts.iter().filter(|item| value_bool(item, "success")).count() as f64;
        let error_attempts = all_attempts
            .iter()
            .filter(|item| item.get("error_message").map(has_value).unwrap_or(false))
            .count() as f64;
        let fallback_attempts = all_attempts
            .iter()
            .filter(|item| value_bool(item, "fallback_used"))
            .count() as f64;
        let success_rate = successful_attempts / total_attempts;
        let tool_acc = all_attempts
            .iter()
            .map(|item| if value_bool(item, "tool_match") { 1.0 } else { 0.0 })
            .collect::<Vec<_>>();
        let arg_acc = all_attempts
            .iter()
            .map(|item| (0.6 * value_f64(item, "arg_required_score")) + (0.4 * value_f64(item, "arg_value_score")))
            .collect::<Vec<_>>();
        let avg_case_score = mean_f64(
            &all_attempts
                .iter()
                .map(|item| value_f64(item, "case_score"))
                .collect::<Vec<_>>(),
        );
        let latencies = all_attempts
            .iter()
            .map(|item| item.get("latency_ms").and_then(Value::as_i64).unwrap_or(0))
            .collect::<Vec<_>>();
        let case_consistency = case_results
            .iter()
            .map(|item| {
                item.get("summary")
                    .and_then(|summary| summary.get("consistency"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            })
            .collect::<Vec<_>>();
        let consistency = if case_consistency.is_empty() {
            1.0
        } else {
            mean_f64(&case_consistency)
        };
        let has_reasoning = all_attempts
            .iter()
            .any(|item| item.get("reasoning_content").map(has_value).unwrap_or(false));
        let latency_target_ms = if has_reasoning { 30_000.0 } else { 8_000.0 };
        let latency_score = (1.0 - (percentile_i64(&latencies, 95) as f64 / latency_target_ms)).max(0.0);
        let generalization_score = avg_case_score * 100.0;
        let stability_score = (0.4 * success_rate + 0.3 * consistency + 0.3 * latency_score) * 100.0;
        let final_score = (0.55 * generalization_score) + (0.45 * stability_score);
        let total_reasoning_tokens = all_attempts
            .iter()
            .map(|item| {
                item.get("usage")
                    .and_then(|usage| usage.get("reasoning_tokens"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
            })
            .sum::<i64>();
        json!({
            "generalization_score": round_f64(generalization_score, 2),
            "stability_score": round_f64(stability_score, 2),
            "final_score": round_f64(final_score, 2),
            "success_rate": round_f64(success_rate * 100.0, 2),
            "tool_selection_accuracy": round_f64(mean_f64(&tool_acc) * 100.0, 2),
            "arg_accuracy": round_f64(mean_f64(&arg_acc) * 100.0, 2),
            "avg_latency_ms": round_f64(mean_i64(&latencies), 2),
            "p95_latency_ms": percentile_i64(&latencies, 95),
            "consistency": round_f64(consistency * 100.0, 2),
            "total_attempts": total_attempts as i64,
            "successful_attempts": successful_attempts as i64,
            "error_attempts": error_attempts as i64,
            "error_rate": round_f64((error_attempts / total_attempts) * 100.0, 2),
            "fallback_attempts": fallback_attempts as i64,
            "fallback_rate": round_f64((fallback_attempts / total_attempts) * 100.0, 2),
            "repeat_count": repeat_count,
            "latency_target_ms": latency_target_ms as i64,
            "has_reasoning": has_reasoning,
            "total_reasoning_tokens": total_reasoning_tokens,
        })
    }

    pub(crate) fn build_ranking(&self, profiles: &[EvalProfileSnapshot]) -> Vec<Value> {
        let mut ranking = profiles
            .iter()
            .filter_map(|profile| {
                let metrics = profile.metrics.as_ref()?;
                Some(json!({
                    "profile_id": profile.profile_id,
                    "name": profile.name,
                    "model": profile.model,
                    "final_score": metrics.get("final_score").cloned().unwrap_or_else(|| json!(0.0)),
                    "generalization_score": metrics.get("generalization_score").cloned().unwrap_or_else(|| json!(0.0)),
                    "stability_score": metrics.get("stability_score").cloned().unwrap_or_else(|| json!(0.0)),
                    "success_rate": metrics.get("success_rate").cloned().unwrap_or_else(|| json!(0.0)),
                    "error_rate": metrics.get("error_rate").cloned().unwrap_or_else(|| json!(0.0)),
                    "fallback_rate": metrics.get("fallback_rate").cloned().unwrap_or_else(|| json!(0.0)),
                    "p95_latency_ms": metrics.get("p95_latency_ms").cloned().unwrap_or_else(|| json!(0)),
                    "has_reasoning": metrics.get("has_reasoning").cloned().unwrap_or(json!(false)),
                    "total_reasoning_tokens": metrics.get("total_reasoning_tokens").cloned().unwrap_or_else(|| json!(0)),
                }))
            })
            .collect::<Vec<_>>();
        ranking.sort_by(|left, right| {
            right
                .get("final_score")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .partial_cmp(&left.get("final_score").and_then(Value::as_f64).unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (index, row) in ranking.iter_mut().enumerate() {
            if let Some(object) = row.as_object_mut() {
                object.insert("rank".to_string(), json!(index + 1));
            }
        }
        ranking
    }

    pub(crate) fn build_compare_payload(
        &self,
        left: &EvalProfileSnapshot,
        right: &EvalProfileSnapshot,
    ) -> LLMEvalCompareResponse {
        let left_metrics = left.metrics.clone().unwrap_or_else(|| json!({}));
        let right_metrics = right.metrics.clone().unwrap_or_else(|| json!({}));
        let numeric_keys = [
            "final_score",
            "generalization_score",
            "stability_score",
            "success_rate",
            "error_rate",
            "fallback_rate",
            "tool_selection_accuracy",
            "arg_accuracy",
            "avg_latency_ms",
            "p95_latency_ms",
            "consistency",
        ];
        let mut metric_deltas = HashMap::new();
        for key in numeric_keys {
            let right_value = right_metrics.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            let left_value = left_metrics.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            metric_deltas.insert(key.to_string(), json!(round_f64(right_value - left_value, 2)));
        }

        let left_case_map = left
            .cases
            .iter()
            .filter_map(|item| Some((item.get("case_id")?.as_str()?.to_string(), item.clone())))
            .collect::<HashMap<_, _>>();
        let right_case_map = right
            .cases
            .iter()
            .filter_map(|item| Some((item.get("case_id")?.as_str()?.to_string(), item.clone())))
            .collect::<HashMap<_, _>>();
        let mut case_ids = left_case_map
            .keys()
            .chain(right_case_map.keys())
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        case_ids.sort();

        let mut case_deltas = Vec::new();
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        for case_id in case_ids {
            let left_summary = left_case_map
                .get(&case_id)
                .and_then(|item| item.get("summary"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let right_summary = right_case_map
                .get(&case_id)
                .and_then(|item| item.get("summary"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let left_score = left_summary.get("avg_score").and_then(Value::as_f64).unwrap_or(0.0);
            let right_score = right_summary.get("avg_score").and_then(Value::as_f64).unwrap_or(0.0);
            let left_success = left_summary.get("success_rate").and_then(Value::as_f64).unwrap_or(0.0);
            let right_success = right_summary.get("success_rate").and_then(Value::as_f64).unwrap_or(0.0);
            case_deltas.push(json!({
                "case_id": case_id,
                "left_avg_score": round_f64(left_score, 4),
                "right_avg_score": round_f64(right_score, 4),
                "delta_score": round_f64(right_score - left_score, 4),
                "left_success_rate": round_f64(left_success * 100.0, 2),
                "right_success_rate": round_f64(right_success * 100.0, 2),
            }));
            if left_success > 0.0 && right_success <= 0.0 {
                regressions.push(case_id.clone());
            }
            if right_success > 0.0 && left_success <= 0.0 {
                improvements.push(case_id.clone());
            }
        }

        LLMEvalCompareResponse {
            left: json!({
                "profile_id": left.profile_id,
                "name": left.name,
                "model": left.model,
                "metrics": left_metrics,
            }),
            right: json!({
                "profile_id": right.profile_id,
                "name": right.name,
                "model": right.model,
                "metrics": right_metrics,
            }),
            metric_deltas,
            case_deltas,
            regression_cases: regressions,
            improvement_cases: improvements,
        }
    }

    pub(crate) fn build_signature(
        &self,
        case: &EvalCaseDefinition,
        observed_tool: Option<&str>,
        observed_arguments: &Value,
    ) -> String {
        let Some(tool) = observed_tool else {
            return "__no_tool__".to_string();
        };
        let mut keys = case
            .expectations
            .iter()
            .map(|item| item.key.clone())
            .collect::<Vec<_>>();
        if keys.is_empty() {
            keys = observed_arguments
                .as_object()
                .map(|object| object.keys().cloned().collect())
                .unwrap_or_default();
        }
        keys.sort();
        keys.dedup();
        let mut compact = Map::new();
        if let Some(object) = observed_arguments.as_object() {
            for key in keys {
                if let Some(value) = object.get(&key) {
                    compact.insert(key, value.clone());
                }
            }
        }
        let serialized_args = serde_json::to_string(&compact).unwrap_or_else(|_| "{}".to_string());
        format!("{tool}|{serialized_args}")
    }
}
