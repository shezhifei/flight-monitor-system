use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::json;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use crate::services::ai_run_event_payload::sanitize_event_payload_opt;
use crate::services::ai_run_event_types as evt;
use crate::services::ai_runtime_client::AiRuntimeClient;
use crate::services::streaming_finalizer::{
    finalize_missing_terminal, finalize_stream_terminal, parse_terminal_from_buffer,
};
use fms_application::services::ai_context_service::AiContextService;
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_domain::models::ai_job::{AiJobStatus, AiRunStatus};
use fms_runtime::spawn_tracked::spawn_tracked;
use futures_util::stream::StreamExt;

use super::shared::{
    bind_conversation_id, current_user_id, resolve_stream_task_type, target_objects_from_request, NLQueryRequest,
};

pub(crate) async fn query_natural_language_stream(
    req: HttpRequest,
    claims: JwtAuth,
    body: web::Json<NLQueryRequest>,
    job_service: web::Data<Arc<AiJobService>>,
    context_service: web::Data<Arc<AiContextService>>,
    runtime_client: web::Data<Arc<AiRuntimeClient>>,
    proposal_ingest_service: web::Data<Arc<AiProposalIngestService>>,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:chat")?;

    // Task I4: pinned task types (e.g. dispatch_ops for the dispatch board
    // assistant) are validated before any job/run is created.
    let task_type = resolve_stream_task_type(&body)?;

    let user_id = current_user_id(&claims);
    let roles: Vec<String> = claims.0.permissions.clone();

    let job = job_service
        .create_job("nl_query_stream", Some(&user_id), None, None, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let target_objects = target_objects_from_request(&body);
    let mut envelope = context_service
        .build_envelope(
            &user_id,
            &roles,
            claims.0.department_id.as_deref(),
            task_type,
            &body.question,
            &target_objects,
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    bind_conversation_id(&body, &mut envelope);

    let run = job_service
        .create_run(&job.job_id, "python-ai-runtime", None, None)
        .await
        .map_err(super::shared::map_job_error)?;

    let _ = job_service
        .append_event(&job.job_id, &run.run_id, evt::RUNTIME_STARTED, None)
        .await;

    envelope.job_id = job.job_id.clone();
    envelope.run_id = run.run_id.clone();

    let envelope_value = serde_json::to_value(&envelope).map_err(|e| ApiError::Internal(e.to_string()))?;

    job_service
        .update_run_input_envelope(&run.run_id, envelope_value.clone())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    job_service
        .transition_job(&job.job_id, AiJobStatus::Claimed)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    job_service
        .transition_run(&run.run_id, AiRunStatus::Claimed)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    job_service
        .transition_job(&job.job_id, AiJobStatus::Running)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    job_service
        .transition_run(&run.run_id, AiRunStatus::Running)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let python_resp_res = runtime_client.stream_run_raw(&req, &envelope_value).await;

    match python_resp_res {
        Ok(python_resp) => {
            let mut python_stream = python_resp.bytes_stream();
            let job_id = job.job_id.clone();
            let run_id = run.run_id.clone();
            let job_service_for_task = job_service.clone();
            let ingest_svc_for_task = proposal_ingest_service.clone();
            let envelope_for_task = envelope.clone();

            let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<Result<web::Bytes, String>>(128);

            spawn_tracked("nl_query_stream", async move {
                let mut buffer = String::new();
                let mut stream_error_message: Option<String> = None;
                let mut token_emitted = false;
                let mut graph_started_emitted = false;
                let mut graph_fallback_emitted = false;

                let _ = job_service_for_task
                    .append_event(&job_id, &run_id, evt::PROVIDER_STREAM_STARTED, None)
                    .await;

                while let Some(chunk_res) = python_stream.next().await {
                    match chunk_res {
                        Ok(chunk) => {
                            if let Ok(text) = std::str::from_utf8(&chunk) {
                                buffer.push_str(text);

                                if !token_emitted && text.contains("event: token") {
                                    token_emitted = true;
                                    let _ = job_service_for_task
                                        .append_event(&job_id, &run_id, evt::FIRST_TOKEN_EMITTED, None)
                                        .await;
                                }
                                if !graph_started_emitted && text.contains("graph_orchestrate") {
                                    graph_started_emitted = true;
                                    let _ = job_service_for_task
                                        .append_event(&job_id, &run_id, evt::GRAPH_ORCHESTRATION_STARTED, None)
                                        .await;
                                }
                                if !graph_fallback_emitted && text.contains("graph_fallback") {
                                    graph_fallback_emitted = true;
                                    let _ = job_service_for_task
                                        .append_event(&job_id, &run_id, evt::GRAPH_ORCHESTRATION_FALLBACK, None)
                                        .await;
                                }
                                if let Some(abort_msg) =
                                    crate::services::sse_stream_parser::extract_transport_abort_message(&buffer)
                                {
                                    stream_error_message = Some(abort_msg);
                                    let _ = chunk_tx.send(Ok(chunk)).await;
                                    break;
                                }
                            }
                            if chunk_tx.send(Ok(chunk)).await.is_err() {}
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            stream_error_message = Some(msg.clone());
                            let _ = chunk_tx.send(Err(msg)).await;
                            break;
                        }
                    }
                }

                let (terminal, _tokens) = parse_terminal_from_buffer(&buffer);
                let result = match terminal {
                    Some(t) => {
                        finalize_stream_terminal(
                            &job_id,
                            &run_id,
                            t,
                            &envelope_for_task,
                            &job_service_for_task,
                            &ingest_svc_for_task,
                        )
                        .await
                    }
                    None => {
                        if let Some(err_msg) = stream_error_message {
                            let _ = job_service_for_task
                                .append_event(
                                    &job_id,
                                    &run_id,
                                    evt::PROVIDER_STREAM_ABORTED,
                                    sanitize_event_payload_opt(Some(serde_json::json!({"error_message": err_msg}))),
                                )
                                .await;
                            let _ = job_service_for_task
                                .append_event(
                                    &job_id,
                                    &run_id,
                                    evt::FINALIZATION_FAILED_TRANSPORT_ERROR,
                                    sanitize_event_payload_opt(Some(serde_json::json!({"error_message": err_msg}))),
                                )
                                .await;
                            let _ = job_service_for_task
                                .append_event(
                                    &job_id,
                                    &run_id,
                                    evt::RUNTIME_COMPLETED,
                                    sanitize_event_payload_opt(Some(serde_json::json!({"error_message": err_msg}))),
                                )
                                .await;
                            crate::services::streaming_finalizer::finalize_transport_error(
                                &job_id,
                                &run_id,
                                &err_msg,
                                &job_service_for_task,
                            )
                            .await
                        } else {
                            let _ = job_service_for_task
                                .append_event(&job_id, &run_id, evt::FINALIZATION_FAILED_MISSING_TERMINAL, None)
                                .await;
                            let _ = job_service_for_task
                                .append_event(
                                    &job_id,
                                    &run_id,
                                    evt::RUNTIME_COMPLETED,
                                    sanitize_event_payload_opt(Some(
                                        serde_json::json!({"error_message": "missing_terminal"}),
                                    )),
                                )
                                .await;
                            finalize_missing_terminal(&job_id, &run_id, &job_service_for_task).await
                        }
                    }
                };

                if !result.succeeded {
                    let fail_message = result.error_message.as_deref().unwrap_or("stream failed");
                    let fail_frame = format!(
                        "event: run.fail\ndata: {}\n\n",
                        serde_json::json!({
                            "contract_version": "ai-runtime.v1",
                            "run_id": run_id,
                            "status": "failed",
                            "answer": fail_message,
                        })
                    );
                    let _ = chunk_tx.send(Ok(web::Bytes::from(fail_frame))).await;
                }
            });

            let client_stream = async_stream::stream! {
                while let Some(item) = chunk_rx.recv().await {
                    yield item.map_err(|e| actix_web::error::ErrorInternalServerError(e));
                }
            };

            Ok(HttpResponse::Ok()
                .content_type("text/event-stream")
                .insert_header(("Cache-Control", "no-cache"))
                .insert_header(("Connection", "keep-alive"))
                .insert_header(("X-Accel-Buffering", "no"))
                .streaming(client_stream))
        }
        Err(e) => {
            let status = e.status();
            let _ = job_service
                .fail_run(
                    &run.run_id,
                    Some("runtime_error"),
                    Some(&format!("Python AI runtime returned {}", status)),
                    None,
                )
                .await;
            let _ = job_service
                .transition_job(&job.job_id, AiJobStatus::FailedTerminal)
                .await;
            let _ = job_service
                .append_event(
                    &job.job_id,
                    &run.run_id,
                    evt::RUNTIME_COMPLETED,
                    sanitize_event_payload_opt(Some(
                        json!({"error_message": format!("Python AI runtime returned {}", status)}),
                    )),
                )
                .await;
            Ok(e)
        }
    }
}
