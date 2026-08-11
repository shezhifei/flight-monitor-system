use std::collections::HashMap;

use sha2::{Digest, Sha256};

use super::types::*;

pub(super) fn parse_workflow_batch_policy(case_properties: &serde_json::Value) -> WorkflowBatchPolicy {
    let Some(policy) = case_properties.get("workflow_policy") else {
        return WorkflowBatchPolicy::default();
    };
    let notification_enabled = policy
        .get("batch_notification_enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let receipt_mode = match policy
        .get("batch_receipt_mode")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
    {
        Some("shared_group") => WorkflowBatchReceiptMode::SharedGroup,
        _ => WorkflowBatchReceiptMode::PerCase,
    };
    WorkflowBatchPolicy {
        notification_enabled,
        receipt_mode,
    }
}

pub(super) fn compute_recipient_set_hash(recipients: &[HashMap<String, serde_json::Value>]) -> String {
    let mut user_ids: Vec<String> = recipients
        .iter()
        .filter_map(|item| {
            item.get("user_id")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect();
    user_ids.sort();
    user_ids.join(",")
}

pub(super) fn derive_batch_notification_idempotency_context(
    batch_id: &str,
    template_code: &str,
    case_type: &str,
    notification_task_id: &str,
    case_ids: &[String],
    recipient_user_ids: &[String],
    receipt_required: bool,
    severity: &str,
) -> WorkflowBatchNotificationIdempotencyContext {
    let mut sorted_case_ids = normalize_sorted_values(case_ids);
    let mut sorted_recipient_user_ids = normalize_sorted_values(recipient_user_ids);
    let sorted_case_ids = stable_join(&mut sorted_case_ids);
    let sorted_recipient_user_ids = stable_join(&mut sorted_recipient_user_ids);
    let key = stable_idempotency_key(&[
        ("scope", "business_case_workflow_batch_group"),
        ("batch_id", batch_id.trim()),
        ("template_code", template_code.trim()),
        ("case_type", case_type.trim()),
        ("notification_task_id", notification_task_id.trim()),
        ("case_ids", &sorted_case_ids),
        ("recipient_user_ids", &sorted_recipient_user_ids),
        ("receipt_required", if receipt_required { "true" } else { "false" }),
        ("severity", severity.trim()),
    ]);
    idempotency_context_from_key(&key, receipt_required)
}

pub(super) fn derive_per_case_batch_notification_idempotency_context(
    batch_id: &str,
    case_id: &str,
    template_code: &str,
    notification_task_id: &str,
    receipt_required: bool,
) -> WorkflowBatchNotificationIdempotencyContext {
    let key = stable_idempotency_key(&[
        ("scope", "business_case_workflow_batch_case"),
        ("batch_id", batch_id.trim()),
        ("case_id", case_id.trim()),
        ("template_code", template_code.trim()),
        ("notification_task_id", notification_task_id.trim()),
    ]);
    idempotency_context_from_key(&key, receipt_required)
}

pub(super) fn idempotency_context_from_key(
    key: &str,
    receipt_required: bool,
) -> WorkflowBatchNotificationIdempotencyContext {
    let digest = sha256_digest(key.as_bytes());
    WorkflowBatchNotificationIdempotencyContext {
        receipt_group_id_override: receipt_required.then(|| crockford_base32_128(&digest[..16])),
        notification_id_seed: format!("workflow_batch:{}", hex::encode(digest)),
    }
}

pub(super) fn stable_idempotency_key(parts: &[(&str, &str)]) -> String {
    let mut key = String::new();
    for (name, value) in parts {
        key.push_str(&name.len().to_string());
        key.push(':');
        key.push_str(name);
        key.push('=');
        key.push_str(&value.len().to_string());
        key.push(':');
        key.push_str(value);
        key.push(';');
    }
    key
}

pub(super) fn normalize_sorted_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

pub(super) fn stable_join(values: &mut [String]) -> String {
    values.sort();
    values.join("\n")
}

pub(super) fn sha256_digest(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    output
}

pub(super) fn crockford_base32_128(bytes: &[u8]) -> String {
    debug_assert_eq!(bytes.len(), 16);
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut encoded = String::with_capacity(26);
    for char_index in 0..26 {
        let mut value = 0_u8;
        for bit_offset in 0..5 {
            let global_bit = char_index * 5 + bit_offset;
            value <<= 1;
            if global_bit < 2 {
                continue;
            }
            let data_bit = global_bit - 2;
            let byte_index = data_bit / 8;
            let bit_index = 7 - (data_bit % 8);
            value |= (bytes[byte_index] >> bit_index) & 1;
        }
        encoded.push(ALPHABET[value as usize] as char);
    }
    encoded
}

pub(super) fn build_batch_notification_title(case_type_name: &str, count: usize) -> String {
    format!("{} 个{}", count, case_type_name)
}

pub(super) fn build_batch_notification_body(items: &[WorkflowBatchPlanItem]) -> String {
    items
        .iter()
        .map(|item| {
            let flight_no = item.business_case.flight_no.trim();
            let extra = item
                .extra_info
                .get("extra_info")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if extra.is_empty() {
                flight_no.to_string()
            } else {
                format!("{} {}", flight_no, extra)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
