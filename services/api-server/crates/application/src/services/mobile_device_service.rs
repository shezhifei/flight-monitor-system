//! 移动设备服务。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};

use fms_domain::error::DomainError;
use fms_domain::models::mobile::MobileDeviceRegistration;
use fms_domain::ports::mobile_repository::MobileDeviceRepository;

use crate::schemas::dispatch_schemas::{DeviceHeartbeatRequest, DeviceRegisterRequest};

pub trait MobileRealtimeMetricsRecorder: Send + Sync {
    fn record_sse_reconnects(&self, count: u64);
}

pub struct MobileDeviceService {
    repo: Arc<dyn MobileDeviceRepository + Send + Sync>,
    stale_minutes: i64,
    metrics_recorder: Option<Arc<dyn MobileRealtimeMetricsRecorder + Send + Sync>>,
}

impl MobileDeviceService {
    pub fn new(repo: Arc<dyn MobileDeviceRepository + Send + Sync>, stale_minutes: i64) -> Self {
        Self {
            repo,
            stale_minutes: stale_minutes.max(1),
            metrics_recorder: None,
        }
    }
}

impl MobileDeviceService {
    pub fn with_metrics_recorder(
        mut self,
        metrics_recorder: Arc<dyn MobileRealtimeMetricsRecorder + Send + Sync>,
    ) -> Self {
        self.metrics_recorder = Some(metrics_recorder);
        self
    }

    pub async fn register_device(
        &self,
        user_id: &str,
        payload: DeviceRegisterRequest,
    ) -> Result<MobileDeviceRegistration, DomainError> {
        let normalized_user_id = normalize_required(user_id, "user_id")?;
        let device_id = normalize_required_limited(&payload.device_id, 64, "device_id")?;
        let now = Utc::now();

        let item = MobileDeviceRegistration {
            device_id,
            user_id: normalized_user_id,
            platform: normalize_optional_limited(payload.platform.as_deref(), 32, "platform")?
                .unwrap_or_else(|| "android".to_string()),
            push_channel: normalize_push_channel(payload.push_channel.as_deref())?,
            push_token: normalize_optional_limited(payload.push_token.as_deref(), 1024, "push_token")?,
            app_version: normalize_optional_limited(payload.app_version.as_deref(), 64, "app_version")?,
            os_version: normalize_optional_limited(payload.os_version.as_deref(), 64, "os_version")?,
            device_model: normalize_optional_limited(payload.device_model.as_deref(), 128, "device_model")?,
            manufacturer: normalize_optional_limited(payload.manufacturer.as_deref(), 64, "manufacturer")?,
            is_active: true,
            last_heartbeat_at: now,
            registered_at: now,
            updated_at: now,
            metadata: payload.metadata,
        };
        self.repo.upsert_device(&item).await
    }

    pub async fn unregister_device(&self, user_id: &str, device_id: &str) -> Result<bool, DomainError> {
        let normalized_user_id = normalize_required(user_id, "user_id")?;
        let normalized_device_id = normalize_required(device_id, "device_id")?;
        self.repo
            .deactivate_device(&normalized_user_id, &normalized_device_id)
            .await
    }

    pub async fn heartbeat_device(
        &self,
        user_id: &str,
        device_id: &str,
        payload: DeviceHeartbeatRequest,
    ) -> Result<Option<(MobileDeviceRegistration, HashMap<String, bool>)>, DomainError> {
        let normalized_user_id = normalize_required(user_id, "user_id")?;
        let normalized_device_id = normalize_required_limited(device_id, 64, "device_id")?;
        let reconnect_count = extract_sse_reconnect_count(&payload);
        let mut metadata = payload.metadata;
        metadata.insert(
            "heartbeat_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        if let Some(network_status) =
            normalize_optional_limited(payload.network_status.as_deref(), 32, "network_status")?
        {
            metadata.insert("network_status".to_string(), serde_json::Value::String(network_status));
        }
        if let Some(battery_level) = payload.battery_level {
            validate_battery_level(battery_level)?;
            metadata.insert(
                "battery_level".to_string(),
                serde_json::Value::Number(battery_level.into()),
            );
        }

        let Some(saved) = self
            .repo
            .heartbeat_device(
                &normalized_user_id,
                &normalized_device_id,
                &serde_json::to_value(metadata).unwrap_or_else(|_| serde_json::json!({})),
            )
            .await?
        else {
            return Ok(None);
        };

        if reconnect_count > 0 {
            if let Some(metrics_recorder) = &self.metrics_recorder {
                metrics_recorder.record_sse_reconnects(reconnect_count);
            }
        }

        let channels = self.resolve_delivery_channels(&normalized_user_id).await?;
        Ok(Some((saved, channels)))
    }

    pub async fn resolve_delivery_channels(&self, user_id: &str) -> Result<HashMap<String, bool>, DomainError> {
        let normalized_user_id = normalize_required(user_id, "user_id")?;
        let devices = self.repo.list_active_devices(&normalized_user_id, 100).await?;
        let stale_deadline = Utc::now() - Duration::minutes(self.stale_minutes);

        let has_fresh_push_device = devices.iter().any(|device| {
            device.is_active
                && device.push_channel != "none"
                && device.push_token.as_deref().unwrap_or("").trim().is_empty().not()
                && device.last_heartbeat_at >= stale_deadline
        });

        Ok(HashMap::from([
            ("push".to_string(), has_fresh_push_device),
            ("sse".to_string(), true),
            ("in_app".to_string(), true),
        ]))
    }
}

fn normalize_required(value: &str, field_name: &str) -> Result<String, DomainError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError(format!("{field_name} is required")));
    }
    Ok(normalized.to_string())
}

fn normalize_required_limited(value: &str, max_length: usize, field_name: &str) -> Result<String, DomainError> {
    let normalized = normalize_required(value, field_name)?;
    validate_max_length(&normalized, max_length, field_name)?;
    Ok(normalized)
}

fn normalize_optional_limited(
    value: Option<&str>,
    max_length: usize,
    field_name: &str,
) -> Result<Option<String>, DomainError> {
    let text = value.unwrap_or("").trim();
    if text.is_empty() {
        return Ok(None);
    }
    validate_max_length(text, max_length, field_name)?;
    Ok(Some(text.to_string()))
}

fn normalize_push_channel(value: Option<&str>) -> Result<String, DomainError> {
    let channel = normalize_optional_limited(value, 32, "push_channel")?
        .unwrap_or_else(|| "none".to_string())
        .to_ascii_lowercase();
    let normalized = if channel.is_empty() {
        "none".to_string()
    } else {
        channel
    };
    match normalized.as_str() {
        "none" | "fcm" | "hms" | "xiaomi" | "oppo" | "vivo" | "wecom" => Ok(normalized),
        _ => Err(DomainError::ValidationError(format!(
            "unsupported push_channel: {normalized}"
        ))),
    }
}

fn validate_battery_level(value: i32) -> Result<(), DomainError> {
    if (0..=100).contains(&value) {
        return Ok(());
    }
    Err(DomainError::ValidationError(
        "battery_level must be between 0 and 100".into(),
    ))
}

fn validate_max_length(value: &str, max_length: usize, field_name: &str) -> Result<(), DomainError> {
    if value.chars().count() <= max_length {
        return Ok(());
    }
    Err(DomainError::ValidationError(format!(
        "{field_name} must be at most {max_length} characters"
    )))
}

trait BoolNot {
    fn not(self) -> bool;
}

impl BoolNot for bool {
    fn not(self) -> bool {
        !self
    }
}

fn extract_sse_reconnect_count(payload: &DeviceHeartbeatRequest) -> u64 {
    let mut reconnect_count = if payload.sse_reconnected.unwrap_or(false) {
        1_u64
    } else {
        0_u64
    };

    if let Some(count) = payload.sse_reconnect_count {
        reconnect_count = reconnect_count.max(count.max(0) as u64);
    }

    if payload
        .metadata
        .get("sse_reconnected")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        reconnect_count = reconnect_count.max(1);
    }

    if let Some(count) = payload
        .metadata
        .get("sse_reconnect_count")
        .and_then(serde_json::Value::as_i64)
    {
        reconnect_count = reconnect_count.max(count.max(0) as u64);
    }

    reconnect_count
}

#[cfg(test)]
mod tests {
    use super::extract_sse_reconnect_count;
    use crate::schemas::dispatch_schemas::DeviceHeartbeatRequest;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn reconnect_count_prefers_explicit_top_level_counter() {
        let payload = DeviceHeartbeatRequest {
            network_status: None,
            battery_level: None,
            sse_reconnected: Some(true),
            sse_reconnect_count: Some(3),
            metadata: HashMap::new(),
        };

        assert_eq!(extract_sse_reconnect_count(&payload), 3);
    }

    #[test]
    fn reconnect_count_falls_back_to_metadata() {
        let payload = DeviceHeartbeatRequest {
            network_status: None,
            battery_level: None,
            sse_reconnected: None,
            sse_reconnect_count: None,
            metadata: HashMap::from([
                ("sse_reconnected".to_string(), json!(true)),
                ("sse_reconnect_count".to_string(), json!(2)),
            ]),
        };

        assert_eq!(extract_sse_reconnect_count(&payload), 2);
    }
}
