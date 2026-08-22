//! Runtime 方法组：流程实例、变量、executions。
//!
//! 列表查询照搬 flowable-rest 的实现范式（process_instances_query.rs:229-243）：
//! `db_store().find_all` 全量取出后内存 `retain` 过滤；未知过滤键 warn 忽略。
use std::collections::HashMap;
use std::sync::Arc;

use fms_domain::ports::flowable_gateway::FlowableGatewayError;
use flowable_engine::engine::process_engine::ProcessEngine;
use flowable_engine::error::FlowableError;
use flowable_engine::runtime::execution::Execution;
use flowable_engine::runtime::process_instance::ProcessInstance;
use serde_json::{json, Map, Value};

use super::run_on_engine;

/// ProcessInstance → REST JSON（Java REST 超集；vendored ProcessInstanceResponse
/// 是子集——无 startTime/tenantId，不要以它为准）
pub(crate) fn process_instance_to_json(instance: &ProcessInstance) -> Value {
    json!({
        "id": instance.id,
        "name": instance.name,
        "processDefinitionId": instance.process_definition_id,
        "processDefinitionKey": instance.process_definition_key,
        "processDefinitionName": instance.process_definition_name,
        "processDefinitionVersion": instance.process_definition_version,
        "businessKey": instance.business_key,
        "businessStatus": instance.business_status,
        "startTime": instance.start_time,       // Option<DateTime<Utc>> → RFC3339
        "startUserId": instance.start_user_id,
        "isEnded": instance.is_ended,
        "isSuspended": instance.is_suspended,
        "tenantId": instance.tenant_id,
        "callbackId": instance.callback_id,
        "callbackType": instance.callback_type,
        "referenceId": instance.reference_id,
        "referenceType": instance.reference_type,
    })
}

/// Execution → REST JSON（字段对齐 flowable-rest process_instances_query.rs:874-895）
pub(crate) fn execution_to_json(execution: &Execution) -> Value {
    json!({
        "id": execution.id,
        "processInstanceId": execution.process_instance_id,
        "processDefinitionId": execution.process_definition_id,
        "activityId": execution.activity_id,
        "parentId": execution.parent_id,
        "isActive": execution.is_active,
        "isEnded": execution.is_ended,
        "isSuspended": execution.is_suspended,
    })
}

/// 变量值 → Java REST 的 type 字段（前端不消费此字段，宽松推断即可）
fn variable_type_of(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "double",
        _ => "json",
    }
}

/// 解析 process instance 的当前 execution（照搬 flowable-rest
/// process_instances.rs:2573-2591 的 find_current_execution_for_process_instance 模式）。
fn find_current_execution(
    engine: &ProcessEngine,
    process_instance_id: &str,
) -> Result<Option<Execution>, FlowableError> {
    let store = engine.get_runtime_store();
    let mut session = store
        .create_session()
        .map_err(|error| FlowableError::Internal(error.to_string()))?;
    let direct = store.find_execution(process_instance_id, &mut session);
    let _ = session.rollback();
    if let Some(execution) = direct {
        if execution.process_instance_id.as_deref() == Some(process_instance_id)
            && !execution.is_ended
        {
            return Ok(Some(execution));
        }
    }
    let executions = store
        .db_store()
        .find_all::<Execution>("executions")
        .map_err(|error| FlowableError::Internal(error.to_string()))?;
    Ok(executions.into_iter().find(|execution| {
        execution.process_instance_id.as_deref() == Some(process_instance_id)
            && !execution.is_ended
    }))
}

pub(super) async fn start_process_instance(
    engine: &Arc<ProcessEngine>,
    process_key: &str,
    variables: Option<&Map<String, Value>>,
    business_key: Option<&str>,
    tenant_id: Option<&str>,
) -> Result<Option<String>, FlowableGatewayError> {
    let key = process_key.trim().to_string();
    let vars: HashMap<String, Value> = variables.cloned().unwrap_or_default().into_iter().collect();
    let business_key = business_key
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let tenant_id = tenant_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    run_on_engine(Arc::clone(engine), move |engine| {
        let runtime = engine.get_runtime_service();
        let mut builder = runtime
            .create_process_instance_builder()
            .process_definition_key(key);
        for (name, value) in vars {
            builder = builder.variable(name, value);
        }
        if let Some(business_key) = business_key {
            builder = builder.business_key(business_key);
        }
        if let Some(tenant) = tenant_id {
            builder = builder.tenant_id(tenant);
        }
        let instance = runtime.start_process_instance(builder)?;
        Ok(Some(instance.id))
    })
    .await
}

pub(super) async fn get_process_instances(
    engine: &Arc<ProcessEngine>,
    filters: &[(&str, String)],
) -> Result<Vec<Value>, FlowableGatewayError> {
    let filters: Vec<(String, String)> = filters
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let mut instances = engine
            .get_runtime_store()
            .db_store()
            .find_all::<ProcessInstance>("process_instances")
            .map_err(|error| FlowableError::Internal(error.to_string()))?;
        for (key, value) in &filters {
            let value = value.trim();
            match key.as_str() {
                "processInstanceId" => instances.retain(|i| i.id == value),
                "processDefinitionId" => instances.retain(|i| i.process_definition_id == value),
                "processDefinitionKey" => instances.retain(|i| i.process_definition_key == value),
                "businessKey" => {
                    instances.retain(|i| i.business_key.as_deref() == Some(value))
                }
                "tenantId" => instances.retain(|i| i.tenant_id.as_deref() == Some(value)),
                unknown => {
                    tracing::warn!(filter = unknown, "embedded flowable: ignoring unknown process instance filter");
                }
            }
        }
        Ok(instances.iter().map(process_instance_to_json).collect())
    })
    .await
}

pub(super) async fn get_process_instance(
    engine: &Arc<ProcessEngine>,
    process_instance_id: &str,
) -> Result<Option<Value>, FlowableGatewayError> {
    let id = process_instance_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        let store = engine.get_runtime_store();
        let mut session = store
            .create_session()
            .map_err(|error| FlowableError::Internal(error.to_string()))?;
        let instance = store.find_process_instance(&id, &mut session);
        let _ = session.rollback();
        Ok(instance.as_ref().map(process_instance_to_json))
    })
    .await
}

pub(super) async fn delete_process_instance(
    engine: &Arc<ProcessEngine>,
    process_instance_id: &str,
    delete_reason: Option<&str>,
) -> Result<bool, FlowableGatewayError> {
    let id = process_instance_id.trim().to_string();
    let reason = delete_reason.map(ToOwned::to_owned);
    run_on_engine(Arc::clone(engine), move |engine| {
        match engine.get_runtime_service().delete_process_instance(id, reason) {
            Ok(()) => Ok(true),
            // 对齐 parse_bool_response 的 404→false 语义
            Err(FlowableError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    })
    .await
}

pub(super) async fn get_process_instance_variables(
    engine: &Arc<ProcessEngine>,
    process_instance_id: &str,
) -> Result<Value, FlowableGatewayError> {
    let id = process_instance_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        let execution = find_current_execution(engine, &id)?
            .ok_or_else(|| FlowableError::NotFound(format!("process instance '{id}' not found")))?;
        let variables = engine.get_runtime_service().get_variables(execution.id)?;
        // REST 契约：[{name, type, value}] 数组（Java 对 process instance 变量返回 scope=null，省略）
        Ok(Value::Array(
            variables
                .into_iter()
                .map(|(name, value)| {
                    json!({
                        "name": name,
                        "type": variable_type_of(&value),
                        "value": value,
                    })
                })
                .collect(),
        ))
    })
    .await
}

pub(super) async fn set_process_instance_variables(
    engine: &Arc<ProcessEngine>,
    process_instance_id: &str,
    variables: &Map<String, Value>,
) -> Result<bool, FlowableGatewayError> {
    let id = process_instance_id.trim().to_string();
    let vars: Vec<(String, Value)> = variables
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let Some(execution) = find_current_execution(engine, &id)? else {
            return Ok(false);
        };
        let runtime = engine.get_runtime_service();
        for (name, value) in vars {
            runtime.set_variable(execution.id.clone(), name, value)?;
        }
        Ok(true)
    })
    .await
}

pub(super) async fn get_executions(
    engine: &Arc<ProcessEngine>,
    filters: &[(&str, String)],
) -> Result<Vec<Value>, FlowableGatewayError> {
    let filters: Vec<(String, String)> = filters
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let mut executions = engine
            .get_runtime_store()
            .db_store()
            .find_all::<Execution>("executions")
            .map_err(|error| FlowableError::Internal(error.to_string()))?;
        for (key, value) in &filters {
            let value = value.trim();
            match key.as_str() {
                "processInstanceId" => {
                    executions.retain(|e| e.process_instance_id.as_deref() == Some(value))
                }
                "processDefinitionId" => {
                    executions.retain(|e| e.process_definition_id.as_deref() == Some(value))
                }
                "processDefinitionKey" => {
                    executions.retain(|e| e.process_definition_key.as_deref() == Some(value))
                }
                "id" | "executionId" => executions.retain(|e| e.id == value),
                "activityId" => {
                    executions.retain(|e| e.activity_id.as_deref() == Some(value))
                }
                "tenantId" => executions.retain(|e| e.tenant_id.as_deref() == Some(value)),
                unknown => {
                    tracing::warn!(filter = unknown, "embedded flowable: ignoring unknown execution filter");
                }
            }
        }
        Ok(executions.iter().map(execution_to_json).collect())
    })
    .await
}
