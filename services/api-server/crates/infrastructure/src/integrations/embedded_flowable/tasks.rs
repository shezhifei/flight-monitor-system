//! Task 方法组：任务查询/认领/完成。
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use flowable_engine::engine::process_engine::ProcessEngine;
use flowable_engine::engine::query::Query;
use flowable_engine::error::FlowableError;
use flowable_engine::runtime::process_instance::ProcessInstance;
use flowable_engine::task::Task;
use fms_domain::ports::flowable_gateway::FlowableGatewayError;
use serde_json::{json, Map, Value};

use super::run_on_engine;

/// Task → REST JSON（vendored TaskResponse tasks.rs:471-508 + createTime）。
/// 已知差异：Task 实体无 `process_definition_id`（Java REST 有 `processDefinitionId`）；
/// 后端仅消费 `processInstanceId`（get_task.rs:80），前端消费情况冒烟验证。
pub(crate) fn task_to_json(task: &Task) -> Value {
    json!({
        "id": task.id,
        "name": task.name,
        "description": task.description,
        "processInstanceId": task.process_instance_id,
        "executionId": task.execution_id,
        "taskDefinitionKey": task.task_definition_key,
        "assignee": task.assignee,
        "owner": task.owner,
        "delegationState": task.delegation_state,
        "parentTaskId": task.parent_task_id,
        "priority": task.priority,
        "dueDate": task.due_date,
        "claimTime": task.claim_time,
        "createTime": task.created_time,        // Java REST 有此字段，vendored TaskResponse 缺失
        "category": task.category,
        "formKey": task.form_key,
        "tenantId": task.tenant_id,
        "state": task.state,
        "suspensionState": if task.suspension_state == 1 { "suspended" } else { "active" },
    })
}

pub(super) async fn get_tasks(
    engine: &Arc<ProcessEngine>,
    filters: &[(&str, String)],
) -> Result<Vec<Value>, FlowableGatewayError> {
    let filters: Vec<(String, String)> = filters
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        let task_service = engine.get_task_service();
        let mut query = task_service.create_task_query();
        let mut process_definition_key: Option<String> = None;
        for (key, value) in &filters {
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "processInstanceId" => query = query.process_instance_id(value),
                "assignee" => query = query.task_assignee(value),
                "owner" => query = query.task_owner(value),
                "tenantId" => query = query.task_tenant_id(value),
                "name" | "taskName" => query = query.task_name(value),
                "taskDefinitionKey" => query = query.task_definition_key(value),
                // TaskQueryCriteria 不支持 processDefinitionKey：先按定义 key 反查
                // 实例 id 集合，list() 后再按 task.process_instance_id ∈ 集合过滤
                "processDefinitionKey" => process_definition_key = Some(value),
                unknown => {
                    tracing::warn!(filter = unknown, "embedded flowable: ignoring unknown task filter");
                }
            }
        }
        let mut tasks = query.list()?;
        if let Some(key) = process_definition_key {
            let instances = engine
                .get_runtime_store()
                .db_store()
                .find_all::<ProcessInstance>("process_instances")
                .map_err(|error| FlowableError::Internal(error.to_string()))?;
            let ids: HashSet<&str> = instances
                .iter()
                .filter(|instance| instance.process_definition_key == key)
                .map(|instance| instance.id.as_str())
                .collect();
            tasks.retain(|task| ids.contains(task.process_instance_id.as_str()));
        }
        Ok(tasks.iter().map(task_to_json).collect())
    })
    .await
}

pub(super) async fn get_task(
    engine: &Arc<ProcessEngine>,
    task_id: &str,
) -> Result<Option<Value>, FlowableGatewayError> {
    let id = task_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        // TaskQuery 无 taskId 过滤，单查走 store
        let store = engine.get_runtime_store();
        let mut session = store
            .create_session()
            .map_err(|error| FlowableError::Internal(error.to_string()))?;
        let task = store.find_task(&id, &mut session);
        let _ = session.rollback();
        Ok(task.as_ref().map(task_to_json))
    })
    .await
}

pub(super) async fn claim_task(
    engine: &Arc<ProcessEngine>,
    task_id: &str,
    user_id: &str,
) -> Result<bool, FlowableGatewayError> {
    let task_id = task_id.trim().to_string();
    let user_id = user_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        match engine.get_task_service().claim_task_by_id(task_id, user_id) {
            Ok(()) => Ok(true),
            // 对齐 parse_bool_response 的 404→false 语义
            Err(FlowableError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    })
    .await
}

pub(super) async fn unclaim_task(engine: &Arc<ProcessEngine>, task_id: &str) -> Result<bool, FlowableGatewayError> {
    let task_id = task_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        match engine.get_task_service().unclaim_task_by_id(task_id) {
            Ok(()) => Ok(true),
            Err(FlowableError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    })
    .await
}

pub(super) async fn complete_task(
    engine: &Arc<ProcessEngine>,
    task_id: &str,
    variables: Option<&Map<String, Value>>,
) -> Result<bool, FlowableGatewayError> {
    let task_id = task_id.trim().to_string();
    let vars: HashMap<String, Value> = variables.cloned().unwrap_or_default().into_iter().collect();
    run_on_engine(Arc::clone(engine), move |engine| {
        match engine
            .get_task_service()
            .complete_task_by_id_with_variables(task_id, vars)
        {
            Ok(()) => Ok(true),
            Err(FlowableError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    })
    .await
}
