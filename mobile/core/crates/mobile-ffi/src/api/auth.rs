//! Auth + device exports (plan §4 Auth/设备).

use mobile_core::dto::mobile::{
    MobileDeviceHeartbeatRequest, MobileDeviceRegisterRequest,
};

use super::runtime;

/// Device registration payload (mirror of
/// `mobile_core::dto::mobile::MobileDeviceRegisterRequest`). `metadata` is
/// passed as a JSON string because frb has no arbitrary-JSON map type.
pub struct DeviceRegisterInfo {
    pub device_id: String,
    pub push_channel: Option<String>,
    pub push_token: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub manufacturer: Option<String>,
    pub metadata_json: Option<String>,
}

/// Device heartbeat payload (mirror of `MobileDeviceHeartbeatRequest`).
pub struct DeviceHeartbeatMeta {
    pub network_status: Option<String>,
    pub battery_level: Option<i64>,
    pub metadata_json: Option<String>,
}

/// Mirror of `mobile_core::dto::mobile::MobileDeviceResponse`.
pub struct DeviceInfo {
    pub device_id: String,
    pub user_id: String,
    pub is_active: bool,
    pub last_heartbeat_at: String,
}

impl From<mobile_core::dto::mobile::MobileDeviceResponse> for DeviceInfo {
    fn from(d: mobile_core::dto::mobile::MobileDeviceResponse) -> Self {
        Self {
            device_id: d.device_id,
            user_id: d.user_id,
            is_active: d.is_active,
            last_heartbeat_at: d.last_heartbeat_at,
        }
    }
}

fn parse_metadata(metadata_json: Option<&str>) -> anyhow::Result<std::collections::HashMap<String, serde_json::Value>> {
    match metadata_json {
        None => Ok(Default::default()),
        Some(raw) => Ok(serde_json::from_str(raw)?),
    }
}

/// Login and activate the session. On success Dart must persist
/// [`super::session::current_token_bundle`] into flutter_secure_storage.
pub async fn login(username: String, password: String) -> anyhow::Result<()> {
    let rt = runtime()?;
    mobile_core::api::auth::login(&rt.client, &username, &password).await?;
    Ok(())
}

/// Register this device. `platform` stays `"android"` (decision point D3).
pub async fn register_device(info: DeviceRegisterInfo) -> anyhow::Result<DeviceInfo> {
    let rt = runtime()?;
    let mut request = MobileDeviceRegisterRequest::new(info.device_id);
    if let Some(channel) = info.push_channel {
        request.push_channel = channel;
    }
    request.push_token = info.push_token;
    request.app_version = info.app_version;
    request.os_version = info.os_version;
    request.device_model = info.device_model;
    request.manufacturer = info.manufacturer;
    request.metadata = parse_metadata(info.metadata_json.as_deref())?;
    Ok(mobile_core::api::auth::register_device(&rt.client, &request)
        .await?
        .into())
}

/// Device heartbeat. Uses the runtime's operator-context id as the device id
/// (same ANDROID_ID the legacy app registers with).
pub async fn device_heartbeat(meta: DeviceHeartbeatMeta) -> anyhow::Result<DeviceInfo> {
    let rt = runtime()?;
    let device_id = rt.client.device_id().to_string();
    let request = MobileDeviceHeartbeatRequest {
        network_status: meta.network_status,
        battery_level: meta.battery_level,
        metadata: parse_metadata(meta.metadata_json.as_deref())?,
    };
    Ok(mobile_core::api::auth::device_heartbeat(&rt.client, &device_id, &request)
        .await?
        .into())
}

/// Auth keep-alive heartbeat.
pub async fn auth_heartbeat() -> anyhow::Result<()> {
    let rt = runtime()?;
    mobile_core::api::auth::auth_heartbeat(&rt.client).await?;
    Ok(())
}
