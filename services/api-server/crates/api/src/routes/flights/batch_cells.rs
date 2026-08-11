//! PATCH /api/v2/flights/batch-cells — atomic multi-flight single-field update.

use std::sync::Arc;

use actix_web::{web, HttpResponse};
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;

use super::shared::{actor_id, ok_resp};
use fms_application::schemas::flight_schemas::FlightBatchCellUpdateRequest;
use fms_application::services::flight_batch_cell_update_service::{FlightBatchCellError, FlightBatchCellUpdateService};

/// PATCH /api/v2/flights/batch-cells
///
/// All-or-nothing: either every target is updated (same transaction + outbox)
/// or none are. Conflicts return 409 with code `FLIGHT_BATCH_CONFLICT`.
pub async fn batch_update_cells(
    svc: web::Data<Arc<FlightBatchCellUpdateService>>,
    body: web::Json<FlightBatchCellUpdateRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("flight:manage")?;

    let actor = actor_id(&claims).to_string();
    let is_admin = claims.0.is_admin.unwrap_or(false);
    let permissions = claims.0.permissions.clone();

    match svc.execute(body.into_inner(), &actor, is_admin, &permissions).await {
        Ok(response) => Ok(ok_resp(
            format!("批量更新成功：{} 条", response.updated_count),
            response,
        )),
        Err(FlightBatchCellError::Conflict { message, details }) => Ok(HttpResponse::Conflict().json(json!({
            "success": false,
            "error": {
                "code": "FLIGHT_BATCH_CONFLICT",
                "message": message,
                "details": details,
                "type": "conflict_error",
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }
        }))),
        Err(FlightBatchCellError::Validation(message)) => Err(ApiError::ValidationError(message)),
        Err(FlightBatchCellError::Forbidden(message)) => Err(ApiError::Forbidden(message)),
        Err(FlightBatchCellError::NotFound(message)) => Err(ApiError::NotFound(message)),
        Err(FlightBatchCellError::Internal(message)) => Err(ApiError::Internal(message)),
    }
}
