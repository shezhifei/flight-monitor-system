use std::collections::HashMap;

use chrono::{Duration as ChronoDuration, Utc};
use tracing::warn;

use fms_domain::error::DomainError;

use crate::schemas::dispatch_schemas::WorkflowDispatchCreateRequest;

use super::snapshots::{extract_flight_context, task_identifier};
use super::templates::{extract_optional_string, flatten_object_template_variables, render_template_from_map};
use super::types::*;

pub(super) fn build_dispatch_create_request(
    process_instance_id: &str,
    process_instance: &serde_json::Value,
    task: &serde_json::Value,
    variables: &serde_json::Map<String, serde_json::Value>,
    config: &WorkflowDispatchTaskConfig,
) -> Result<WorkflowDispatchCreateRequest, DomainError> {
    let flight_context = extract_flight_context(variables);
    let flight_id = extract_optional_string(variables, &["flightId", "flight_id"])
        .or_else(|| extract_optional_string(&flight_context, &["flight_id"]))
        .ok_or_else(|| {
            DomainError::BusinessRuleViolation("dispatch task requires flightId workflow variable".to_string())
        })?;
    if flight_context.is_empty() {
        return Err(DomainError::BusinessRuleViolation(
            "dispatch task requires flightContext workflow variable".to_string(),
        ));
    }

    let business_key = extract_optional_string(variables, &["businessKey", "business_key"]).or_else(|| {
        process_instance
            .get("businessKey")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    });
    let process_definition_key = extract_optional_string(variables, &["templateCode"]).or_else(|| {
        process_instance
            .get("processDefinitionKey")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    });
    let stand_id = extract_optional_string(&flight_context, &["stand", "stand_id"]);
    let task_id = task_identifier(task).ok_or_else(|| {
        DomainError::BusinessRuleViolation(format!(
            "dispatch task missing id for process={process_instance_id} node={}",
            config.node_id
        ))
    })?;
    let idempotency_key = format!("{process_instance_id}:{}:{flight_id}", config.node_id);

    let mut template_variables = variables.clone();
    template_variables.entry("businessKey".to_string()).or_insert_with(|| {
        business_key
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null)
    });
    template_variables.entry("business_key".to_string()).or_insert_with(|| {
        business_key
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null)
    });
    template_variables.insert(
        "processInstanceId".to_string(),
        serde_json::Value::String(process_instance_id.to_string()),
    );
    template_variables.insert(
        "process_instance_id".to_string(),
        serde_json::Value::String(process_instance_id.to_string()),
    );
    template_variables.insert("nodeId".to_string(), serde_json::Value::String(config.node_id.clone()));
    template_variables.insert(
        "nodeName".to_string(),
        serde_json::Value::String(config.node_name.clone()),
    );
    template_variables.insert("taskId".to_string(), serde_json::Value::String(task_id.clone()));
    if let Some(task_definition_key) = task
        .get("taskDefinitionKey")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        template_variables.insert(
            "taskDefinitionKey".to_string(),
            serde_json::Value::String(task_definition_key.to_string()),
        );
    }
    template_variables.insert(
        "flightContext".to_string(),
        serde_json::Value::Object(flight_context.clone()),
    );
    template_variables.insert("flightId".to_string(), serde_json::Value::String(flight_id.clone()));
    template_variables.insert("flight_id".to_string(), serde_json::Value::String(flight_id.clone()));
    flatten_object_template_variables(&flight_context, None, &mut template_variables);
    if let Some(extra_info) = variables.get("extraInfo").and_then(serde_json::Value::as_object) {
        flatten_object_template_variables(extra_info, Some("extraInfo"), &mut template_variables);
    }

    let description = config
        .description_template
        .as_deref()
        .map(|template| render_template_from_map(template, &template_variables))
        .filter(|value| !value.trim().is_empty());
    let assignment_deadline = (config.assignment_deadline_minutes > 0)
        .then(|| Utc::now() + ChronoDuration::minutes(i64::from(config.assignment_deadline_minutes)));

    let mut context = flight_context
        .clone()
        .into_iter()
        .collect::<HashMap<String, serde_json::Value>>();
    context.insert(
        "workflow_node_id".to_string(),
        serde_json::Value::String(config.node_id.clone()),
    );
    context.insert(
        "workflow_node_name".to_string(),
        serde_json::Value::String(config.node_name.clone()),
    );
    context.insert(
        "workflow_idempotency_key".to_string(),
        serde_json::Value::String(idempotency_key),
    );
    context.insert(
        "flight_context".to_string(),
        serde_json::Value::Object(flight_context.clone()),
    );
    context.insert(
        "business_key".to_string(),
        business_key
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    context.insert(
        "process_definition_key".to_string(),
        process_definition_key
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    context.insert(
        "priority".to_string(),
        serde_json::Value::String(config.priority.clone()),
    );
    context.insert(
        "description".to_string(),
        description
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    context.insert(
        "target_department".to_string(),
        serde_json::Value::String(config.target_department.clone()),
    );
    context.insert(
        "target_job_title".to_string(),
        config
            .target_job_title
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    context.insert(
        "required_people".to_string(),
        serde_json::Value::Number(config.required_people.into()),
    );
    context.insert("auto_completed_process_task".to_string(), serde_json::Value::Bool(true));

    Ok(WorkflowDispatchCreateRequest {
        process_instance_id: process_instance_id.to_string(),
        process_task_id: task_id,
        process_definition_key,
        business_key,
        flight_id,
        task_type: config.task_type.clone(),
        stand_id,
        planned_start_time: None,
        planned_end_time: None,
        assignment_deadline,
        target_department: config.target_department.clone(),
        target_job_title: config.target_job_title.clone(),
        required_people: config.required_people,
        priority: config.priority.clone(),
        description,
        context: context.into_iter().collect(),
    })
}

pub(super) fn dispatch_order_status_value(status: fms_domain::models::dispatch::DispatchOrderStatus) -> String {
    match status {
        fms_domain::models::dispatch::DispatchOrderStatus::Pending => "pending",
        fms_domain::models::dispatch::DispatchOrderStatus::Assigned => "assigned",
        fms_domain::models::dispatch::DispatchOrderStatus::InProgress => "in_progress",
        fms_domain::models::dispatch::DispatchOrderStatus::Completed => "completed",
        fms_domain::models::dispatch::DispatchOrderStatus::Cancelled => "cancelled",
    }
    .to_string()
}

pub(super) fn merge_dispatch_order_refs(
    existing: Option<&serde_json::Value>,
    config: &WorkflowDispatchTaskConfig,
    task_id: &str,
    order_id: &str,
    status: &str,
) -> Vec<serde_json::Value> {
    let mut refs = existing
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.is_object())
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let updated = serde_json::json!({
        "nodeId": config.node_id,
        "taskId": task_id,
        "dispatchOrderId": order_id,
        "status": status,
        "createdAt": Utc::now().to_rfc3339(),
    });

    let mut replaced = false;
    for item in &mut refs {
        if item.get("nodeId").and_then(serde_json::Value::as_str).map(str::trim) == Some(config.node_id.as_str()) {
            *item = updated.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        refs.push(updated);
    }
    refs
}

pub(super) fn handle_dispatch_task_continuation_error(
    message: String,
    raise_on_error: bool,
) -> Result<(), DomainError> {
    if raise_on_error {
        Err(DomainError::BusinessRuleViolation(message))
    } else {
        warn!(%message, "dispatch task continuation skipped");
        Ok(())
    }
}
