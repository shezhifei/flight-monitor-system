//! 统一错误处理 — 将领域/基础设施错误映射为 HTTP 响应

use actix_web::{
    error::{InternalError, JsonPayloadError, QueryPayloadError},
    HttpRequest, HttpResponse, ResponseError,
};
use fms_application::schemas::response::ApiErrorResponse;
use std::fmt;

use crate::services::runtime_error_monitor::{
    current_request_operation, record_runtime_error_background, take_api_error_recording_suppressed, RuntimeErrorInput,
};
use crate::services::runtime_error_types::{ErrorCategory, RuntimeErrorKind, Severity};
use std::str::FromStr;

/// 基础设施错误的机器可辨识类别。响应体只暴露该 kind，原始 message 仅写日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Auth,
    Config,
    Database,
    Network,
    Unknown,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Config => "config",
            Self::Database => "database",
            Self::Network => "network",
            Self::Unknown => "unknown",
        }
    }

    fn http_status(self) -> actix_web::http::StatusCode {
        use actix_web::http::StatusCode;
        match self {
            Self::Auth => StatusCode::UNAUTHORIZED,
            Self::Network => StatusCode::SERVICE_UNAVAILABLE,
            Self::Config | Self::Database | Self::Unknown => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 暴露给客户端的泛化文案（旧 message 字段，前端迁移期保留）。
    fn public_message(self) -> &'static str {
        match self {
            Self::Auth => "认证失败",
            Self::Config => "服务配置错误",
            Self::Database => "数据存储错误",
            Self::Network => "依赖服务暂不可用",
            Self::Unknown => "内部服务错误",
        }
    }

    fn runtime_category(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Config | Self::Unknown => "system",
            Self::Database | Self::Network => "infrastructure",
        }
    }
}

/// API 层统一错误类型
#[derive(Debug)]
pub enum ApiError {
    /// 400 Bad Request
    BadRequest(String),
    /// 400 Bad Request with structured details
    BadRequestWithDetails {
        message: String,
        details: serde_json::Value,
    },
    /// 401 Unauthorized
    Unauthorized(String),
    /// 403 Forbidden
    Forbidden(String),
    /// 404 Not Found
    NotFound(String),
    /// 410 Gone（已下线的写路径，如只读化的历史目录）
    Gone(String),
    /// 409 Conflict
    Conflict(String),
    /// 422 Validation Error
    ValidationError(String),
    /// 422 Validation Error with structured details
    ValidationErrorWithDetails {
        message: String,
        details: serde_json::Value,
    },
    /// 500 Internal Server Error
    Internal(String),
    /// 503 Service Unavailable
    ServiceUnavailable(String),
    /// 501 Not Implemented（预留能力，如人脸占席）
    NotImplemented(String),
    /// 基础设施错误：响应体只暴露 typed kind，原始 message 仅写日志。
    Infra { kind: ErrorKind, message: String },
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "Bad Request: {msg}"),
            Self::BadRequestWithDetails { message, .. } => write!(f, "Bad Request: {message}"),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {msg}"),
            Self::Forbidden(msg) => write!(f, "Forbidden: {msg}"),
            Self::NotFound(msg) => write!(f, "Not Found: {msg}"),
            Self::Gone(msg) => write!(f, "Gone: {msg}"),
            Self::Conflict(msg) => write!(f, "Conflict: {msg}"),
            Self::ValidationError(msg) => write!(f, "Validation Error: {msg}"),
            Self::ValidationErrorWithDetails { message, .. } => {
                write!(f, "Validation Error: {message}")
            }
            Self::Internal(msg) => write!(f, "Internal Error: {msg}"),
            Self::ServiceUnavailable(msg) => write!(f, "Service Unavailable: {msg}"),
            Self::NotImplemented(msg) => write!(f, "Not Implemented: {msg}"),
            Self::Infra { kind, message } => write!(f, "Infra Error [{}]: {message}", kind.as_str()),
        }
    }
}

impl ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        if !take_api_error_recording_suppressed() {
            record_api_error_with_context(self);
        }

        let (status, payload) = match self {
            Self::BadRequest(msg) => (actix_web::http::StatusCode::BAD_REQUEST, msg.clone()),
            Self::BadRequestWithDetails { message, details } => {
                return HttpResponse::BadRequest().json(ApiErrorResponse::with_details(
                    actix_web::http::StatusCode::BAD_REQUEST.as_u16(),
                    message.clone(),
                    details.clone(),
                ));
            }
            Self::Unauthorized(msg) => (actix_web::http::StatusCode::UNAUTHORIZED, msg.clone()),
            Self::Forbidden(msg) => (actix_web::http::StatusCode::FORBIDDEN, msg.clone()),
            Self::NotFound(msg) => (actix_web::http::StatusCode::NOT_FOUND, msg.clone()),
            Self::Gone(msg) => (actix_web::http::StatusCode::GONE, msg.clone()),
            Self::Conflict(msg) => (actix_web::http::StatusCode::CONFLICT, msg.clone()),
            Self::ValidationError(msg) => (actix_web::http::StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            Self::ValidationErrorWithDetails { message, details } => {
                return HttpResponse::UnprocessableEntity().json(ApiErrorResponse::with_details(
                    actix_web::http::StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
                    message.clone(),
                    details.clone(),
                ));
            }
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "内部服务器错误");
                (
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "内部服务器错误".to_string(),
                )
            }
            Self::ServiceUnavailable(msg) => (actix_web::http::StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            Self::NotImplemented(msg) => (actix_web::http::StatusCode::NOT_IMPLEMENTED, msg.clone()),
            Self::Infra { kind, message } => {
                // 原始基础设施错误文本仅写日志；响应体只暴露 typed kind + 泛化文案。
                tracing::error!(error = %message, kind = kind.as_str(), "基础设施错误");
                let status = kind.http_status();
                return HttpResponse::build(status).json(ApiErrorResponse::with_kind(
                    status.as_u16(),
                    kind.public_message(),
                    kind.as_str(),
                ));
            }
        };

        HttpResponse::build(status).json(ApiErrorResponse::new(status.as_u16(), payload))
    }
}

pub(crate) fn record_api_error_with_context(error: &ApiError) {
    if let Some(record) = runtime_error_input_for_api_error(error) {
        record_runtime_error_background(record);
    }
}

fn runtime_error_input_for_api_error(error: &ApiError) -> Option<RuntimeErrorInput> {
    match error {
        ApiError::Internal(message) => Some(RuntimeErrorInput {
            error_type: RuntimeErrorKind::ApiInternalError,
            message: message.clone(),
            severity: Severity::High,
            category: ErrorCategory::System,
            operation: current_request_operation(),
            details: None,
        }),
        ApiError::ServiceUnavailable(message) => Some(RuntimeErrorInput {
            error_type: RuntimeErrorKind::ApiServiceUnavailable,
            message: message.clone(),
            severity: Severity::High,
            category: ErrorCategory::Infrastructure,
            operation: current_request_operation(),
            details: None,
        }),
        ApiError::Infra { kind, message } => Some(RuntimeErrorInput {
            error_type: RuntimeErrorKind::ApiInfraError,
            message: message.clone(),
            severity: Severity::High,
            category: ErrorCategory::from_str(kind.runtime_category()).unwrap_or(ErrorCategory::Other),
            operation: current_request_operation(),
            details: None,
        }),
        _ => None,
    }
}

pub fn default_json_payload_error_handler(err: JsonPayloadError, req: &HttpRequest) -> actix_web::Error {
    let details = serde_json::json!([
        {
            "type": "json_invalid",
            "loc": ["body"],
            "msg": "JSON decode error",
            "input": serde_json::Value::Null,
            "ctx": {
                "error": err.to_string(),
                "path": req.path().to_string(),
            }
        }
    ]);

    InternalError::from_response(
        err,
        HttpResponse::UnprocessableEntity().json(ApiErrorResponse::with_details(
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            "输入验证失败",
            details,
        )),
    )
    .into()
}

pub fn default_query_payload_error_handler(err: QueryPayloadError, req: &HttpRequest) -> actix_web::Error {
    let details = serde_json::json!([
        {
            "type": "query_invalid",
            "loc": ["query"],
            "msg": "Query validation error",
            "input": serde_json::Value::Null,
            "ctx": {
                "error": err.to_string(),
                "path": req.path().to_string(),
            }
        }
    ]);

    InternalError::from_response(
        err,
        HttpResponse::UnprocessableEntity().json(ApiErrorResponse::with_details(
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            "输入验证失败",
            details,
        )),
    )
    .into()
}

/// 从 DomainError 转换
impl From<fms_domain::error::DomainError> for ApiError {
    fn from(err: fms_domain::error::DomainError) -> Self {
        match err {
            fms_domain::error::DomainError::NotFound { entity_type, id } => {
                Self::NotFound(format!("{entity_type} (id={id}) 未找到"))
            }
            fms_domain::error::DomainError::ValidationError(msg) => Self::ValidationError(msg),
            fms_domain::error::DomainError::InvalidStateTransition { from, to } => {
                Self::BadRequest(format!("非法状态转换: {from} → {to}"))
            }
            fms_domain::error::DomainError::BusinessRuleViolation(msg) => Self::BadRequest(msg),
            fms_domain::error::DomainError::BusinessRuleViolationWithDetails { message, details } => {
                Self::BadRequestWithDetails { message, details }
            }
            fms_domain::error::DomainError::PermissionDenied(msg) => Self::Forbidden(msg),
            fms_domain::error::DomainError::Unauthorized(msg) => Self::Unauthorized(msg),
            fms_domain::error::DomainError::Conflict(msg) => Self::Conflict(msg),
            fms_domain::error::DomainError::Internal(msg) => Self::Internal(msg),
            fms_domain::error::DomainError::ConcurrencyConflict(msg) => Self::Conflict(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApiError;
    use super::{default_json_payload_error_handler, default_query_payload_error_handler};
    use actix_web::{body::to_bytes, http::StatusCode, test, web, App, HttpResponse, ResponseError};
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct DummyJsonPayload {
        required: String,
    }

    #[derive(Debug, Deserialize)]
    struct DummyQueryPayload {
        include_status: bool,
    }

    #[actix_web::test]
    async fn validation_error_matches_python_error_envelope_shape() {
        let response = ApiError::ValidationError("输入验证失败".to_string()).error_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(payload["success"], false);
        assert_eq!(payload["error"]["code"], "HTTP_422");
        assert_eq!(payload["error"]["message"], "输入验证失败");
        assert_eq!(payload["error"]["type"], "validation_error");
        assert!(payload["error"]["timestamp"].as_str().is_some());
    }

    #[actix_web::test]
    async fn infra_database_error_exposes_typed_kind_without_raw_text() {
        let response = super::ApiError::Infra {
            kind: super::ErrorKind::Database,
            message: "connection refused to db host 10.0.0.1:5432".to_string(),
        }
        .error_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = to_bytes(response.into_body()).await.expect("read body");
        let raw = String::from_utf8(body.to_vec()).expect("utf8 body");

        // 原始基础设施错误文本不得出现在响应体中（只经 tracing 记录）
        assert!(!raw.contains("connection refused"), "raw infra text leaked: {raw}");
        assert!(!raw.contains("10.0.0.1"), "raw infra host leaked: {raw}");

        let payload: serde_json::Value = serde_json::from_slice(raw.as_bytes()).expect("json body");
        assert_eq!(payload["success"], false);
        assert_eq!(payload["error"]["kind"], "database");
        assert_eq!(payload["error"]["code"], "HTTP_500");
        // 旧 message 字段保留（前端迁移期），但只含泛化文案
        assert!(payload["error"]["message"].as_str().is_some());
    }

    #[actix_web::test]
    async fn infra_auth_error_maps_to_401_with_auth_kind() {
        let response = super::ApiError::Infra {
            kind: super::ErrorKind::Auth,
            message: "token expired secret=abc".to_string(),
        }
        .error_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = to_bytes(response.into_body()).await.expect("read body");
        let raw = String::from_utf8(body.to_vec()).expect("utf8 body");
        assert!(!raw.contains("secret=abc"), "raw auth detail leaked: {raw}");

        let payload: serde_json::Value = serde_json::from_slice(raw.as_bytes()).expect("json body");
        assert_eq!(payload["error"]["kind"], "auth");
    }

    #[actix_web::test]
    async fn not_found_error_uses_python_error_type() {
        let response = ApiError::NotFound("not found".to_string()).error_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(payload["error"]["code"], "HTTP_404");
        assert_eq!(payload["error"]["type"], "not_found_error");
        assert_eq!(payload["error"]["message"], "not found");
    }

    #[actix_web::test]
    async fn global_json_error_handler_returns_python_style_422_payload() {
        let app = test::init_service(
            App::new()
                .app_data(web::JsonConfig::default().error_handler(default_json_payload_error_handler))
                .route(
                    "/json",
                    web::post().to(|payload: web::Json<DummyJsonPayload>| async move {
                        let _ = payload.required.len();
                        HttpResponse::Ok().finish()
                    }),
                ),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/json")
                .set_payload("{invalid")
                .insert_header(("content-type", "application/json"))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["error"]["message"], "输入验证失败");
        assert_eq!(payload["error"]["type"], "validation_error");
        assert_eq!(payload["error"]["details"][0]["loc"][0], "body");
    }

    #[actix_web::test]
    async fn global_query_error_handler_returns_python_style_422_payload() {
        let app = test::init_service(
            App::new()
                .app_data(web::QueryConfig::default().error_handler(default_query_payload_error_handler))
                .route(
                    "/query",
                    web::get().to(|query: web::Query<DummyQueryPayload>| async move {
                        let _ = query.include_status;
                        HttpResponse::Ok().finish()
                    }),
                ),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/query?include_status=not-a-bool")
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body()).await.expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["error"]["message"], "输入验证失败");
        assert_eq!(payload["error"]["type"], "validation_error");
        assert_eq!(payload["error"]["details"][0]["loc"][0], "query");
    }
}
