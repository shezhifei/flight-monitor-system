use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{debug, error};

use super::case_loader::{
    build_reasoning_block, cancelled_attempt, error_attempt, extract_chat_reasoning_content,
    extract_chat_response_content, extract_chat_tool_response, extract_chat_usage, extract_responses_reasoning_content,
    extract_responses_text, extract_responses_tool_response, extract_responses_usage, parse_json_like_text,
    retryable_status, should_fallback_to_text_mode, strip_nulls, truncate_text, value_bool, value_f64, value_str,
};
use super::service::LLMEvalService;
use super::types::{EvalCaseDefinition, RuntimeProfile};
use crate::schemas::llm_eval_schemas::EvalRunOptionsRequest;

impl LLMEvalService {
    pub(crate) fn build_pooled_reqwest_client() -> reqwest::Client {
        crate::http_client::shared_http_client()
    }

    pub(crate) async fn run_job(&self, job_id: String) {
        if let Some(mut job_entry) = self.state.jobs.get_mut(&job_id) {
            let job = job_entry.value_mut();
            job.status = "running".to_string();
            job.started_at = Some(Utc::now().to_rfc3339());
        } else {
            return;
        }

        let options = {
            match self.state.jobs.get(&job_id) {
                Some(job_entry) => job_entry.value().options.clone(),
                None => return,
            }
        };
        let profiles = self
            .state
            .runtime_profiles
            .get(&job_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default();
        let cases = self
            .state
            .runtime_cases
            .get(&job_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default();

        let mut join_set = JoinSet::new();
        let semaphore = Arc::new(Semaphore::new(options.profile_concurrency.max(1) as usize));
        for profile in profiles {
            let permit = semaphore.clone();
            let service = self.clone();
            let profile_cases = cases.clone();
            let job_id_clone = job_id.clone();
            let options_clone = options.clone();
            join_set.spawn(async move {
                let _permit = permit
                    .acquire_owned()
                    .await
                    .expect("profile semaphore closed unexpectedly");
                service
                    .run_profile(job_id_clone, profile, profile_cases, options_clone)
                    .await;
            });
        }

        while let Some(result) = join_set.join_next().await {
            if let Err(error) = result {
                error!("llm eval profile join failed: {error}");
            }
        }

        let cancelled = self.is_cancelled(&job_id).await;
        let ranking = {
            self.state
                .jobs
                .get(&job_id)
                .map(|job_entry| self.build_ranking(job_entry.value().profiles.as_slice()))
                .unwrap_or_default()
        };
        if let Some(mut job_entry) = self.state.jobs.get_mut(&job_id) {
            let job = job_entry.value_mut();
            job.status = if cancelled {
                "cancelled".to_string()
            } else {
                "completed".to_string()
            };
            job.finished_at = Some(Utc::now().to_rfc3339());
            job.ranking = ranking;
        }

        self.state.runtime_profiles.remove(&job_id);
        self.state.runtime_cases.remove(&job_id);
        self.state.cancel_flags.remove(&job_id);
        self.state.tasks.remove(&job_id);
    }

    pub(crate) async fn run_profile(
        &self,
        job_id: String,
        profile: RuntimeProfile,
        cases: Vec<EvalCaseDefinition>,
        options: EvalRunOptionsRequest,
    ) {
        if self.is_cancelled(&job_id).await {
            self.set_profile_result(
                &job_id,
                &profile.profile_id,
                "cancelled",
                None,
                Vec::new(),
                Some("cancelled".to_string()),
            )
            .await;
            return;
        }
        self.update_profile_status(&job_id, &profile.profile_id, "running")
            .await;
        let repeat_count = options.repeat_count.max(1);
        let case_concurrency = options.case_concurrency.max(1) as usize;
        let enable_tool_routing = options.enable_tool_routing;
        let mut attempts_by_case: HashMap<String, Vec<Value>> = HashMap::new();
        let case_map = cases
            .iter()
            .map(|item| (item.case_id.clone(), item.clone()))
            .collect::<HashMap<_, _>>();

        let semaphore = Arc::new(Semaphore::new(case_concurrency));
        let mut join_set = JoinSet::new();
        for case in &cases {
            for attempt_index in 1..=repeat_count {
                let permit = semaphore.clone();
                let service = self.clone();
                let job_id_clone = job_id.clone();
                let profile_clone = profile.clone();
                let case_clone = case.clone();
                join_set.spawn(async move {
                    let _permit = permit
                        .acquire_owned()
                        .await
                        .expect("case semaphore closed unexpectedly");
                    if service.is_cancelled(&job_id_clone).await {
                        return Some((case_clone.case_id.clone(), cancelled_attempt(attempt_index)));
                    }
                    let mut attempt = if profile_clone.api_mode == "responses" {
                        if enable_tool_routing {
                            service.execute_tool_case_responses(&profile_clone, &case_clone).await
                        } else {
                            service.execute_text_case_responses(&profile_clone, &case_clone).await
                        }
                    } else if enable_tool_routing {
                        service.execute_tool_case(&profile_clone, &case_clone).await
                    } else {
                        service.execute_text_case(&profile_clone, &case_clone).await
                    };
                    if let Some(object) = attempt.as_object_mut() {
                        object.insert("attempt_index".to_string(), json!(attempt_index));
                    }
                    Some((case_clone.case_id.clone(), attempt))
                });
            }
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Some((case_id, attempt))) => {
                    attempts_by_case.entry(case_id).or_default().push(attempt);
                    self.increment_progress(&job_id, &profile.profile_id).await;
                }
                Ok(None) => {}
                Err(error) => {
                    debug!("llm eval attempt join aborted: {error}");
                }
            }
        }

        if self.is_cancelled(&job_id).await {
            self.set_profile_result(
                &job_id,
                &profile.profile_id,
                "cancelled",
                None,
                Vec::new(),
                Some("cancelled".to_string()),
            )
            .await;
            return;
        }

        let mut case_results = Vec::new();
        for (case_id, attempts) in attempts_by_case {
            if let Some(definition) = case_map.get(&case_id) {
                case_results.push(self.aggregate_case_results(definition, attempts));
            }
        }
        case_results.sort_by(|left, right| value_str(left, "case_id").cmp(value_str(right, "case_id")));
        let metrics = self.aggregate_profile_metrics(&case_results, repeat_count);
        self.set_profile_result(
            &job_id,
            &profile.profile_id,
            "completed",
            Some(metrics),
            case_results,
            None,
        )
        .await;
    }

    pub(crate) async fn execute_tool_case(&self, profile: &RuntimeProfile, case: &EvalCaseDefinition) -> Value {
        let started = std::time::Instant::now();
        let payload = json!({
            "model": profile.model,
            "messages": [
                {
                    "role": "system",
                    "content": "你是测试模式下的工具路由器。必须从提供的 tools 中选择最合适的一个并给出参数。不要直接回答业务结论。"
                },
                {
                    "role": "user",
                    "content": case.prompt
                }
            ],
            "stream": false,
            "tools": self.chat_tools(case),
            "tool_choice": "auto",
            "reasoning_effort": profile.reasoning_effort,
            "max_completion_tokens": profile.max_completion_tokens,
        });

        match self.post_json(profile, "/chat/completions", payload).await {
            Ok(response) => {
                let (observed_tool, observed_arguments, assistant_content) = extract_chat_tool_response(&response);
                let evaluation = self.evaluate_case(case, observed_tool.as_deref(), &observed_arguments);
                json!({
                    "success": value_bool(&evaluation, "success"),
                    "case_score": value_f64(&evaluation, "case_score"),
                    "tool_match": value_bool(&evaluation, "tool_match"),
                    "arg_required_score": value_f64(&evaluation, "arg_required_score"),
                    "arg_value_score": value_f64(&evaluation, "arg_value_score"),
                    "observed_tool": observed_tool,
                    "observed_arguments": observed_arguments,
                    "signature": self.build_signature(case, observed_tool.as_deref(), &observed_arguments),
                    "latency_ms": started.elapsed().as_millis() as i64,
                    "error_message": Value::Null,
                    "fallback_used": false,
                    "fallback_reason": Value::Null,
                    "usage": extract_chat_usage(&response),
                    "assistant_excerpt": truncate_text(&assistant_content, 400),
                    "reasoning_content": extract_chat_reasoning_content(&response),
                })
            }
            Err(error_text) if should_fallback_to_text_mode(&error_text) => {
                let mut fallback = self.execute_text_case(profile, case).await;
                if let Some(object) = fallback.as_object_mut() {
                    object.insert("fallback_used".to_string(), Value::Bool(true));
                    object.insert("fallback_reason".to_string(), Value::String(error_text));
                }
                fallback
            }
            Err(error_text) => error_attempt(started.elapsed().as_millis() as i64, error_text),
        }
    }

    pub(crate) async fn execute_text_case(&self, profile: &RuntimeProfile, case: &EvalCaseDefinition) -> Value {
        let started = std::time::Instant::now();
        let tool_names = self.tool_names_for_case(case).join("\n- ");
        let system_prompt = format!(
            "你是工具路由评测器。根据用户问题输出 JSON，格式必须为 {{\"tool\":\"<tool_name>\",\"arguments\":{{...}}}}。不得输出其他文本。可选工具:\n- {tool_names}"
        );
        let payload = json!({
            "model": profile.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": case.prompt }
            ],
            "stream": false,
            "reasoning_effort": profile.reasoning_effort,
            "max_completion_tokens": profile.max_completion_tokens,
        });

        match self.post_json(profile, "/chat/completions", payload).await {
            Ok(response) => {
                let content = extract_chat_response_content(&response);
                let parsed = parse_json_like_text(&content);
                let observed_tool = parsed
                    .get("tool")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let observed_arguments = parsed
                    .get("arguments")
                    .and_then(Value::as_object)
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or_else(|| json!({}));
                let evaluation = self.evaluate_case(case, observed_tool.as_deref(), &observed_arguments);
                json!({
                    "success": value_bool(&evaluation, "success"),
                    "case_score": value_f64(&evaluation, "case_score"),
                    "tool_match": value_bool(&evaluation, "tool_match"),
                    "arg_required_score": value_f64(&evaluation, "arg_required_score"),
                    "arg_value_score": value_f64(&evaluation, "arg_value_score"),
                    "observed_tool": observed_tool,
                    "observed_arguments": observed_arguments,
                    "signature": self.build_signature(case, observed_tool.as_deref(), &observed_arguments),
                    "latency_ms": started.elapsed().as_millis() as i64,
                    "error_message": Value::Null,
                    "fallback_used": false,
                    "fallback_reason": Value::Null,
                    "usage": extract_chat_usage(&response),
                    "assistant_excerpt": truncate_text(&content, 400),
                    "reasoning_content": extract_chat_reasoning_content(&response),
                })
            }
            Err(error_text) => error_attempt(started.elapsed().as_millis() as i64, error_text),
        }
    }

    pub(crate) async fn execute_tool_case_responses(
        &self,
        profile: &RuntimeProfile,
        case: &EvalCaseDefinition,
    ) -> Value {
        let started = std::time::Instant::now();
        let payload = json!({
            "model": profile.model,
            "instructions": profile.instructions.clone().unwrap_or_else(|| "你是测试模式下的工具路由器。必须从提供的 tools 中选择最合适的一个并给出参数。不要直接回答业务结论。".to_string()),
            "input": case.prompt,
            "tools": self.responses_tools(case),
            "tool_choice": "auto",
            "store": profile.store,
            "include": profile.include,
            "reasoning": build_reasoning_block(profile),
            "max_output_tokens": profile.max_completion_tokens,
        });

        match self.post_json(profile, "/responses", payload).await {
            Ok(response) => {
                let (observed_tool, observed_arguments, assistant_content) = extract_responses_tool_response(&response);
                let evaluation = self.evaluate_case(case, observed_tool.as_deref(), &observed_arguments);
                json!({
                    "success": value_bool(&evaluation, "success"),
                    "case_score": value_f64(&evaluation, "case_score"),
                    "tool_match": value_bool(&evaluation, "tool_match"),
                    "arg_required_score": value_f64(&evaluation, "arg_required_score"),
                    "arg_value_score": value_f64(&evaluation, "arg_value_score"),
                    "observed_tool": observed_tool,
                    "observed_arguments": observed_arguments,
                    "signature": self.build_signature(case, observed_tool.as_deref(), &observed_arguments),
                    "latency_ms": started.elapsed().as_millis() as i64,
                    "error_message": Value::Null,
                    "fallback_used": false,
                    "fallback_reason": Value::Null,
                    "usage": extract_responses_usage(&response),
                    "assistant_excerpt": truncate_text(&assistant_content, 400),
                    "reasoning_content": extract_responses_reasoning_content(&response),
                })
            }
            Err(error_text) if should_fallback_to_text_mode(&error_text) => {
                let mut fallback = self.execute_text_case_responses(profile, case).await;
                if let Some(object) = fallback.as_object_mut() {
                    object.insert("fallback_used".to_string(), Value::Bool(true));
                    object.insert("fallback_reason".to_string(), Value::String(error_text));
                }
                fallback
            }
            Err(error_text) => error_attempt(started.elapsed().as_millis() as i64, error_text),
        }
    }

    pub(crate) async fn execute_text_case_responses(
        &self,
        profile: &RuntimeProfile,
        case: &EvalCaseDefinition,
    ) -> Value {
        let started = std::time::Instant::now();
        let tool_names = self.tool_names_for_case(case).join("\n- ");
        let payload = json!({
            "model": profile.model,
            "instructions": profile.instructions.clone().unwrap_or_else(|| format!("你是工具路由评测器。根据用户问题输出 JSON，格式必须为 {{\"tool\":\"<tool_name>\",\"arguments\":{{...}}}}。不得输出其他文本。可选工具:\n- {tool_names}")),
            "input": case.prompt,
            "store": profile.store,
            "include": profile.include,
            "reasoning": build_reasoning_block(profile),
            "max_output_tokens": profile.max_completion_tokens,
        });

        match self.post_json(profile, "/responses", payload).await {
            Ok(response) => {
                let content = extract_responses_text(&response);
                let parsed = parse_json_like_text(&content);
                let observed_tool = parsed
                    .get("tool")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let observed_arguments = parsed
                    .get("arguments")
                    .and_then(Value::as_object)
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or_else(|| json!({}));
                let evaluation = self.evaluate_case(case, observed_tool.as_deref(), &observed_arguments);
                json!({
                    "success": value_bool(&evaluation, "success"),
                    "case_score": value_f64(&evaluation, "case_score"),
                    "tool_match": value_bool(&evaluation, "tool_match"),
                    "arg_required_score": value_f64(&evaluation, "arg_required_score"),
                    "arg_value_score": value_f64(&evaluation, "arg_value_score"),
                    "observed_tool": observed_tool,
                    "observed_arguments": observed_arguments,
                    "signature": self.build_signature(case, observed_tool.as_deref(), &observed_arguments),
                    "latency_ms": started.elapsed().as_millis() as i64,
                    "error_message": Value::Null,
                    "fallback_used": false,
                    "fallback_reason": Value::Null,
                    "usage": extract_responses_usage(&response),
                    "assistant_excerpt": truncate_text(&content, 400),
                    "reasoning_content": extract_responses_reasoning_content(&response),
                })
            }
            Err(error_text) => error_attempt(started.elapsed().as_millis() as i64, error_text),
        }
    }

    pub(crate) async fn post_json(
        &self,
        profile: &RuntimeProfile,
        endpoint: &str,
        mut payload: Value,
    ) -> Result<Value, String> {
        strip_nulls(&mut payload);
        let url = format!("{}{}", profile.base_url.trim_end_matches('/'), endpoint);
        for attempt in 0..=profile.max_retries.max(0) {
            let response = self
                .http
                .post(&url)
                .bearer_auth(&profile.api_key)
                .timeout(StdDuration::from_secs_f64(profile.timeout.max(1.0)))
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    if status.is_success() {
                        return serde_json::from_str(&text)
                            .map_err(|error| format!("invalid JSON response: {error}; body={text}"));
                    }
                    let message = format!("HTTP {}: {}", status.as_u16(), truncate_text(&text, 600));
                    if attempt < profile.max_retries && retryable_status(status) {
                        sleep(StdDuration::from_secs_f64(profile.retry_delay.max(0.0))).await;
                        continue;
                    }
                    return Err(message);
                }
                Err(error) => {
                    if attempt < profile.max_retries {
                        sleep(StdDuration::from_secs_f64(profile.retry_delay.max(0.0))).await;
                        continue;
                    }
                    return Err(error.to_string());
                }
            }
        }
        Err("request failed".to_string())
    }
}
