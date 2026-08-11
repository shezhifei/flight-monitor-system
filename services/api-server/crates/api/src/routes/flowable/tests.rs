use super::{
    decode_base64_url_segment, detail_response, extract_claim_from_authorization, flowable_client_unavailable,
    flowable_draft_service_unavailable, flowable_health_error_response, flowable_service_unavailable,
    flowable_stream_event_to_sse, map_draft_error, map_service_error, missing_process_instance_response,
    resolve_requested_tenant, COMMON_TENANT,
};
use actix_web::{body::to_bytes, http::StatusCode, test::TestRequest, HttpRequest};
use chrono::Utc;
use fms_application::schemas::auth_schemas::TokenData;
use fms_application::services::flowable_draft_service::FlowableDraftAssistantStreamEvent;
use fms_application::services::flowable_draft_service::FlowableDraftServiceError;
use fms_application::services::flowable_service::FlowableServiceError;
use serde_json::json;
use tokio::sync::mpsc;

fn encode_base64_url(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut output = String::new();
    let mut index = 0;
    while index < data.len() {
        let b0 = data[index];
        let b1 = data.get(index + 1).copied();
        let b2 = data.get(index + 2).copied();

        output.push(ALPHABET[(b0 >> 2) as usize] as char);
        output.push(ALPHABET[((b0 & 0x03) << 4 | b1.unwrap_or(0) >> 4) as usize] as char);

        if let Some(b1) = b1 {
            output.push(ALPHABET[((b1 & 0x0f) << 2 | b2.unwrap_or(0) >> 6) as usize] as char);
        }
        if let Some(b2) = b2 {
            output.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        }

        index += 3;
    }

    output
}

fn request_with_claims_payload(payload: serde_json::Value) -> HttpRequest {
    let header_segment = encode_base64_url(br#"{"alg":"none","typ":"JWT"}"#);
    let payload_segment = encode_base64_url(&serde_json::to_vec(&payload).expect("serialize payload"));
    TestRequest::default()
        .insert_header((
            "Authorization",
            format!("Bearer {header_segment}.{payload_segment}.signature"),
        ))
        .to_http_request()
}

fn test_claims(department: Option<&str>) -> crate::middleware::jwt::JwtAuth {
    crate::middleware::jwt::JwtAuth(TokenData {
        sub: Some("user-1".to_string()),
        email: None,
        username: Some("tester".to_string()),
        token_kind: Some("access".to_string()),
        is_admin: Some(false),
        permissions: vec![],
        department: department.map(ToOwned::to_owned),
        department_id: department.map(ToOwned::to_owned),
        pv: Some(1),
        iat: Some(Utc::now().timestamp()),
        exp: Some((Utc::now() + chrono::Duration::hours(1)).timestamp()),
        iss: None,
        aud: None,
        ua_hash: None,
        ip_subnet_hash: None,
    })
}

#[actix_web::test]
async fn flowable_detail_response_uses_python_error_envelope() {
    let response = detail_response(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST", "bad draft request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"]["code"], "INVALID_REQUEST");
    assert_eq!(payload["detail"]["message"], "bad draft request");
    println!("flowable detail payload: {}", payload);
}

#[actix_web::test]
async fn flowable_draft_error_maps_to_python_error_envelope() {
    let response = map_draft_error(FlowableDraftServiceError::InvalidRequest("draft invalid".to_string()));
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"]["code"], "INVALID_REQUEST");
    assert_eq!(payload["detail"]["message"], "draft invalid");
    println!("flowable draft error payload: {}", payload);
}

#[actix_web::test]
async fn flowable_process_document_error_maps_to_python_detail_contract() {
    let response = map_draft_error(FlowableDraftServiceError::ProcessDocument {
        status_code: 422,
        code: "DOCUMENT_PARSE_ERROR".to_string(),
        message: "上传文件内容为空或无法解析".to_string(),
    });
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"]["code"], "DOCUMENT_PARSE_ERROR");
    assert_eq!(payload["detail"]["message"], "上传文件内容为空或无法解析");
}

#[actix_web::test]
async fn flowable_ai_unavailable_maps_to_python_detail_contract() {
    let response = map_draft_error(FlowableDraftServiceError::AIUnavailable(
        "AI service unavailable".to_string(),
    ));
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"]["code"], "AI_UNAVAILABLE");
    assert_eq!(payload["detail"]["message"], "AI service unavailable");
}

#[actix_web::test]
async fn flowable_bpmn_validation_maps_to_python_detail_contract() {
    let response = map_draft_error(FlowableDraftServiceError::BpmnDraftValidation {
        code: "BPMN_DRAFT_INVALID".to_string(),
        message: "生成的 BPMN 草案缺少流程定义".to_string(),
    });
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"]["code"], "BPMN_DRAFT_INVALID");
    assert_eq!(payload["detail"]["message"], "生成的 BPMN 草案缺少流程定义");
}

#[test]
fn flowable_upstream_errors_map_to_internal_server_error_semantics() {
    let error = map_service_error(FlowableServiceError::Upstream("flowable down".to_string()));
    assert!(matches!(error, crate::error::ApiError::Internal(_)));
}

#[test]
fn flowable_base64_url_decoder_supports_jwt_payloads() {
    let decoded = decode_base64_url_segment("eyJkZXBhcnRtZW50X2lkIjoiZGVwLTEifQ").expect("decode payload");
    let payload: serde_json::Value = serde_json::from_slice(&decoded).expect("json payload");
    assert_eq!(payload["department_id"], "dep-1");
}

#[test]
fn flowable_extracts_department_id_from_authorization_payload() {
    let request = request_with_claims_payload(json!({
        "department_id": "dep-001",
        "department": "ops",
    }));

    assert_eq!(
        extract_claim_from_authorization(&request, "department_id").as_deref(),
        Some("dep-001")
    );
}

#[test]
fn flowable_resolve_requested_tenant_defaults_to_department_scope() {
    let request = request_with_claims_payload(json!({
        "department_id": "dep-001",
        "department": "ops",
    }));
    let claims = test_claims(Some("ops"));

    let tenant = resolve_requested_tenant(&request, &claims, None).expect("resolve tenant");
    assert_eq!(tenant, "dep-001");
}

#[test]
fn flowable_resolve_requested_tenant_allows_common_without_department() {
    let request = TestRequest::default().to_http_request();
    let claims = test_claims(None);

    let tenant = resolve_requested_tenant(&request, &claims, Some(COMMON_TENANT)).expect("common");
    assert_eq!(tenant, COMMON_TENANT);
}

#[test]
fn flowable_resolve_requested_tenant_rejects_cross_department_access() {
    let request = request_with_claims_payload(json!({
        "department_id": "dep-001",
        "department": "ops",
    }));
    let claims = test_claims(Some("ops"));

    let error = resolve_requested_tenant(&request, &claims, Some("dep-002")).expect_err("should reject foreign tenant");
    assert!(matches!(error, crate::error::ApiError::Forbidden(_)));
}

#[actix_web::test]
async fn missing_process_instance_response_matches_python_502_semantics() {
    let response = missing_process_instance_response("启动流程实例失败: Flowable 未返回流程实例ID");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "启动流程实例失败: Flowable 未返回流程实例ID");
    println!("flowable 502 payload: {}", payload);
}

#[actix_web::test]
async fn flowable_health_failure_uses_python_fixed_error_message() {
    let response = flowable_health_error_response();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["success"], false);
    assert_eq!(payload["data"]["status"], "error");
    assert_eq!(payload["data"]["message"], "Flowable REST API 调用失败");
    println!("flowable health payload: {}", payload);
}

#[actix_web::test]
async fn flowable_client_unavailable_matches_python_detail_contract() {
    let response = flowable_client_unavailable();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "Flowable 客户端不可用");
}

#[actix_web::test]
async fn flowable_service_unavailable_matches_python_detail_contract() {
    let response = flowable_service_unavailable();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "Flowable 应用服务不可用");
}

#[actix_web::test]
async fn flowable_draft_service_unavailable_matches_python_detail_contract() {
    let response = flowable_draft_service_unavailable();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body()).await.expect("read body");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(payload["detail"], "流程草案生成服务不可用");
}

#[test]
fn flowable_stream_events_use_python_progress_and_done_names() {
    let progress = flowable_stream_event_to_sse(
        "req_1",
        "user_1",
        FlowableDraftAssistantStreamEvent::Progress {
            stage: "ai_request".to_string(),
            message: "正在建立流式会话".to_string(),
            mode: "contextual".to_string(),
        },
    );
    let done = flowable_stream_event_to_sse(
        "req_1",
        "user_1",
        FlowableDraftAssistantStreamEvent::Completed {
            mode: "contextual".to_string(),
            warning_count: 1,
            model: "model-x".to_string(),
        },
    );
    let error = flowable_stream_event_to_sse(
        "req_1",
        "user_1",
        FlowableDraftAssistantStreamEvent::Error {
            mode: "contextual".to_string(),
            message: "stream failed".to_string(),
        },
    );
    let text_delta = flowable_stream_event_to_sse(
        "req_1",
        "user_1",
        FlowableDraftAssistantStreamEvent::TextDelta {
            mode: "contextual".to_string(),
            delta: "hello".to_string(),
            accumulated_chars: 5,
        },
    );

    assert!(progress.starts_with("event: progress\n"));
    assert!(done.starts_with("event: done\n"));
    assert!(error.starts_with("event: error\n"));
    assert!(text_delta.contains("\"accumulated_chars\":5"));
    assert!(error.contains("\"stage\":\"stream\""));
    assert!(progress.contains("\"user_id\":\"user_1\""));
    assert!(done.contains("\"warnings_count\":1"));
    assert!(text_delta.contains("\"timestamp\":"));
}

#[actix_web::test]
async fn final_result_is_emitted_after_runtime_events() {
    const CHANNEL_CAPACITY: usize = 16;
    let (sender, mut receiver) = mpsc::channel::<String>(CHANNEL_CAPACITY);
    let runtime_sender = sender.clone();

    let event_forwarder = tokio::spawn(async move {
        let _ = runtime_sender.send("progress-1".to_string()).await;
        let _ = runtime_sender.send("text-delta-1".to_string()).await;
    });

    let _ = event_forwarder.await;
    let _ = sender.send("final-result".to_string()).await;

    assert_eq!(receiver.recv().await.as_deref(), Some("progress-1"));
    assert_eq!(receiver.recv().await.as_deref(), Some("text-delta-1"));
    assert_eq!(receiver.recv().await.as_deref(), Some("final-result"));
}
