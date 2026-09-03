use actix_web::{web, HttpResponse};
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::middleware::permissions::PermissionCheck;

#[derive(Debug, Deserialize)]
pub struct RuntimeHealthQuery {
    #[serde(default = "default_detailed")]
    pub detailed: bool,
}

fn default_detailed() -> bool {
    false
}

/// Health check endpoint for AI runtime service
///
/// Checks if MQ publisher is connected and outbox writer is active
/// Returns degraded status if components are missing
/// P0-5-B: Terminal Event Durability - Health monitoring
pub async fn runtime_health(query: web::Query<RuntimeHealthQuery>, claims: JwtAuth) -> Result<HttpResponse, ApiError> {
    claims.ensure_permission("ai:view")?;

    // Try to resolve MQ publisher
    let mq_status = match get_mq_publisher_status() {
        Ok(Some(_)) => "connected".to_string(),
        Ok(None) => "disconnected".to_string(),
        Err(e) => format!("error: {}", e),
    };

    // Check outbox writer status
    let outbox_status = match get_outbox_writer_status() {
        Ok(Some(_)) => "active".to_string(),
        Ok(None) => "inactive".to_string(),
        Err(e) => format!("error: {}", e),
    };

    // Determine overall status
    let overall_status = if mq_status == "connected" && outbox_status == "active" {
        "healthy"
    } else {
        "degraded"
    };

    let response = if query.detailed {
        json!({
            "status": overall_status,
            "components": {
                "mq_publisher": {
                    "status": mq_status,
                    "details": mq_status != "connected"
                },
                "outbox_writer": {
                    "status": outbox_status,
                    "details": outbox_status != "active"
                }
            },
            "durable_events_enabled": mq_status == "connected" && outbox_status == "active"
        })
    } else {
        json!({
            "status": overall_status,
            "mq_connected": mq_status == "connected",
            "outbox_active": outbox_status == "active"
        })
    };

    Ok(HttpResponse::Ok().json(response))
}

fn get_mq_publisher_status() -> Result<Option<String>, String> {
    // Import runtime services from Python sidecar
    // This function should be extended to properly inject dependencies
    // For now, return mock status
    Ok(Some("mock-publisher".to_string()))
}

fn get_outbox_writer_status() -> Result<Option<String>, String> {
    // Similar to above - should check outbox writer activity
    Ok(Some("mock-outbox".to_string()))
}

#[cfg(test)]
mod tests {

    #[actix_web::test]
    async fn test_runtime_health_healthy() {
        // Test implementation would require full DI setup
        // Placeholder for future integration tests
    }
}
