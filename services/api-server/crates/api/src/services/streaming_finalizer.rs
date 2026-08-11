use std::sync::Arc;
use std::time::Instant;

use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_domain::models::ai_context_envelope::ContextEnvelope;
use fms_domain::models::ai_job::AiJobStatus;
use fms_domain::models::ai_structured_output::AiStructuredOutput;

use crate::services::ai_run_event_payload::sanitize_event_payload_opt;
use crate::services::ai_run_event_types::*;
use crate::services::sse_stream_parser::{extract_terminal, is_degraded_success, parse_sse_stream, StreamTerminal};

/// Log an error if a final-state persistence operation fails.
/// Terminal state bookkeeping must never silently fail — at minimum we log
/// the error for alerting/reconciliation.
macro_rules! log_if_err {
    ($expr:expr, $ctx:expr) => {
        if let Err(e) = $expr {
            tracing::error!("streaming_finalizer_persistence_failed context={} error={}", $ctx, e);
        }
    };
}

#[derive(Debug, Clone)]
pub struct FinalizeResult {
    pub terminal_detected: bool,
    pub terminal_type: String,
    pub succeeded: bool,
    pub degraded: bool,
    pub proposal_count: usize,
    pub rejected_count: usize,
    pub error_category: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: u64,
}

impl FinalizeResult {
    pub fn no_terminal(duration_ms: u64) -> Self {
        Self {
            terminal_detected: false,
            terminal_type: "none".to_string(),
            succeeded: false,
            degraded: false,
            proposal_count: 0,
            rejected_count: 0,
            error_category: Some("transport_error".to_string()),
            error_message: Some("No terminal event received from SSE stream".to_string()),
            duration_ms,
        }
    }

    pub fn transport_error(duration_ms: u64, msg: String) -> Self {
        Self {
            terminal_detected: false,
            terminal_type: "transport_error".to_string(),
            succeeded: false,
            degraded: false,
            proposal_count: 0,
            rejected_count: 0,
            error_category: Some("transport_error".to_string()),
            error_message: Some(msg),
            duration_ms,
        }
    }
}

/// Parse a complete SSE buffer and extract the terminal event.
/// This is a pure, synchronous function suitable for unit testing.
pub fn parse_terminal_from_buffer(buffer: &str) -> (Option<StreamTerminal>, Vec<String>) {
    let events = parse_sse_stream(buffer);
    let terminal = extract_terminal(&events);
    let token_deltas: Vec<String> = events
        .iter()
        .filter(|e| matches!(e.event_type, crate::services::sse_stream_parser::SseEventType::Token))
        .map(|e| e.data.clone())
        .collect();
    (terminal, token_deltas)
}

/// Execute the terminal finalization logic: proposal ingest, complete_run/fail_run, job transition.
/// This async function is decoupled from the HTTP streaming layer and can be tested independently.
pub async fn finalize_stream_terminal(
    job_id: &str,
    run_id: &str,
    terminal: StreamTerminal,
    envelope: &ContextEnvelope,
    job_service: &Arc<AiJobService>,
    ingest_service: &Arc<AiProposalIngestService>,
) -> FinalizeResult {
    let start = Instant::now();
    let degraded = is_degraded_success(&terminal);
    let raw_output = match serde_json::to_value(terminal.clone().into_output()) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::error!("streaming_finalizer_serialize_terminal_failed error={}", e);
            None
        }
    };

    match terminal {
        StreamTerminal::Succeeded(output) => {
            let proposal_count = output
                .get("proposals")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let has_proposals = proposal_count > 0;

            log_if_err!(
                job_service
                    .append_event(job_id, run_id, PROVIDER_STREAM_COMPLETED, None)
                    .await,
                "append_event:PROVIDER_STREAM_COMPLETED"
            );

            if has_proposals {
                log_if_err!(
                    job_service
                        .append_event(job_id, run_id, PROPOSAL_INGEST_STARTED, None)
                        .await,
                    "append_event:PROPOSAL_INGEST_STARTED"
                );
                let structured_output = match serde_json::from_value::<AiStructuredOutput>(output.clone()) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_msg = format!(
                            "proposal_validation_failed: terminal output deserialization failed: {}",
                            e
                        );
                        log_if_err!(
                            job_service
                                .append_event(
                                    job_id,
                                    run_id,
                                    PROPOSAL_INGEST_FAILED,
                                    sanitize_event_payload_opt(Some(serde_json::json!({"error_message": &err_msg}))),
                                )
                                .await,
                            "append_event:PROPOSAL_INGEST_FAILED"
                        );
                        log_if_err!(
                            job_service
                                .fail_run(run_id, Some("proposal_validation_failed"), Some(&err_msg), raw_output)
                                .await,
                            "fail_run:proposal_validation_failed"
                        );
                        log_if_err!(
                            job_service.transition_job(job_id, AiJobStatus::FailedTerminal).await,
                            "transition_job:FailedTerminal(validation)"
                        );
                        let duration_ms = start.elapsed().as_millis() as u64;
                        log_if_err!(
                            job_service
                                .append_event(
                                    job_id,
                                    run_id,
                                    RUNTIME_COMPLETED,
                                    sanitize_event_payload_opt(Some(
                                        serde_json::json!({"duration_ms": duration_ms, "error_message": err_msg}),
                                    )),
                                )
                                .await,
                            "append_event:RUNTIME_COMPLETED(validation_failed)"
                        );
                        return FinalizeResult {
                            terminal_detected: true,
                            terminal_type: "run.complete_failed_ingest".to_string(),
                            succeeded: false,
                            degraded,
                            proposal_count,
                            rejected_count: 0,
                            error_category: Some("proposal_validation_failed".to_string()),
                            error_message: Some(err_msg),
                            duration_ms,
                        };
                    }
                };
                let ingest_result = ingest_service.ingest(structured_output, envelope).await;
                let rejected_count = ingest_result.rejected_proposals.len();

                if !ingest_result.success {
                    let first_error = ingest_result
                        .rejected_proposals
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("unknown");
                    let err_msg = format!(
                        "proposal_validation_failed: {} rejected; first: {}",
                        rejected_count, first_error
                    );
                    log_if_err!(
                        job_service
                            .append_event(
                                job_id,
                                run_id,
                                PROPOSAL_INGEST_FAILED,
                                sanitize_event_payload_opt(Some(serde_json::json!({"error_message": err_msg}))),
                            )
                            .await,
                        "append_event:PROPOSAL_INGEST_FAILED(ingest_rejected)"
                    );
                    log_if_err!(
                        job_service
                            .fail_run(run_id, Some("proposal_validation_failed"), Some(&err_msg), raw_output)
                            .await,
                        "fail_run:proposal_validation_failed(ingest_rejected)"
                    );
                    log_if_err!(
                        job_service.transition_job(job_id, AiJobStatus::FailedTerminal).await,
                        "transition_job:FailedTerminal(ingest_rejected)"
                    );
                    let duration_ms = start.elapsed().as_millis() as u64;
                    log_if_err!(
                        job_service
                            .append_event(
                                job_id,
                                run_id,
                                RUNTIME_COMPLETED,
                                sanitize_event_payload_opt(Some(
                                    serde_json::json!({"duration_ms": duration_ms, "error_message": err_msg}),
                                )),
                            )
                            .await,
                        "append_event:RUNTIME_COMPLETED(ingest_rejected)"
                    );
                    return FinalizeResult {
                        terminal_detected: true,
                        terminal_type: "run.complete_failed_ingest".to_string(),
                        succeeded: false,
                        degraded,
                        proposal_count,
                        rejected_count,
                        error_category: Some("proposal_validation_failed".to_string()),
                        error_message: Some(err_msg),
                        duration_ms,
                    };
                }

                log_if_err!(
                    job_service
                        .append_event(job_id, run_id, PROPOSAL_INGEST_SUCCEEDED, None)
                        .await,
                    "append_event:PROPOSAL_INGEST_SUCCEEDED"
                );
                log_if_err!(
                    job_service.complete_run(run_id, raw_output, None, None).await,
                    "complete_run:succeeded(with_proposals)"
                );
                log_if_err!(
                    job_service.transition_job(job_id, AiJobStatus::Succeeded).await,
                    "transition_job:Succeeded(with_proposals)"
                );
                let duration_ms = start.elapsed().as_millis() as u64;
                log_if_err!(
                    job_service
                        .append_event(
                            job_id,
                            run_id,
                            RUNTIME_COMPLETED,
                            sanitize_event_payload_opt(Some(serde_json::json!({"duration_ms": duration_ms}))),
                        )
                        .await,
                    "append_event:RUNTIME_COMPLETED(succeeded_with_proposals)"
                );
                FinalizeResult {
                    terminal_detected: true,
                    terminal_type: "run.complete_succeeded".to_string(),
                    succeeded: true,
                    degraded,
                    proposal_count,
                    rejected_count,
                    error_category: None,
                    error_message: None,
                    duration_ms,
                }
            } else {
                log_if_err!(
                    job_service.complete_run(run_id, raw_output, None, None).await,
                    "complete_run:succeeded(no_proposals)"
                );
                log_if_err!(
                    job_service.transition_job(job_id, AiJobStatus::Succeeded).await,
                    "transition_job:Succeeded(no_proposals)"
                );
                let duration_ms = start.elapsed().as_millis() as u64;
                log_if_err!(
                    job_service
                        .append_event(
                            job_id,
                            run_id,
                            RUNTIME_COMPLETED,
                            sanitize_event_payload_opt(Some(serde_json::json!({"duration_ms": duration_ms}))),
                        )
                        .await,
                    "append_event:RUNTIME_COMPLETED(succeeded_no_proposals)"
                );
                FinalizeResult {
                    terminal_detected: true,
                    terminal_type: "run.complete_succeeded".to_string(),
                    succeeded: true,
                    degraded,
                    proposal_count: 0,
                    rejected_count: 0,
                    error_category: None,
                    error_message: None,
                    duration_ms,
                }
            }
        }
        StreamTerminal::Failed(output) => {
            log_if_err!(
                job_service
                    .append_event(job_id, run_id, PROVIDER_STREAM_COMPLETED, None)
                    .await,
                "append_event:PROVIDER_STREAM_COMPLETED(failed)"
            );
            log_if_err!(
                job_service
                    .fail_run(
                        run_id,
                        Some("business_failure"),
                        Some("Python AI runtime returned failure event"),
                        raw_output,
                    )
                    .await,
                "fail_run:business_failure"
            );
            log_if_err!(
                job_service.transition_job(job_id, AiJobStatus::FailedTerminal).await,
                "transition_job:FailedTerminal(business_failure)"
            );
            let duration_ms = start.elapsed().as_millis() as u64;
            log_if_err!(
                job_service
                    .append_event(
                        job_id,
                        run_id,
                        RUNTIME_COMPLETED,
                        sanitize_event_payload_opt(Some(serde_json::json!({"duration_ms": duration_ms}))),
                    )
                    .await,
                "append_event:RUNTIME_COMPLETED(business_failure)"
            );
            FinalizeResult {
                terminal_detected: true,
                terminal_type: "run.fail".to_string(),
                succeeded: false,
                degraded: false,
                proposal_count: output
                    .get("proposals")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0),
                rejected_count: 0,
                error_category: Some("business_failure".to_string()),
                error_message: Some("Python AI runtime returned failure event".to_string()),
                duration_ms,
            }
        }
    }
}

/// Handle the case where no terminal event was found in the stream.
pub async fn finalize_missing_terminal(job_id: &str, run_id: &str, job_service: &Arc<AiJobService>) -> FinalizeResult {
    let start = Instant::now();
    log_if_err!(
        job_service
            .fail_run(
                run_id,
                Some("transport_error"),
                Some("No terminal event received from SSE stream"),
                None,
            )
            .await,
        "fail_run:transport_error(no_terminal)"
    );
    log_if_err!(
        job_service.transition_job(job_id, AiJobStatus::FailedTerminal).await,
        "transition_job:FailedTerminal(no_terminal)"
    );
    FinalizeResult::no_terminal(start.elapsed().as_millis() as u64)
}

/// Handle a transport error that occurred while reading the stream.
pub async fn finalize_transport_error(
    job_id: &str,
    run_id: &str,
    error_message: &str,
    job_service: &Arc<AiJobService>,
) -> FinalizeResult {
    let start = Instant::now();
    log_if_err!(
        job_service
            .fail_run(run_id, Some("transport_error"), Some(error_message), None)
            .await,
        "fail_run:transport_error"
    );
    log_if_err!(
        job_service.transition_job(job_id, AiJobStatus::FailedTerminal).await,
        "transition_job:FailedTerminal(transport_error)"
    );
    FinalizeResult::transport_error(start.elapsed().as_millis() as u64, error_message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_terminal_from_buffer_succeeded() {
        let buffer = format!(
            "event: progress\ndata: {{\"step\":\"init\"}}\n\n\
             event: token\ndata: {{\"delta\":\"hello\"}}\n\n\
             event: run.complete\ndata: {}\n\n",
            json!({
                "contract_version": "1.0",
                "run_id": "run_test",
                "status": "succeeded",
                "answer": "hello",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": [],
                "metrics": null
            })
        );

        let (terminal, tokens) = parse_terminal_from_buffer(&buffer);
        assert!(terminal.is_some());
        assert!(terminal.unwrap().is_success());
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    fn test_parse_terminal_from_buffer_failed() {
        let buffer = format!(
            "event: run.fail\ndata: {}\n\n",
            json!({
                "contract_version": "1.0",
                "run_id": "run_test",
                "status": "failed",
                "answer": "error",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": [],
                "metrics": null
            })
        );

        let (terminal, tokens) = parse_terminal_from_buffer(&buffer);
        assert!(terminal.is_some());
        assert!(!terminal.unwrap().is_success());
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_parse_terminal_from_buffer_no_terminal() {
        let buffer = "event: progress\ndata: {\"step\":\"init\"}\n\n";
        let (terminal, tokens) = parse_terminal_from_buffer(buffer);
        assert!(terminal.is_none());
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_parse_terminal_duplicate_last_wins() {
        let buffer = format!(
            "event: run.complete\ndata: {}\n\n\
             event: run.complete\ndata: {}\n\n",
            json!({
                "contract_version": "1.0",
                "run_id": "run_test",
                "status": "succeeded",
                "answer": "first",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": [],
                "metrics": null
            }),
            json!({
                "contract_version": "1.0",
                "run_id": "run_test",
                "status": "failed",
                "answer": "second",
                "reasoning_steps": [],
                "evidence": [],
                "proposals": [],
                "limitations": [],
                "metrics": null
            })
        );

        let (terminal, _) = parse_terminal_from_buffer(&buffer);
        let output = terminal.expect("terminal").into_output();
        let status = output.get("status").and_then(|s| s.as_str()).unwrap_or("");
        assert_eq!(status, "failed", "last terminal event should win");
    }

    // W0-1: Malformed / version-drift sidecar payload must never abort the process.
    // The terminal is extracted as a raw serde_json::Value (lenient), but strict
    // AiStructuredOutput deserialization in finalize_stream_terminal must turn missing
    // fields into a proposal_validation_failed run failure, not a panic.

    #[test]
    fn test_parse_terminal_missing_required_fields_does_not_panic() {
        // Version drift: only a subset of the contract fields are present.
        // parse_terminal_from_buffer must still extract the terminal (raw Value),
        // not panic. The strict deserialization happens later in finalize_stream_terminal.
        let buffer = "event: run.complete\ndata: {\"status\":\"succeeded\"}\n\n";
        let (terminal, tokens) = parse_terminal_from_buffer(buffer);
        assert!(
            terminal.is_some(),
            "terminal should be extracted even with missing fields"
        );
        assert!(terminal.unwrap().is_success());
        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn test_ai_structured_output_rejects_missing_required_fields() {
        // Contract drift: missing run_id/status/answer/proposals etc. must fail
        // AiStructuredOutput deserialization (not panic). In finalize_stream_terminal
        // this Err is mapped to a proposal_validation_failed run failure.
        let incomplete = json!({
            "contract_version": "1.0"
            // missing: run_id, status, answer, reasoning_steps, evidence, proposals, limitations
        });
        let result: Result<AiStructuredOutput, _> = serde_json::from_value(incomplete);
        assert!(
            result.is_err(),
            "missing required fields must fail deserialization, not panic"
        );
    }

    #[test]
    fn test_ai_structured_output_rejects_wrong_type() {
        // Type drift: proposals as string instead of array must fail deserialization.
        let wrong_type = json!({
            "contract_version": "1.0",
            "run_id": "run_test",
            "status": "succeeded",
            "answer": "hello",
            "reasoning_steps": [],
            "evidence": [],
            "proposals": "not_an_array",
            "limitations": [],
            "metrics": null
        });
        let result: Result<AiStructuredOutput, _> = serde_json::from_value(wrong_type);
        assert!(
            result.is_err(),
            "type-mismatched fields must fail deserialization, not panic"
        );
    }
}
