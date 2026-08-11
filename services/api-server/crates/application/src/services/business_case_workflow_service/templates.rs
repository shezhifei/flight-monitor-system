use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;

pub(super) fn flatten_object_template_variables(
    source: &serde_json::Map<String, serde_json::Value>,
    prefix: Option<&str>,
    target: &mut serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in source {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            continue;
        }
        let target_key = match prefix {
            Some(prefix) => format!("{prefix}.{normalized_key}"),
            None => normalized_key.to_string(),
        };
        target.insert(target_key, value.clone());
        if prefix.is_some() {
            target
                .entry(normalized_key.to_string())
                .or_insert_with(|| value.clone());
        }
        if let serde_json::Value::Object(map) = value {
            flatten_object_template_variables(map, Some(normalized_key), target);
            if let Some(prefix) = prefix {
                flatten_object_template_variables(map, Some(&format!("{prefix}.{normalized_key}")), target);
            }
        }
    }
}

pub(super) fn render_template_from_map(
    template: &str,
    variables: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let variable_map = variables
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<HashMap<_, _>>();
    render_template(template, &variable_map)
}

pub(super) fn build_template_variables(
    case_id: &str,
    flight_id: &str,
    flight_context: &HashMap<String, serde_json::Value>,
    extra_info: &HashMap<String, serde_json::Value>,
    description: &str,
    recipients: &[HashMap<String, serde_json::Value>],
) -> HashMap<String, serde_json::Value> {
    let mut variables = HashMap::from([
        ("caseId".to_string(), serde_json::Value::String(case_id.to_string())),
        ("case_id".to_string(), serde_json::Value::String(case_id.to_string())),
        ("flightId".to_string(), serde_json::Value::String(flight_id.to_string())),
        (
            "flight_id".to_string(),
            serde_json::Value::String(flight_id.to_string()),
        ),
        (
            "description".to_string(),
            serde_json::Value::String(description.trim().to_string()),
        ),
        (
            "flightContext".to_string(),
            serde_json::to_value(flight_context).unwrap_or_else(|_| serde_json::json!({})),
        ),
        (
            "extraInfo".to_string(),
            serde_json::to_value(extra_info).unwrap_or_else(|_| serde_json::json!({})),
        ),
        (
            "recipientSnapshot".to_string(),
            serde_json::to_value(recipients).unwrap_or_else(|_| serde_json::json!([])),
        ),
    ]);
    flatten_template_variables(flight_context, None, &mut variables);
    flatten_template_variables(extra_info, None, &mut variables);
    flatten_template_variables(flight_context, Some("flightContext"), &mut variables);
    flatten_template_variables(extra_info, Some("extraInfo"), &mut variables);
    variables
}

pub(super) fn build_runtime_variables(
    run: &BusinessCaseWorkflowRun,
    receipt_group: &serde_json::Value,
    outcome: &str,
    failed_reason: Option<&str>,
) -> HashMap<String, serde_json::Value> {
    let extra_info = run
        .start_payload
        .get("extra_info")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect::<HashMap<_, _>>();
    let mut variables = HashMap::from([
        (
            "templateCode".to_string(),
            serde_json::Value::String(run.template_code.clone()),
        ),
        ("caseId".to_string(), serde_json::Value::String(run.case_id.clone())),
        ("flightId".to_string(), serde_json::Value::String(run.flight_id.clone())),
        (
            "workflowOutcome".to_string(),
            serde_json::Value::String(outcome.to_string()),
        ),
        (
            "flightContext".to_string(),
            serde_json::to_value(&run.flight_context_snapshot).unwrap_or_else(|_| serde_json::json!({})),
        ),
        (
            "extraInfo".to_string(),
            serde_json::to_value(&extra_info).unwrap_or_else(|_| serde_json::json!({})),
        ),
        (
            "recipientSnapshot".to_string(),
            serde_json::to_value(&run.recipient_snapshot).unwrap_or_else(|_| serde_json::json!([])),
        ),
        (
            "receiptGroupId".to_string(),
            run.receipt_group_id
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        ),
        (
            "receiptSummary".to_string(),
            receipt_group
                .get("summary")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        ),
        (
            "failedReason".to_string(),
            failed_reason
                .map(|value| serde_json::Value::String(value.to_string()))
                .unwrap_or(serde_json::Value::Null),
        ),
    ]);
    flatten_template_variables(&run.flight_context_snapshot, Some("flightContext"), &mut variables);
    flatten_template_variables(&extra_info, Some("extraInfo"), &mut variables);
    variables
}

pub(super) fn build_wait_receipt_completion_variables(
    run: &BusinessCaseWorkflowRun,
    outcome: &str,
    failed_reason: Option<&str>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut variables = serde_json::Map::new();
    variables.insert(
        "workflowOutcome".to_string(),
        serde_json::Value::String(outcome.to_string()),
    );
    if let Some(receipt_group_id) = run.receipt_group_id.as_deref() {
        variables.insert(
            "receiptGroupId".to_string(),
            serde_json::Value::String(receipt_group_id.to_string()),
        );
    }
    if let Some(reason) = failed_reason.filter(|value| !value.trim().is_empty()) {
        variables.insert(
            "failedReason".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
    }
    variables
}

pub(super) fn flatten_template_variables(
    source: &HashMap<String, serde_json::Value>,
    prefix: Option<&str>,
    target: &mut HashMap<String, serde_json::Value>,
) {
    for (key, value) in source {
        let normalized_key = key.trim();
        if normalized_key.is_empty() {
            continue;
        }
        let target_key = match prefix {
            Some(prefix) => format!("{prefix}.{normalized_key}"),
            None => normalized_key.to_string(),
        };
        target.insert(target_key, value.clone());
        if prefix.is_some() {
            target
                .entry(normalized_key.to_string())
                .or_insert_with(|| value.clone());
        }
        if let serde_json::Value::Object(map) = value {
            let nested = map.clone().into_iter().collect::<HashMap<_, _>>();
            flatten_template_variables(&nested, Some(normalized_key), target);
            if let Some(prefix) = prefix {
                flatten_template_variables(&nested, Some(&format!("{prefix}.{normalized_key}")), target);
            }
        }
    }
}

pub(super) fn render_template(template: &str, variables: &HashMap<String, serde_json::Value>) -> String {
    let template = template.trim();
    if template.is_empty() {
        return String::new();
    }
    placeholder_regex()
        .replace_all(template, |captures: &regex::Captures<'_>| {
            let key = captures.get(1).map(|value| value.as_str()).unwrap_or_default().trim();
            lookup_template_value(variables, key)
                .map(json_value_to_template_string)
                .unwrap_or_default()
        })
        .into_owned()
}

pub(super) fn build_notification_body(
    body_template: &str,
    template_variables: &HashMap<String, serde_json::Value>,
    append_extra_info: bool,
    extra_info: &HashMap<String, serde_json::Value>,
) -> String {
    let base_body = render_template(body_template, template_variables).trim().to_string();
    if !append_extra_info {
        return base_body;
    }

    let Some(extra_info_text) = extract_optional_string_from_hashmap(extra_info, &["extra_info", "summary"]) else {
        return base_body;
    };
    if base_body.contains(&extra_info_text) {
        return base_body;
    }
    if base_body.is_empty() {
        return format!("额外信息：{extra_info_text}");
    }
    format!("{base_body}\n额外信息：{extra_info_text}")
}

pub(super) fn placeholder_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\$\{([A-Za-z0-9_.]+)\}").expect("valid template regex"))
}

pub(super) fn lookup_template_value<'a>(
    variables: &'a HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<&'a serde_json::Value> {
    if let Some(value) = variables.get(key) {
        return Some(value);
    }
    let mut current = variables.get(key.split('.').next().unwrap_or_default())?;
    for part in key.split('.').skip(1) {
        current = current.get(part)?;
    }
    Some(current)
}

pub(super) fn json_value_to_template_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

pub(super) fn extract_optional_string(
    values: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        values
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(super) fn extract_optional_string_from_hashmap(
    values: &HashMap<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        values
            .get(*key)
            .map(json_value_to_template_string)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn normalize_workflow_extra_info(
    source: &HashMap<String, serde_json::Value>,
    description: &str,
    flight_context: &HashMap<String, serde_json::Value>,
    fallback_gate: Option<&str>,
    fallback_stand: Option<&str>,
) -> HashMap<String, serde_json::Value> {
    let mut extra_info = source.clone();

    let gate = extract_optional_string_from_hashmap(&extra_info, &["gate", "gate_no"])
        .or_else(|| {
            fallback_gate
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| extract_optional_string_from_hashmap(flight_context, &["gate"]));

    let stand = extract_optional_string_from_hashmap(&extra_info, &["stand", "stand_no"])
        .or_else(|| {
            fallback_stand
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| extract_optional_string_from_hashmap(flight_context, &["stand"]));

    let trigger_reason = extract_optional_string_from_hashmap(&extra_info, &["trigger_reason", "reason"]);
    let extra_info_text = extract_optional_string_from_hashmap(&extra_info, &["extra_info", "summary"]).or_else(|| {
        let normalized = description.trim();
        (!normalized.is_empty()).then(|| normalized.to_string())
    });

    if let Some(gate) = gate {
        extra_info.insert("gate".to_string(), serde_json::Value::String(gate.clone()));
        extra_info
            .entry("gate_no".to_string())
            .or_insert_with(|| serde_json::Value::String(gate));
    }
    if let Some(stand) = stand {
        extra_info.insert("stand".to_string(), serde_json::Value::String(stand.clone()));
        extra_info
            .entry("stand_no".to_string())
            .or_insert_with(|| serde_json::Value::String(stand));
    }
    if let Some(trigger_reason) = trigger_reason {
        extra_info.insert("trigger_reason".to_string(), serde_json::Value::String(trigger_reason));
    }
    if let Some(extra_info_text) = extra_info_text {
        extra_info.insert(
            "extra_info".to_string(),
            serde_json::Value::String(extra_info_text.clone()),
        );
        extra_info
            .entry("summary".to_string())
            .or_insert_with(|| serde_json::Value::String(extra_info_text));
    }

    extra_info
}
