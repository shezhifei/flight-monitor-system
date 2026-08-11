//! 移动端专用路由。

use actix_multipart::Multipart;
use actix_web::{web, HttpResponse};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;
use fms_application::schemas::dispatch_schemas::{DeviceHeartbeatRequest, DeviceRegisterRequest};
use fms_application::services::flight_service::FlightService;
use fms_application::services::mobile_device_service::MobileDeviceService;
use fms_application::services::mobile_operations_service::MobileOperationsService;
use fms_application::services::mobile_upload_service::{MobileUploadService, UploadSource};
use fms_application::services::mobile_workbench_service::MobileWorkbenchService;

#[derive(Debug, Deserialize)]
struct WorkbenchQuery {
    pending_sync_action_count: Option<i64>,
    max_orders: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct EventFeedQuery {
    limit: Option<i64>,
}

/// GET /api/v2/mobile/workbench
async fn workbench(
    svc: web::Data<Arc<MobileWorkbenchService>>,
    flight_svc: Option<web::Data<Arc<FlightService>>>,
    query: web::Query<WorkbenchQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let pending_sync_action_count =
        validate_query_range(query.pending_sync_action_count, 0, 100000, "pending_sync_action_count")?.unwrap_or(0);
    let max_orders = validate_query_range(query.max_orders, 1, 200, "max_orders")?.unwrap_or(50);
    let mut result = svc
        .build_workbench(user_id, pending_sync_action_count, max_orders)
        .await?;
    if let Some(flight_svc) = flight_svc {
        populate_missing_workbench_gates(&mut result, flight_svc.get_ref().as_ref()).await?;
    }
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": result,
        "message": "mobile workbench loaded",
    })))
}

/// GET /api/v2/mobile/operations/events
async fn operations_events(
    svc: web::Data<Arc<MobileOperationsService>>,
    query: web::Query<EventFeedQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let limit = validate_query_range(query.limit, 1, 500, "limit")?.unwrap_or(120);
    let result = svc
        .build_event_feed(user_id, claims.0.is_admin.unwrap_or(false), limit)
        .await?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": result,
        "message": "mobile operations events loaded",
    })))
}

/// POST /api/v2/mobile/uploads
///
/// Upload size limit is enforced by the global `PayloadConfig` in `main.rs`
/// (default 20 MB). Route-level duplicate checks are removed to avoid
/// inconsistent limits.
async fn upload_asset(
    svc: web::Data<Arc<MobileUploadService>>,
    claims: JwtAuth,
    mut multipart: Multipart,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let user_id = extract_user_id(&claims)?;
    let mut category = "dispatch_issue".to_string();
    let mut file_name: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut file_bytes = Vec::new();

    while let Some(field_result) = multipart.next().await {
        let mut field = field_result.map_err(|error| ApiError::BadRequest(format!("multipart 解析失败: {error}")))?;
        let field_name = field
            .content_disposition()
            .and_then(|disposition| disposition.get_name())
            .unwrap_or("")
            .to_string();
        if field_name == "category" {
            let mut text_bytes = Vec::new();
            while let Some(chunk_result) = field.next().await {
                let chunk = chunk_result.map_err(|error| ApiError::BadRequest(format!("读取分类字段失败: {error}")))?;
                text_bytes.extend_from_slice(&chunk);
            }
            if let Ok(value) = String::from_utf8(text_bytes) {
                let normalized = value.trim();
                if !normalized.is_empty() {
                    category = normalized.to_string();
                }
            }
            continue;
        }

        if field_name == "file" {
            file_name = field
                .content_disposition()
                .and_then(|disposition| disposition.get_filename())
                .map(|value| value.to_string());
            content_type = field.content_type().map(|value| value.to_string());
            while let Some(chunk_result) = field.next().await {
                let chunk = chunk_result.map_err(|error| ApiError::BadRequest(format!("读取上传文件失败: {error}")))?;
                file_bytes.extend_from_slice(&chunk);
            }
        }
    }

    let response = svc
        .save_upload(
            user_id,
            file_name.as_deref().unwrap_or("upload.bin"),
            content_type.as_deref(),
            UploadSource::InMemory(file_bytes),
            &category,
            HashMap::from([("uploader".to_string(), serde_json::Value::String(user_id.to_string()))]),
        )
        .await?;

    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": response,
        "message": "upload succeeded",
    })))
}

/// GET /api/v2/mobile/uploads/{upload_id}/content
async fn download_asset(
    req: actix_web::HttpRequest,
    svc: web::Data<Arc<MobileUploadService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<actix_web::HttpResponse, ApiError> {
    claims.ensure_permission("dispatch:view")?;
    let upload_id = path.into_inner();
    let Some((asset, absolute_path)) = svc.resolve_content_path(&upload_id).await? else {
        return Err(ApiError::NotFound("upload not found".into()));
    };
    let filename = svc.build_download_filename(&asset).replace('"', "_");
    let mime_type: actix_web::mime::Mime = asset
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string())
        .parse()
        .unwrap_or(actix_web::mime::APPLICATION_OCTET_STREAM);
    let named_file =
        actix_files::NamedFile::open(&absolute_path).map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(named_file
        .set_content_disposition(actix_web::http::header::ContentDisposition {
            disposition: actix_web::http::header::DispositionType::Attachment,
            parameters: vec![actix_web::http::header::DispositionParam::Filename(filename)],
        })
        .set_content_type(mime_type)
        .into_response(&req))
}

/// POST /api/v2/mobile/devices/register
async fn register_device(
    svc: web::Data<Arc<MobileDeviceService>>,
    claims: JwtAuth,
    body: web::Json<DeviceRegisterRequest>,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let saved = svc.register_device(user_id, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": saved,
        "message": "mobile device registered",
    })))
}

/// DELETE /api/v2/mobile/devices/{device_id}
async fn unregister_device(
    svc: web::Data<Arc<MobileDeviceService>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let device_id = path.into_inner();
    let success = svc.unregister_device(user_id, &device_id).await?;
    if !success {
        return Err(ApiError::NotFound("device not found".into()));
    }
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": { "device_id": device_id, "unregistered": true },
        "message": "mobile device unregistered",
    })))
}

/// POST /api/v2/mobile/devices/{device_id}/heartbeat
async fn device_heartbeat(
    svc: web::Data<Arc<MobileDeviceService>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<DeviceHeartbeatRequest>,
) -> Result<HttpResponse, ApiError> {
    let user_id = extract_user_id(&claims)?;
    let device_id = path.into_inner();
    let Some((saved, delivery_channels)) = svc.heartbeat_device(user_id, &device_id, body.into_inner()).await? else {
        return Err(ApiError::NotFound("device not found".into()));
    };
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "device_id": saved.device_id,
            "user_id": saved.user_id,
            "platform": saved.platform,
            "push_channel": saved.push_channel,
            "push_token": saved.push_token,
            "app_version": saved.app_version,
            "os_version": saved.os_version,
            "device_model": saved.device_model,
            "manufacturer": saved.manufacturer,
            "is_active": saved.is_active,
            "last_heartbeat_at": saved.last_heartbeat_at,
            "registered_at": saved.registered_at,
            "updated_at": saved.updated_at,
            "metadata": saved.metadata,
            "delivery_channels": delivery_channels,
        },
        "message": "heartbeat received",
    })))
}

fn extract_user_id(claims: &JwtAuth) -> Result<&str, ApiError> {
    claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))
}

fn validate_query_range(value: Option<i64>, min: i64, max: i64, field_name: &str) -> Result<Option<i64>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if (min..=max).contains(&value) {
        return Ok(Some(value));
    }
    Err(ApiError::ValidationError(format!(
        "{field_name} must be between {min} and {max}"
    )))
}

async fn populate_missing_workbench_gates(
    payload: &mut serde_json::Value,
    flight_svc: &FlightService,
) -> Result<(), ApiError> {
    let Some(my_orders) = payload.get_mut("my_orders").and_then(|value| value.as_array_mut()) else {
        return Ok(());
    };

    let missing_gate_flight_ids = my_orders
        .iter()
        .filter(|item| is_blank_value(item.get("gate")))
        .filter_map(|item| item.get("flight_id").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .fold(Vec::<String>::new(), |mut acc, flight_id| {
            if !acc.iter().any(|item| item == flight_id) {
                acc.push(flight_id.to_string());
            }
            acc
        });

    if missing_gate_flight_ids.is_empty() {
        return Ok(());
    }

    let gate_map = flight_svc.batch_get_gate_map(&missing_gate_flight_ids).await?;
    for item in my_orders.iter_mut() {
        if !is_blank_value(item.get("gate")) {
            continue;
        }
        let Some(flight_id) = item.get("flight_id").and_then(|value| value.as_str()).map(str::trim) else {
            continue;
        };
        let Some(Some(gate)) = gate_map.get(flight_id) else {
            continue;
        };
        if let Some(map) = item.as_object_mut() {
            map.insert("gate".to_string(), serde_json::Value::String(gate.clone()));
        }
    }

    Ok(())
}

fn is_blank_value(value: Option<&serde_json::Value>) -> bool {
    match value {
        None => true,
        Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(value)) => value.trim().is_empty(),
        _ => false,
    }
}

/// 注册移动端路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v2/mobile")
            .route("/workbench", web::get().to(workbench))
            .route("/operations/events", web::get().to(operations_events))
            .route("/uploads", web::post().to(upload_asset))
            .route("/uploads/{upload_id}/content", web::get().to(download_asset))
            .route("/devices/register", web::post().to(register_device))
            .route("/devices/{device_id}", web::delete().to(unregister_device))
            .route("/devices/{device_id}/heartbeat", web::post().to(device_heartbeat)),
    );
}
