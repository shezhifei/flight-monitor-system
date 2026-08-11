use std::collections::{HashMap, HashSet};

use chrono::Utc;

use fms_domain::error::DomainError;
use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;
use fms_domain::models::user::User;

use crate::services::flowable_service::FlowableServiceError;

use super::service::WorkflowActor;
use super::templates::flatten_object_template_variables;
use super::types::*;
use super::utils::{insert_opt_datetime, insert_opt_string};

pub(super) fn build_flight_context_snapshot(
    flight: &crate::schemas::flight_schemas::FlightResponse,
) -> HashMap<String, serde_json::Value> {
    let mut snapshot = HashMap::new();
    insert_opt_string(
        &mut snapshot,
        "inbound_flight_no",
        flight.inbound_leg.as_ref().map(|leg| leg.flight_no.clone()),
    );
    insert_opt_string(
        &mut snapshot,
        "outbound_flight_no",
        flight.outbound_leg.as_ref().map(|leg| leg.flight_no.clone()),
    );
    insert_opt_string(&mut snapshot, "flight_id", flight.flight_id.clone());
    insert_opt_string(&mut snapshot, "flight_no", flight.flight_number.clone());
    insert_opt_string(&mut snapshot, "airline_code", flight.airline_code.clone());
    insert_opt_string(&mut snapshot, "registration", flight.registration.clone());
    insert_opt_string(&mut snapshot, "aircraft_type", flight.aircraft_type_detail.clone());
    insert_opt_string(&mut snapshot, "status", flight.status.clone());
    insert_opt_string(&mut snapshot, "stand", flight.stand.clone());
    insert_opt_string(&mut snapshot, "gate", flight.gate.clone());
    insert_opt_string(&mut snapshot, "terminal", flight.terminal.clone());
    insert_opt_string(&mut snapshot, "position", flight.position.clone());
    insert_opt_string(&mut snapshot, "baggage_carousel", flight.baggage_carousel.clone());
    insert_opt_datetime(&mut snapshot, "scheduled_departure", flight.scheduled_departure);
    insert_opt_datetime(&mut snapshot, "estimated_departure", flight.estimated_departure);
    insert_opt_datetime(&mut snapshot, "actual_departure", flight.actual_departure);
    insert_opt_datetime(&mut snapshot, "scheduled_arrival", flight.scheduled_arrival);
    insert_opt_datetime(&mut snapshot, "estimated_arrival", flight.estimated_arrival);
    insert_opt_datetime(&mut snapshot, "actual_arrival", flight.actual_arrival);
    snapshot
}

pub(super) fn build_flowable_start_variables(
    template_code: &str,
    case_id: &str,
    flight_id: &str,
    flight_context: &HashMap<String, serde_json::Value>,
    description: &str,
    extra_info: &HashMap<String, serde_json::Value>,
    case_type: &str,
    actor: &WorkflowActor,
    created_at: Option<chrono::DateTime<Utc>>,
) -> serde_json::Map<String, serde_json::Value> {
    let started_by = actor.started_by();
    let mut variables = serde_json::Map::new();
    variables.insert(
        "templateCode".to_string(),
        serde_json::Value::String(template_code.to_string()),
    );
    variables.insert("flightId".to_string(), serde_json::Value::String(flight_id.to_string()));
    variables.insert(
        "flight_id".to_string(),
        serde_json::Value::String(flight_id.to_string()),
    );
    variables.insert("caseId".to_string(), serde_json::Value::String(case_id.to_string()));
    variables.insert("case_id".to_string(), serde_json::Value::String(case_id.to_string()));
    variables.insert(
        "businessKey".to_string(),
        serde_json::Value::String(case_id.to_string()),
    );
    variables.insert(
        "business_key".to_string(),
        serde_json::Value::String(case_id.to_string()),
    );
    variables.insert("caseType".to_string(), serde_json::Value::String(case_type.to_string()));
    variables.insert(
        "case_type".to_string(),
        serde_json::Value::String(case_type.to_string()),
    );
    variables.insert(
        "flightContext".to_string(),
        serde_json::to_value(flight_context).unwrap_or_else(|_| serde_json::json!({})),
    );
    variables.insert(
        "description".to_string(),
        serde_json::Value::String(description.trim().to_string()),
    );
    variables.insert(
        "created_at".to_string(),
        created_at
            .map(|value| serde_json::Value::String(value.to_rfc3339()))
            .unwrap_or(serde_json::Value::Null),
    );
    variables.insert("startedBy".to_string(), serde_json::Value::String(started_by.clone()));
    variables.insert("started_by".to_string(), serde_json::Value::String(started_by));
    variables.insert("operator".to_string(), serde_json::Value::String(actor.operator()));
    variables.insert(
        "operator_user_id".to_string(),
        actor
            .user_id
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    variables.insert(
        "operator_username".to_string(),
        actor
            .username
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    variables.insert(
        "operator_name_snapshot".to_string(),
        serde_json::Value::String(actor.operator_name_snapshot()),
    );
    variables.insert(
        "operator_context_type".to_string(),
        actor
            .context_type
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    variables.insert(
        "operator_context_id".to_string(),
        actor
            .context_id
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    variables.insert(
        "extraInfo".to_string(),
        serde_json::to_value(extra_info).unwrap_or_else(|_| serde_json::json!({})),
    );
    flatten_object_template_variables(&flight_context.clone().into_iter().collect(), None, &mut variables);
    flatten_object_template_variables(&extra_info.clone().into_iter().collect(), None, &mut variables);
    flatten_object_template_variables(
        &flight_context.clone().into_iter().collect(),
        Some("flightContext"),
        &mut variables,
    );
    flatten_object_template_variables(
        &extra_info.clone().into_iter().collect(),
        Some("extraInfo"),
        &mut variables,
    );
    variables
}

pub(super) fn build_workflow_start_payload(
    business_key: &str,
    process_definition_id: Option<&str>,
    process_definition_key: &str,
    bpmn_source: &str,
    extra_info: &HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    HashMap::from([
        (
            "business_key".to_string(),
            serde_json::Value::String(business_key.to_string()),
        ),
        (
            "process_definition_id".to_string(),
            process_definition_id
                .map(|value| serde_json::Value::String(value.to_string()))
                .unwrap_or(serde_json::Value::Null),
        ),
        (
            "process_definition_key".to_string(),
            serde_json::Value::String(process_definition_key.to_string()),
        ),
        (
            "bpmn_source".to_string(),
            serde_json::Value::String(bpmn_source.to_string()),
        ),
        (
            "extra_info".to_string(),
            serde_json::to_value(extra_info).unwrap_or_else(|_| serde_json::json!({})),
        ),
    ])
}

pub(super) fn latest_process_definition(items: &[serde_json::Value]) -> Option<&serde_json::Value> {
    items
        .iter()
        .max_by_key(|item| item.get("version").and_then(serde_json::Value::as_i64).unwrap_or(0))
}

pub(super) fn normalize_process_instance(
    instance: serde_json::Value,
    _active_tasks: &[serde_json::Value],
    _variables: &serde_json::Map<String, serde_json::Value>,
    _wait_task: Option<&serde_json::Value>,
) -> serde_json::Value {
    instance
}

pub(super) fn normalize_variable_payload(payload: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match payload {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| {
                let serde_json::Value::Object(mut object) = item else {
                    return None;
                };
                let name = object
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)?;
                let value = object.remove("value").unwrap_or(serde_json::Value::Null);
                Some((name, value))
            })
            .collect(),
        serde_json::Value::Object(object) => object,
        _ => serde_json::Map::new(),
    }
}

pub(super) fn normalize_historic_variables(
    items: Vec<serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut variables = serde_json::Map::new();
    for item in items {
        let serde_json::Value::Object(mut object) = item else {
            continue;
        };
        let Some(name) = object
            .get("variableName")
            .and_then(serde_json::Value::as_str)
            .or_else(|| object.get("name").and_then(serde_json::Value::as_str))
            .map(str::to_string)
        else {
            continue;
        };
        let value = object.remove("value").unwrap_or(serde_json::Value::Null);
        variables.insert(name, value);
    }
    variables
}

pub(super) fn resolve_wait_task(
    active_tasks: &[serde_json::Value],
    run: &BusinessCaseWorkflowRun,
) -> Option<serde_json::Value> {
    if let Some(waiting_task_id) = run.waiting_task_id.as_deref() {
        if let Some(task) = active_tasks.iter().find(|task| {
            task.get("id")
                .and_then(serde_json::Value::as_str)
                .map(|id| id == waiting_task_id)
                .unwrap_or(false)
        }) {
            return Some(task.clone());
        }
    }
    locate_task_by_definition_key(active_tasks, "wait_receipts")
}

pub(super) fn locate_task_by_definition_key(
    tasks: &[serde_json::Value],
    task_definition_key: &str,
) -> Option<serde_json::Value> {
    tasks.iter().find_map(|task| {
        let matches_definition_key = task
            .get("taskDefinitionKey")
            .and_then(serde_json::Value::as_str)
            .map(|value| value == task_definition_key)
            .unwrap_or(false);
        let matches_id = task
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|value| value == task_definition_key)
            .unwrap_or(false);
        if matches_definition_key || matches_id {
            Some(task.clone())
        } else {
            None
        }
    })
}

pub(super) fn task_identifier(task: &serde_json::Value) -> Option<String> {
    task.get("id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

pub(super) fn derive_flowable_run_status(
    active_tasks: &[serde_json::Value],
    wait_task: &Option<serde_json::Value>,
    run: &BusinessCaseWorkflowRun,
    snapshot_receipt_group_id: Option<&str>,
) -> String {
    if run.completed_at.is_some() {
        return if run.outcome.as_deref() == Some("rejected") || run.status == "failed" {
            "failed".to_string()
        } else {
            "completed".to_string()
        };
    }
    if matches!(run.status.as_str(), "completed" | "failed" | "completing_case") {
        return run.status.clone();
    }
    if wait_task.is_some() {
        return "waiting_receipts".to_string();
    }
    if run.receipt_group_id.is_some()
        || snapshot_receipt_group_id.is_some()
        || matches!(run.status.as_str(), "notification_sent" | "waiting_receipts")
    {
        return "waiting_receipts".to_string();
    }
    if active_tasks.is_empty() {
        "active".to_string()
    } else {
        "running".to_string()
    }
}

pub(super) fn reconcile_run_with_snapshot(run: &mut BusinessCaseWorkflowRun, snapshot: &FlowableRunSnapshot) -> bool {
    let mut changed = false;
    if run.waiting_task_id != snapshot.wait_task_id {
        run.waiting_task_id = snapshot.wait_task_id.clone();
        changed = true;
    }
    if run.receipt_group_id.is_none() && snapshot.receipt_group_id.is_some() {
        run.receipt_group_id = snapshot.receipt_group_id.clone();
        changed = true;
    }
    let recipient_snapshot = snapshot
        .variables
        .get("recipientSnapshot")
        .and_then(json_array_to_vec_map_value);
    if run.recipient_snapshot.is_empty() {
        if let Some(recipient_snapshot) = recipient_snapshot {
            if !recipient_snapshot.is_empty() {
                run.recipient_snapshot = recipient_snapshot;
                changed = true;
            }
        }
    }
    if run.status != snapshot.status {
        run.status = snapshot.status.clone();
        changed = true;
    }
    changed
}

pub(super) fn select_dispatch_task(
    tasks: &[serde_json::Value],
    definition: &WorkflowRuntimeDefinition,
) -> Option<serde_json::Value> {
    tasks.iter().find_map(|task| {
        let task_key = task_definition_key_or_id(task)?;
        if definition.dispatch_tasks.contains_key(&task_key) {
            Some(task.clone())
        } else {
            None
        }
    })
}

pub(super) fn task_definition_key_or_id(task: &serde_json::Value) -> Option<String> {
    task.get("taskDefinitionKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| task_identifier(task))
}

pub(super) fn normalize_runtime_variables(payload: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let payload = match payload {
        serde_json::Value::Object(mut object) => {
            if matches!(object.get("data"), Some(serde_json::Value::Array(_))) {
                object.remove("data").unwrap_or(serde_json::Value::Array(Vec::new()))
            } else if object
                .values()
                .all(|value| !matches!(value, serde_json::Value::Object(_)))
            {
                return object
                    .into_iter()
                    .map(|(key, value)| (key, normalize_runtime_variable_value(value)))
                    .collect();
            } else {
                return serde_json::Map::new();
            }
        }
        other => other,
    };

    match payload {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| {
                let serde_json::Value::Object(mut object) = item else {
                    return None;
                };
                let name = object
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?
                    .to_string();
                let value = object.remove("value").unwrap_or(serde_json::Value::Null);
                Some((name, normalize_runtime_variable_value(value)))
            })
            .collect(),
        _ => serde_json::Map::new(),
    }
}

pub(super) fn normalize_runtime_variable_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.starts_with('[') || trimmed.starts_with('{') {
                serde_json::from_str(trimmed).unwrap_or(serde_json::Value::String(text))
            } else {
                serde_json::Value::String(text)
            }
        }
        other => other,
    }
}

pub(super) fn extract_flight_context(
    variables: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    variables
        .get("flightContext")
        .or_else(|| variables.get("flight_context"))
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn json_array_to_vec_map_value(
    value: &serde_json::Value,
) -> Option<Vec<HashMap<String, serde_json::Value>>> {
    match value {
        serde_json::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| match item {
                    serde_json::Value::Object(map) => Some(map.clone().into_iter().collect()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

pub(super) fn resolve_receipt_group_outcome(receipt_group: &serde_json::Value) -> Option<ReceiptWorkflowOutcome> {
    let summary = receipt_group.get("summary")?;
    let rejected_count = summary
        .get("rejected_count")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let pending_count = summary
        .get("pending_count")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let total_count = summary
        .get("total_count")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);

    if rejected_count > 0 {
        Some(ReceiptWorkflowOutcome::Rejected)
    } else if pending_count <= 0 && total_count > 0 {
        Some(ReceiptWorkflowOutcome::Confirmed)
    } else {
        None
    }
}

pub(super) fn derive_receipt_failed_reason(receipt_group: &serde_json::Value) -> Option<String> {
    let items = receipt_group.get("items")?.as_array()?;
    for item in items {
        let ack_status = item
            .get("ack_status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if ack_status != "rejected" {
            continue;
        }

        let note = item
            .get("ack_note")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let user_id = item
            .get("user_id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(note) = note {
            return Some(match user_id {
                Some(user_id) => format!("{user_id}: {note}"),
                None => note.to_string(),
            });
        }
        if let Some(user_id) = user_id {
            return Some(format!("{user_id} rejected notification"));
        }
    }

    Some("Notification rejected".to_string())
}

pub(super) fn mark_run_as_system_error(
    mut run: BusinessCaseWorkflowRun,
    failed_reason: &str,
) -> BusinessCaseWorkflowRun {
    run.status = "failed".to_string();
    run.outcome = Some("system_error".to_string());
    run.failed_reason = Some(failed_reason.to_string());
    run.completed_at = Some(Utc::now());
    run.updated_at = Utc::now();
    run
}

pub(super) fn matches_department(user: &User, department: &str) -> bool {
    user.department
        .as_deref()
        .map(|value| value.trim() == department.trim())
        .unwrap_or(false)
}

pub(super) fn matches_any_role(user: &User, roles: &[String]) -> bool {
    let expected = roles
        .iter()
        .map(|role| role.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    user.roles
        .iter()
        .any(|role| expected.contains(&role.name.trim().to_ascii_lowercase()))
}

pub(super) fn user_to_recipient_snapshot(user: User) -> HashMap<String, serde_json::Value> {
    let mut snapshot = HashMap::new();
    snapshot.insert("user_id".to_string(), serde_json::Value::String(user.id.clone()));
    snapshot.insert("username".to_string(), serde_json::Value::String(user.username.clone()));
    snapshot.insert(
        "display_name".to_string(),
        user.display_name
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    snapshot.insert(
        "department".to_string(),
        user.department
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    snapshot.insert(
        "roles".to_string(),
        serde_json::Value::Array(
            user.roles
                .into_iter()
                .map(|role| serde_json::Value::String(role.name))
                .collect(),
        ),
    );
    snapshot
}

pub(super) fn map_flowable_error(error: FlowableServiceError) -> DomainError {
    match error {
        FlowableServiceError::Validation(message)
        | FlowableServiceError::NotFound(message)
        | FlowableServiceError::Upstream(message) => DomainError::Internal(message),
    }
}

pub(super) fn require_linked_business_case(
    case_id: &str,
    business_case: Option<fms_domain::models::business_case::FlightBusinessCase>,
) -> Result<fms_domain::models::business_case::FlightBusinessCase, DomainError> {
    business_case.ok_or_else(|| DomainError::Internal(format!("Business case workflow linked case missing: {case_id}")))
}
