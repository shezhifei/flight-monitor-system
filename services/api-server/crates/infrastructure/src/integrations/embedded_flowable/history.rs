//! History 方法组：历史流程实例/任务/变量。
//!
//! 历史查询 API 覆盖面比 fms 过滤键小（history_service.rs:982-1053），
//! 采用"查询 API 收敛过滤 + 实体内存过滤"混合策略；未知键 warn 忽略。
use std::sync::Arc;

use chrono::DateTime;
use fms_domain::ports::flowable_gateway::FlowableGatewayError;
use flowable_engine::engine::process_engine::ProcessEngine;
use flowable_engine::engine::query::Query;
use flowable_engine::error::FlowableError;
use flowable_engine::history::historic_entities::{
    HistoricProcessInstance, HistoricTaskInstance, HistoricVariableInstance,
};
use serde_json::{json, Value};

use super::run_on_engine;

/// HistoricProcessInstance → REST JSON。`processDefinitionKey` 经定义表 join 填充。
/// 已知差异：实体无 tenant_id（Java REST 有 tenantId）。
pub(crate) fn historic_process_instance_to_json(
    instance: &HistoricProcessInstance,
    definition_key: Option<&str>,
) -> Value {
    json!({
        "id": instance.id,
        "processDefinitionId": instance.process_definition_id,
        "processDefinitionKey": definition_key, // join 填充
        "businessKey": instance.business_key,
        "startTime": instance.start_time,
        "endTime": instance.end_time,
        "durationInMillis": instance.duration_ms,
        "startUserId": instance.start_user_id,
        "deleteReason": instance.delete_reason,
    })
}

pub(crate) fn historic_task_to_json(task: &HistoricTaskInstance) -> Value {
    json!({
        "id": task.id,
        "processInstanceId": task.process_instance_id,
        "processDefinitionId": task.process_definition_id,
        "executionId": task.execution_id,
        "taskDefinitionKey": task.task_definition_key,
        "name": task.name,
        "description": task.description,
        "assignee": task.assignee,
        "owner": task.owner,
        "claimTime": task.claim_time,
        "tenantId": task.tenant_id,
        "category": task.category,
        "formKey": task.form_key,
        "parentTaskId": task.parent_task_id,
        "priority": task.priority,
        "dueDate": task.due_date,
        "startTime": task.start_time,
        "endTime": task.end_time,
        "durationInMillis": task.duration_ms,
        "deleteReason": task.delete_reason,
    })
}

pub(crate) fn historic_variable_to_json(variable: &HistoricVariableInstance) -> Value {
    json!({
        "id": variable.id,
        "processInstanceId": variable.process_instance_id,
        "executionId": variable.execution_id,
        "taskId": variable.task_id,
        "name": variable.name,
        "variableType": variable.variable_type,
        "value": variable.value,
        "createTime": variable.create_time,
        "lastUpdatedTime": variable.last_updated_time,
    })
}

/// RFC3339 时间过滤值解析；解析失败视为不匹配该过滤（防御性）。
fn parse_instant(value: &str) -> Option<DateTime<chrono::Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|instant| instant.with_timezone(&chrono::Utc))
}

/// process_definition_id → key 映射（历史实体上没有 definition key，需 join）
fn definition_key_map(
    engine: &ProcessEngine,
) -> Result<std::collections::HashMap<String, String>, FlowableError> {
    Ok(engine
        .get_repository_service()
        .get_process_definitions()?
        .into_iter()
        .map(|def| (def.id, def.key))
        .collect())
}

pub(super) async fn get_historic_process_instances(
    engine: &Arc<ProcessEngine>,
    filters: &[(&str, String)],
) -> Result<Vec<Value>, FlowableGatewayError> {
    let filters: Vec<(String, String)> = filters
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let history = engine.get_history_service();
        // 查询 API 仅支持 process_instance_id/involved_user：processInstanceId
        // 用查询 API 收敛，其余键全量 list() 后内存过滤
        let mut query = history.create_historic_process_instance_query();
        let mut memory_filters: Vec<(String, String)> = Vec::new();
        for (key, value) in &filters {
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "processInstanceId" => query = query.process_instance_id(value),
                "involvedUser" => query = query.involved_user(value),
                _ => memory_filters.push((key.clone(), value)),
            }
        }
        let mut instances = query.list()?;
        let key_map = definition_key_map(engine)?;
        for (key, value) in &memory_filters {
            match key.as_str() {
                "businessKey" => {
                    instances.retain(|i| i.business_key.as_deref() == Some(value.as_str()))
                }
                "processDefinitionKey" => instances.retain(|i| {
                    key_map
                        .get(&i.process_definition_id)
                        .map(String::as_str)
                        == Some(value.as_str())
                }),
                "startedBefore" => {
                    if let Some(bound) = parse_instant(value) {
                        instances.retain(|i| i.start_time < bound);
                    }
                }
                "startedAfter" => {
                    if let Some(bound) = parse_instant(value) {
                        instances.retain(|i| i.start_time > bound);
                    }
                }
                "finishedBefore" => {
                    if let Some(bound) = parse_instant(value) {
                        instances.retain(|i| i.end_time.map(|t| t < bound).unwrap_or(false));
                    }
                }
                "finishedAfter" => {
                    if let Some(bound) = parse_instant(value) {
                        instances.retain(|i| i.end_time.map(|t| t > bound).unwrap_or(false));
                    }
                }
                "startedBy" => {
                    instances.retain(|i| i.start_user_id.as_deref() == Some(value.as_str()))
                }
                // 已知差异：HistoricProcessInstance 实体无 tenant_id，无法支撑
                "tenantId" => {
                    tracing::warn!("embedded flowable: historic process instance tenantId filter unsupported, ignored");
                }
                unknown => {
                    tracing::warn!(filter = unknown, "embedded flowable: ignoring unknown historic process instance filter");
                }
            }
        }
        Ok(instances
            .iter()
            .map(|instance| {
                historic_process_instance_to_json(
                    instance,
                    key_map.get(&instance.process_definition_id).map(String::as_str),
                )
            })
            .collect())
    })
    .await
}

pub(super) async fn get_historic_process_instance(
    engine: &Arc<ProcessEngine>,
    process_instance_id: &str,
) -> Result<Option<Value>, FlowableGatewayError> {
    let id = process_instance_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        let mut instances = engine
            .get_history_service()
            .create_historic_process_instance_query()
            .process_instance_id(id)
            .list()?;
        let key_map = definition_key_map(engine)?;
        Ok(instances.pop().as_ref().map(|instance| {
            historic_process_instance_to_json(
                instance,
                key_map.get(&instance.process_definition_id).map(String::as_str),
            )
        }))
    })
    .await
}

pub(super) async fn get_historic_tasks(
    engine: &Arc<ProcessEngine>,
    filters: &[(&str, String)],
) -> Result<Vec<Value>, FlowableGatewayError> {
    let filters: Vec<(String, String)> = filters
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let mut query = engine
            .get_history_service()
            .create_historic_task_instance_query();
        let mut memory_filters: Vec<(String, String)> = Vec::new();
        for (key, value) in &filters {
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "processInstanceId" => query = query.process_instance_id(value),
                "taskDefinitionKey" => query = query.task_definition_key(value),
                "assignee" => query = query.task_assignee(value),
                "owner" => query = query.task_owner(value),
                _ => memory_filters.push((key.clone(), value)),
            }
        }
        let mut tasks = query.list()?;
        for (key, value) in &memory_filters {
            match key.as_str() {
                "createdBefore" => {
                    if let Some(bound) = parse_instant(value) {
                        tasks.retain(|t| t.start_time < bound);
                    }
                }
                "createdAfter" => {
                    if let Some(bound) = parse_instant(value) {
                        tasks.retain(|t| t.start_time > bound);
                    }
                }
                "completedBefore" => {
                    if let Some(bound) = parse_instant(value) {
                        tasks.retain(|t| t.end_time.map(|e| e < bound).unwrap_or(false));
                    }
                }
                "completedAfter" => {
                    if let Some(bound) = parse_instant(value) {
                        tasks.retain(|t| t.end_time.map(|e| e > bound).unwrap_or(false));
                    }
                }
                unknown => {
                    tracing::warn!(filter = unknown, "embedded flowable: ignoring unknown historic task filter");
                }
            }
        }
        Ok(tasks.iter().map(historic_task_to_json).collect())
    })
    .await
}

pub(super) async fn get_historic_variable_instances(
    engine: &Arc<ProcessEngine>,
    filters: &[(&str, String)],
) -> Result<Vec<Value>, FlowableGatewayError> {
    let filters: Vec<(String, String)> = filters
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let mut query = engine
            .get_history_service()
            .create_historic_variable_instance_query();
        for (key, value) in &filters {
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "processInstanceId" => query = query.process_instance_id(value),
                "executionId" => query = query.execution_id(value),
                "taskId" => query = query.task_id(value),
                "variableName" => query = query.variable_name(value),
                unknown => {
                    tracing::warn!(filter = unknown, "embedded flowable: ignoring unknown historic variable filter");
                }
            }
        }
        let variables = query.list()?;
        Ok(variables.iter().map(historic_variable_to_json).collect())
    })
    .await
}
