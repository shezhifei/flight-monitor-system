//! AI 微模型执行路由
//!
//! 暴露微模型注册表查询和微模型执行 API。
//! - list/get: 返回模型元数据，包含 enabled 状态和 feature flag。
//! - execute: 受 feature flag 控制，typed input 校验，返回 advisory output。

pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::middleware::permissions::PermissionCheck;
pub(crate) use actix_web::{web, HttpResponse};
pub(crate) use fms_application::schemas::micro_model_schemas::MicroModelExecuteResponse;
pub(crate) use fms_application::services::micro_model_executor::MicroModelExecutor;
pub(crate) use fms_domain::models::micro_model::MicroModelRegistry;
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::{json, Value};
pub(crate) use std::sync::Arc;
#[derive(Debug, Deserialize)]
pub(crate) struct ModelListQuery {
    pub(crate) category: Option<String>,
    pub(crate) proposal_capable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ExecuteRequest {
    #[serde(default = "default_object")]
    pub(crate) input: Value,
    #[serde(default = "default_job_id")]
    pub(crate) job_id: String,
    #[serde(default = "default_run_id")]
    pub(crate) run_id: String,
    #[serde(default)]
    pub(crate) generate_proposals: bool,
    #[serde(default)]
    pub(crate) include_input_snapshot: bool,
}

// ===========================================================================
// Tests
// ===========================================================================

pub(crate) fn default_object() -> Value {
    json!({})
}

pub(crate) fn default_job_id() -> String {
    format!("job_{}", ulid::Ulid::new())
}

pub(crate) fn default_run_id() -> String {
    format!("run_{}", ulid::Ulid::new())
}

pub(crate) fn ok_resp(data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data }))
}
