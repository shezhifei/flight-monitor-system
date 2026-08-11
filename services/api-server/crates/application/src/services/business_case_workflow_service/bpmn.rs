use std::collections::HashMap;

use fms_domain::error::DomainError;

use super::helpers::{
    normalize_notification_severity, parse_bool_attr, parse_int_attr, required_attr, split_csv,
    WorkflowBusinessCaseAction, WorkflowDispatchTaskConfig, WorkflowNotificationTarget,
    WorkflowRecipientResolverConfig, WorkflowRuntimeDefinition,
};

pub(super) fn parse_bpmn_runtime_definition(bpmn_xml: &str) -> Result<WorkflowRuntimeDefinition, DomainError> {
    let document = roxmltree::Document::parse(bpmn_xml)
        .map_err(|error| DomainError::Internal(format!("failed to parse BPMN XML: {error}")))?;
    let process = document
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "process");
    let Some(process) = process else {
        return Err(DomainError::BusinessRuleViolation(
            "BPMN missing process definition".to_string(),
        ));
    };

    let mut notification_task_id = None;
    let mut case_type = None;
    let mut notification_title = None;
    let mut notification_body = None;
    let mut notification_severity = "warning".to_string();
    let mut append_extra_info = false;
    let mut notification_targets = Vec::new();
    let mut recipient_resolver = WorkflowRecipientResolverConfig {
        source: "department_roles".to_string(),
        empty_policy: "fail".to_string(),
        deduplicate: true,
    };
    let mut receipt_required = true;
    let mut completion_policy = "all_notified_acknowledged".to_string();
    let mut reject_policy = "fail_on_any_reject".to_string();
    let mut wait_task_id = None;
    let mut success_action = None;
    let mut failure_action = None;
    let mut dispatch_tasks = HashMap::new();

    if let Some(extension_elements) = process
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "extensionElements")
    {
        if let Some(template) = extension_elements
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "workflowTemplate")
        {
            case_type = template
                .attribute("caseType")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
    }

    for task in process
        .children()
        .filter(|node| node.is_element() && node.tag_name().name() == "userTask")
    {
        let task_id = task.attribute("id").unwrap_or_default().trim().to_string();
        if task_id.is_empty() {
            continue;
        }
        if task_id == "wait_receipts" {
            wait_task_id = Some(task_id.clone());
        }

        let Some(extension_elements) = task
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "extensionElements")
        else {
            continue;
        };

        if let Some(notification_rule) = extension_elements
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "notificationRule")
        {
            notification_task_id = Some(task_id.clone());
            notification_title = notification_rule.attribute("title").map(str::to_string);
            notification_body = notification_rule.attribute("bodyTemplate").map(str::to_string);
            if let Some(value) = notification_rule
                .attribute("severity")
                .or_else(|| notification_rule.attribute("notificationSeverity"))
            {
                notification_severity = normalize_notification_severity(value);
            }
            receipt_required = parse_bool_attr(notification_rule.attribute("receiptRequired"), true);
            append_extra_info = parse_bool_attr(notification_rule.attribute("appendExtraInfo"), false);
            for target in notification_rule
                .descendants()
                .filter(|node| node.is_element() && node.tag_name().name() == "target")
            {
                let department = target.attribute("department").unwrap_or_default().trim().to_string();
                if department.is_empty() {
                    continue;
                }
                notification_targets.push(WorkflowNotificationTarget {
                    department,
                    roles: split_csv(target.attribute("roles")),
                });
            }
            if notification_targets.is_empty() {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "Notification node {task_id} missing target definitions"
                )));
            }
        }

        if let Some(receipt_rule) = extension_elements
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "receiptRule")
        {
            if let Some(value) = receipt_rule.attribute("completionPolicy") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    completion_policy = trimmed.to_string();
                }
            }
            if let Some(value) = receipt_rule.attribute("rejectPolicy") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    reject_policy = trimmed.to_string();
                }
            }
        }

        if let Some(resolver) = extension_elements
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "recipientResolver")
        {
            if let Some(value) = resolver.attribute("source") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    recipient_resolver.source = trimmed.to_string();
                }
            }
            if let Some(value) = resolver.attribute("emptyPolicy") {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    recipient_resolver.empty_policy = trimmed.to_string();
                }
            }
            recipient_resolver.deduplicate = parse_bool_attr(resolver.attribute("deduplicate"), true);
        }

        if let Some(action_rule) = extension_elements
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "businessCaseAction")
        {
            let action_name = action_rule.attribute("action").unwrap_or_default().trim();
            let target_status = action_rule
                .attribute("targetStatus")
                .unwrap_or_default()
                .trim()
                .to_string();
            if target_status.is_empty() {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "businessCaseAction on task {task_id} missing targetStatus"
                )));
            }
            let action = WorkflowBusinessCaseAction {
                node_id: task_id.clone(),
                action: action_name.to_string(),
                target_status,
                reason_template: action_rule
                    .attribute("reasonTemplate")
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
                write_finished_at: parse_bool_attr(action_rule.attribute("writeFinishedAt"), true),
                require_case_id: parse_bool_attr(action_rule.attribute("requireCaseId"), true),
            };
            match action_name {
                "complete_case" => success_action = Some(action),
                "fail_case" => failure_action = Some(action),
                _ if !action_name.is_empty() => {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "Unsupported businessCaseAction on task {task_id}: {action_name}"
                    )));
                }
                _ => {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "businessCaseAction on task {task_id} missing action"
                    )));
                }
            }
        }

        if let Some(dispatch_rule) = extension_elements
            .children()
            .find(|node| node.is_element() && node.tag_name().name() == "dispatchTask")
        {
            let task_type = required_attr(&dispatch_rule, "taskType", "fm:dispatchTask")?;
            let target_department = required_attr(&dispatch_rule, "targetDepartment", "fm:dispatchTask")?;
            dispatch_tasks.insert(
                task_id.clone(),
                WorkflowDispatchTaskConfig {
                    node_id: task_id.clone(),
                    node_name: task
                        .attribute("name")
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or(task_id.as_str())
                        .to_string(),
                    task_type,
                    target_department,
                    target_job_title: dispatch_rule
                        .attribute("targetJobTitle")
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned),
                    required_people: parse_int_attr(
                        dispatch_rule.attribute("requiredPeople"),
                        1,
                        1,
                        Some(20),
                        &format!("fm:dispatchTask[{task_id}].requiredPeople"),
                    )?,
                    priority: dispatch_rule
                        .attribute("priority")
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("normal")
                        .to_string(),
                    description_template: dispatch_rule
                        .attribute("descriptionTemplate")
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned),
                    assignment_deadline_minutes: parse_int_attr(
                        dispatch_rule.attribute("assignmentDeadlineMinutes"),
                        30,
                        1,
                        None,
                        &format!("fm:dispatchTask[{task_id}].assignmentDeadlineMinutes"),
                    )?,
                },
            );
        }
    }

    let Some(notification_task_id) = notification_task_id else {
        return Err(DomainError::BusinessRuleViolation(
            "BPMN missing notification node with fm:notificationRule".to_string(),
        ));
    };
    let wait_task_id = wait_task_id
        .ok_or_else(|| DomainError::BusinessRuleViolation("BPMN missing wait_receipts userTask".to_string()))?;
    let success_action = success_action.ok_or_else(|| {
        DomainError::BusinessRuleViolation("BPMN missing success business case action node".to_string())
    })?;
    let failure_action = failure_action.ok_or_else(|| {
        DomainError::BusinessRuleViolation("BPMN missing failure business case action node".to_string())
    })?;

    Ok(WorkflowRuntimeDefinition {
        case_type: case_type.unwrap_or_else(|| "generic_case".to_string()),
        notification_task_id,
        wait_task_id,
        notification_title: notification_title.unwrap_or_default(),
        notification_body: notification_body.unwrap_or_default(),
        notification_severity,
        append_extra_info,
        notification_targets,
        recipient_resolver,
        receipt_required,
        completion_policy,
        reject_policy,
        success_action,
        failure_action,
        dispatch_tasks,
    })
}
