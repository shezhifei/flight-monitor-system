//! Repository 方法组：流程定义与部署。
//!
//! JSON 字段清单对齐 flowable-rest routes/deployments.rs 与
//! process_definitions.rs:178-230（Java REST 形状）。
use std::sync::Arc;

use flowable_engine::engine::process_engine::ProcessEngine;
use flowable_engine::error::FlowableError;
use flowable_engine::repository::deployment::Deployment;
use flowable_engine::repository::process_definition::ProcessDefinition;
use fms_domain::ports::flowable_gateway::FlowableGatewayError;
use serde_json::{json, Value};

use super::run_on_engine;

/// Deployment → REST JSON（字段对齐 flowable-rest routes/deployments.rs::deployment_response）
pub(crate) fn deployment_to_json(deployment: &Deployment) -> Value {
    json!({
        "id": deployment.id,
        "name": deployment.name,
        "category": deployment.category,
        "key": deployment.key,
        "tenantId": deployment.tenant_id,
        "deploymentTime": deployment.deployment_time, // Option<DateTime<Utc>>，serde 序列化为 RFC3339
    })
}

/// ProcessDefinition → REST JSON（字段对齐 flowable-rest routes/process_definitions.rs:178-230）
pub(crate) fn process_definition_to_json(def: &ProcessDefinition) -> Value {
    json!({
        "id": def.id,
        "key": def.key,
        "category": def.category,
        "name": def.name,
        "description": def.description,
        "version": def.version,
        "resourceName": def.resource_name,
        "deploymentId": def.deployment_id,
        "diagramResourceName": def.diagram_resource_name,
        "tenantId": def.tenant_id,
        "suspended": def.is_suspended, // 引擎实体字段是 is_suspended（非硬编码）
    })
}

pub(super) async fn get_process_definitions(
    engine: &Arc<ProcessEngine>,
    key: Option<&str>,
    tenant_id: Option<&str>,
) -> Result<Vec<Value>, FlowableGatewayError> {
    let key = key.map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned);
    let tenant_id = tenant_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    run_on_engine(Arc::clone(engine), move |engine| {
        // repository_service.rs:134-146：返回全部定义（按 key/id 排序），无过滤参数
        let defs = engine.get_repository_service().get_process_definitions()?;
        Ok(defs
            .into_iter()
            .filter(|def| key.as_deref().map(|k| def.key == k).unwrap_or(true))
            .filter(|def| {
                tenant_id
                    .as_deref()
                    .map(|t| def.tenant_id.as_deref() == Some(t))
                    .unwrap_or(true)
            })
            .map(|def| process_definition_to_json(&def))
            .collect())
    })
    .await
}

pub(super) async fn get_process_definition(
    engine: &Arc<ProcessEngine>,
    process_definition_id: &str,
) -> Result<Option<Value>, FlowableGatewayError> {
    let id = process_definition_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        match engine.get_repository_service().get_process_definition(&id) {
            Ok(def) => Ok(Some(process_definition_to_json(&def))),
            Err(FlowableError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    })
    .await
}

pub(super) async fn get_process_definition_xml(
    engine: &Arc<ProcessEngine>,
    process_definition_id: &str,
) -> Result<Option<String>, FlowableGatewayError> {
    let id = process_definition_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        let repo = engine.get_repository_service();
        let def = match repo.get_process_definition(&id) {
            Ok(def) => def,
            Err(FlowableError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let (Some(deployment_id), Some(resource_name)) = (def.deployment_id.as_deref(), def.resource_name.as_deref())
        else {
            return Ok(None);
        };
        // 对照 flowable-rest process_definitions.rs:664-687 的
        // get_process_definition_resource_data 链路
        match repo.get_deployment_resource(deployment_id, resource_name) {
            Ok(resource) => String::from_utf8(resource.bytes)
                .map(Some)
                .map_err(|error| FlowableError::Internal(format!("deployment resource not utf-8: {error}"))),
            Err(FlowableError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    })
    .await
}

pub(super) async fn get_deployments(
    engine: &Arc<ProcessEngine>,
    name: Option<&str>,
    tenant_id: Option<&str>,
) -> Result<Vec<Value>, FlowableGatewayError> {
    let name = name.map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned);
    let tenant_id = tenant_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    run_on_engine(Arc::clone(engine), move |engine| {
        let deployments = engine.get_repository_service().get_deployments()?;
        Ok(deployments
            .into_iter()
            // 对齐旧 FlowableClient 的 nameLike %name% 子串语义（flowable_client.rs:132）
            .filter(|deployment| {
                name.as_deref()
                    .map(|n| deployment.name.as_deref().map(|name| name.contains(n)).unwrap_or(false))
                    .unwrap_or(true)
            })
            .filter(|deployment| {
                tenant_id
                    .as_deref()
                    .map(|t| deployment.tenant_id.as_deref() == Some(t))
                    .unwrap_or(true)
            })
            .map(|deployment| deployment_to_json(&deployment))
            .collect())
    })
    .await
}

pub(super) async fn deploy_process(
    engine: &Arc<ProcessEngine>,
    bpmn_xml: &str,
    deployment_name: Option<&str>,
    category: Option<&str>,
    tenant_id: Option<&str>,
) -> Result<Value, FlowableGatewayError> {
    let xml = bpmn_xml.to_string();
    let name = deployment_name.unwrap_or("process.bpmn20.xml").trim().to_string();
    let category = category.map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned);
    let tenant_id = tenant_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    run_on_engine(Arc::clone(engine), move |engine| {
        let repo = engine.get_repository_service();
        let mut builder = repo
            .create_deployment()
            .add_string("process.bpmn20.xml".to_string(), xml);
        if !name.is_empty() {
            builder = builder.name(name);
        }
        if let Some(category) = category {
            builder = builder.category(category);
        }
        if let Some(tenant) = tenant_id {
            builder = builder.tenant_id(tenant);
        }
        // 关键：走 repository_service.deploy()（DeployCmd，落库+BPMN 解析）。
        // 绝不能调 builder.deploy()——那只是本地构造结构体，不落库。
        let deployment = repo.deploy(builder)?;
        Ok(deployment_to_json(&deployment))
    })
    .await
}

pub(super) async fn delete_deployment(
    engine: &Arc<ProcessEngine>,
    deployment_id: &str,
    cascade: bool,
) -> Result<bool, FlowableGatewayError> {
    let id = deployment_id.trim().to_string();
    run_on_engine(Arc::clone(engine), move |engine| {
        match engine
            .get_repository_service()
            .delete_deployment_with_cascade(&id, cascade)
        {
            Ok(()) => Ok(true),
            // 对齐 parse_bool_response 的 404→false 语义
            Err(FlowableError::NotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    })
    .await
}
