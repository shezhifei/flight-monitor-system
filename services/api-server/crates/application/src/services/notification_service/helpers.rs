use std::collections::HashMap;

use chrono::{Timelike, Utc};
use sha2::{Digest, Sha256};

use fms_domain::error::DomainError;
use fms_domain::models::notification::{Notification, NotificationPreference};

use super::schemas::NotificationResponse;

pub(crate) fn to_response(n: &Notification) -> NotificationResponse {
    NotificationResponse {
        notification_id: n.notification_id.clone(),
        user_id: n.user_id.clone(),
        title: n.title.clone(),
        body: n.body.clone(),
        category: n.category.clone(),
        severity: n.severity.clone(),
        is_read: n.is_read,
        read_status: if n.is_read { "read".into() } else { "unread".into() },
        delivery_status: n.delivery_status.clone(),
        delivered_at: n.delivered_at,
        origin_type: n.origin_type.clone(),
        origin_label: origin_label(Some(&n.origin_type)),
        receipt_required: n.receipt_required,
        receipt_group_id: n.receipt_group_id.clone(),
        ack_status: n.ack_status.clone(),
        ack_at: n.ack_at,
        ack_note: n.ack_note.clone(),
        related_entity_type: n.related_entity_type.clone(),
        related_entity_id: n.related_entity_id.clone(),
        sender_user_id: n.sender_user_id.clone(),
        sender_username: n.sender_username_snapshot.clone(),
        created_at: n.created_at,
        read_at: n.read_at,
    }
}

pub(crate) fn stable_notification_id(seed: &str, user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"notification:v1:");
    hasher.update(seed.as_bytes());
    hasher.update(b"\0");
    hasher.update(user_id.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    ulid::Ulid::from_bytes(bytes).to_string()
}

pub(crate) fn receipt_to_value(n: &Notification) -> serde_json::Value {
    let recipient_username = n
        .recipient_username_snapshot
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            n.recipient_display_name_snapshot
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("未知账号");
    serde_json::json!({
        "receipt_id": n.notification_id,
        "notification_id": n.notification_id,
        "user_id": n.user_id,
        "recipient_user_id": n.user_id,
        "recipient_username": recipient_username,
        "recipient_display_name": n.recipient_display_name_snapshot,
        "recipient_department": n.recipient_department_snapshot,
        "recipient_job_title": n.recipient_job_title_snapshot,
        "title": n.title,
        "severity": n.severity,
        "origin_type": n.origin_type,
        "origin_label": origin_label(Some(&n.origin_type)),
        "receipt_group_id": n.receipt_group_id,
        "delivery_status": n.delivery_status,
        "delivered_at": n.delivered_at,
        "read_status": if n.is_read { "read" } else { "unread" },
        "read_at": n.read_at,
        "ack_status": n.ack_status,
        "ack_at": n.ack_at,
        "ack_note": n.ack_note,
        "sender_user_id": n.sender_user_id,
        "sender_username": n.sender_username_snapshot,
        "updated_at": n.ack_at.or(n.read_at).or(n.delivered_at).unwrap_or(n.created_at),
    })
}

pub(crate) fn receipt_group_summary_to_value(row: &serde_json::Value) -> serde_json::Value {
    let empty_map = serde_json::Map::new();
    let map = row.as_object().unwrap_or(&empty_map);
    let remind_after_at = build_remind_after_at(map.get("created_at"));
    let pending_count = map.get("pending_count").and_then(|value| value.as_i64()).unwrap_or(0);
    let normalized_origin_type = normalize_origin_type(map.get("origin_type").and_then(|value| value.as_str()));
    let info_severity = serde_json::Value::String("info".to_string());
    let zero = serde_json::Value::from(0);
    serde_json::json!({
        "receipt_group_id": get_val(map, "receipt_group_id"),
        "title": get_val(map, "title"),
        "severity": map.get("severity").unwrap_or(&info_severity),
        "origin_label": origin_label(Some(&normalized_origin_type)),
        "origin_type": normalized_origin_type,
        "flight_id": get_val(map, "flight_id"),
        "dispatch_order_id": get_val(map, "dispatch_order_id"),
        "group_id": get_val(map, "group_id"),
        "created_at": get_val(map, "created_at"),
        "latest_updated_at": get_val(map, "latest_updated_at"),
        "remind_after_at": remind_after_at,
        "is_overdue": is_overdue(pending_count, remind_after_at.as_ref()),
        "total_count": map.get("total_count").unwrap_or(&zero),
        "pending_count": map.get("pending_count").unwrap_or(&zero),
        "acknowledged_count": map.get("acknowledged_count").unwrap_or(&zero),
        "rejected_count": map.get("rejected_count").unwrap_or(&zero),
    })
}

pub(crate) fn build_remind_after_at(value: Option<&serde_json::Value>) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = value?;
    serde_json::from_value::<Option<chrono::DateTime<chrono::Utc>>>(value.clone())
        .ok()
        .flatten()
        .map(|created_at| created_at + chrono::Duration::minutes(2))
}

pub(crate) fn is_overdue(pending_count: i64, remind_after_at: Option<&chrono::DateTime<chrono::Utc>>) -> bool {
    pending_count > 0 && remind_after_at.is_some_and(|value| *value <= Utc::now())
}

pub(crate) fn origin_label(origin_type: Option<&str>) -> String {
    if origin_type.unwrap_or("manual").eq_ignore_ascii_case("workflow") {
        "流程".to_string()
    } else {
        "人工".to_string()
    }
}

pub(crate) fn normalize_origin_type(origin_type: Option<&str>) -> String {
    if origin_type.unwrap_or("manual").trim().eq_ignore_ascii_case("workflow") {
        "workflow".to_string()
    } else {
        "manual".to_string()
    }
}

pub(crate) fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub(crate) fn normalize_category_overrides(value: HashMap<String, bool>) -> HashMap<String, bool> {
    value
        .into_iter()
        .map(|(key, enabled)| (key.trim().to_ascii_lowercase(), enabled))
        .filter(|(key, _)| !key.is_empty())
        .collect()
}

pub(crate) fn normalize_time_text(value: Option<String>) -> Result<Option<String>, DomainError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    chrono::NaiveTime::parse_from_str(normalized, "%H:%M")
        .map(|time| Some(time.format("%H:%M").to_string()))
        .map_err(|_| DomainError::ValidationError("time fields must be HH:MM format".into()))
}

pub(crate) fn to_minutes(value: &str) -> Result<u32, DomainError> {
    let (hour, minute) = value
        .split_once(':')
        .ok_or_else(|| DomainError::ValidationError("time fields must be HH:MM format".into()))?;
    let hour = hour
        .parse::<u32>()
        .map_err(|_| DomainError::ValidationError("time fields must be HH:MM format".into()))?;
    let minute = minute
        .parse::<u32>()
        .map_err(|_| DomainError::ValidationError("time fields must be HH:MM format".into()))?;
    Ok(hour * 60 + minute)
}

pub(crate) fn is_muted_now(preference: &NotificationPreference, now: chrono::DateTime<chrono::Utc>) -> bool {
    let (Some(mute_start), Some(mute_end)) = (preference.mute_start.as_deref(), preference.mute_end.as_deref()) else {
        return false;
    };

    let Ok(start) = to_minutes(mute_start) else {
        return false;
    };
    let Ok(end) = to_minutes(mute_end) else {
        return false;
    };

    if start == end {
        return false;
    }

    let now_minutes = now.hour() * 60 + now.minute();
    if start < end {
        start <= now_minutes && now_minutes < end
    } else {
        now_minutes >= start || now_minutes < end
    }
}

pub(crate) fn normalize_note(note: Option<&str>) -> Option<String> {
    note.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn normalize_ack_action(action: &str) -> Result<&'static str, DomainError> {
    match action.trim().to_ascii_lowercase().as_str() {
        "acknowledged" => Ok("acknowledged"),
        "rejected" => Ok("rejected"),
        _ => Err(DomainError::ValidationError(
            "action must be acknowledged or rejected".into(),
        )),
    }
}

pub(crate) fn default_preference(user_id: &str) -> NotificationPreference {
    NotificationPreference {
        user_id: user_id.to_string(),
        in_app_enabled: true,
        external_enabled: false,
        external_channel: "none".to_string(),
        mute_start: None,
        mute_end: None,
        critical_override: true,
        category_overrides: HashMap::new(),
        updated_at: Utc::now(),
    }
}

pub(crate) static NULL_VALUE: serde_json::Value = serde_json::Value::Null;

pub(crate) fn get_val<'a>(map: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> &'a serde_json::Value {
    map.get(key).unwrap_or(&NULL_VALUE)
}
