use serde_json::{json, Value};
use std::sync::Arc;

use fms_domain::error::DomainError;

use super::alert_dispatch_service::AlertDispatchService;
use crate::types::ConcreteAuthService;

pub struct SystemOpsService {
    auth_service: Arc<ConcreteAuthService>,
    alert_dispatch_service: Arc<AlertDispatchService>,
}

impl SystemOpsService {
    pub fn new(auth_service: Arc<ConcreteAuthService>, alert_dispatch_service: Arc<AlertDispatchService>) -> Self {
        Self {
            auth_service,
            alert_dispatch_service,
        }
    }

    pub async fn get_public_health(&self) -> Result<Value, DomainError> {
        let runtime = self.auth_service.get_session_runtime_status().await?;
        let redis_available = runtime.redis_available;
        let is_healthy = runtime.mode.eq_ignore_ascii_case("redis") && redis_available;
        Ok(json!({
            "status": if is_healthy { "healthy" } else { "degraded" },
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "services": {
                "redis": {
                    "available": redis_available,
                    "mode": runtime.mode,
                    "circuit_state": runtime.circuit_state,
                    "fallback_duration_seconds": runtime.fallback_duration_seconds,
                }
            }
        }))
    }

    pub async fn get_online_status_runtime_status(&self) -> Result<Value, DomainError> {
        let runtime = self.auth_service.get_session_runtime_status().await?;
        Ok(serde_json::to_value(runtime).unwrap_or_else(|_| json!({})))
    }

    pub async fn send_test_alert(
        &self,
        title: &str,
        message: &str,
        level: &str,
        channels: &[String],
        recipients: &[String],
        requested_by: &str,
    ) -> Result<Value, DomainError> {
        self.alert_dispatch_service
            .dispatch_test_alert(title, message, level, channels, recipients, requested_by)
            .await;
        Ok(json!({
            "level": level,
            "channels": channels,
            "recipients": recipients,
        }))
    }
}
