//! 统一响应格式

use serde::Serialize;

/// API 统一成功响应
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
    pub timestamp: String,
}

/// API 统一错误响应
#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub error_type: String,
    /// 机器可辨识的错误类别（如 auth/config/database/network/unknown）。
    /// 仅在错误来源能归类时出现；客户端应据此分支而非解析 message。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn ok_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message.into()),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl ApiErrorResponse {
    pub fn new(status_code: u16, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: ApiErrorDetail::new(status_code, message, None),
        }
    }

    pub fn with_details(status_code: u16, message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            success: false,
            error: ApiErrorDetail::new(status_code, message, Some(details)),
        }
    }

    /// 构造带 typed kind 的错误响应；message 应为泛化文案，原始错误文本不得传入。
    pub fn with_kind(status_code: u16, message: impl Into<String>, kind: impl Into<String>) -> Self {
        let mut error = ApiErrorDetail::new(status_code, message, None);
        error.kind = Some(kind.into());
        Self { success: false, error }
    }
}

impl ApiErrorDetail {
    fn new(status_code: u16, message: impl Into<String>, details: Option<serde_json::Value>) -> Self {
        Self {
            code: format!("HTTP_{status_code}"),
            message: message.into(),
            details,
            timestamp: chrono::Utc::now().to_rfc3339(),
            error_type: error_type_for_status(status_code).to_string(),
            kind: None,
        }
    }
}

fn error_type_for_status(status_code: u16) -> &'static str {
    match status_code {
        401 => "authentication_error",
        403 => "authorization_error",
        404 => "not_found_error",
        422 => "validation_error",
        503 => "service_unavailable_error",
        _ => "http_error",
    }
}

#[cfg(test)]
mod tests {
    use super::ApiErrorResponse;

    #[test]
    fn builds_python_compatible_error_shape() {
        let response =
            ApiErrorResponse::with_details(422, "输入验证失败", serde_json::json!([{"loc": ["body", "field"]}]));

        assert!(!response.success);
        assert_eq!(response.error.code, "HTTP_422");
        assert_eq!(response.error.message, "输入验证失败");
        assert_eq!(response.error.error_type, "validation_error");
        assert_eq!(
            response.error.details,
            Some(serde_json::json!([{"loc": ["body", "field"]}]))
        );
        assert!(response.error.timestamp.contains('T'));
    }
}
