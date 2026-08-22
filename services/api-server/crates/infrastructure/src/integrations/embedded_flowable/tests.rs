//! 嵌入式网关契约测试（内存引擎）。
//!
//! 必须用 `#[tokio::test]`：网关方法内部用 `tokio::task::spawn_blocking`，
//! 在非 tokio runtime 下调用会 panic。
use serde_json::Value;

use super::EmbeddedFlowableEngine;
use fms_domain::ports::flowable_gateway::FlowableGateway;

#[tokio::test]
async fn embedded_engine_boots_with_memory_backend() {
    let gateway = EmbeddedFlowableEngine::try_new_from_env().expect("engine boots");
    // 引擎可用性探针：列出流程定义（空库返回空数组）
    let defs = gateway
        .get_process_definitions(None, None)
        .await
        .expect("list definitions");
    assert!(defs.is_empty());
}

#[tokio::test]
async fn deploy_then_list_definitions_roundtrip() {
    let gateway = EmbeddedFlowableEngine::try_new_from_env().unwrap();
    let bpmn = include_str!("fixtures/minimal_user_task.bpmn20.xml");
    let deployed = gateway
        .deploy_process(bpmn, Some("minimal-user-task"), None, None)
        .await
        .unwrap();
    assert!(deployed.get("id").and_then(Value::as_str).is_some());
    assert_eq!(
        deployed.get("name").and_then(Value::as_str),
        Some("minimal-user-task")
    );

    let defs = gateway.get_process_definitions(None, None).await.unwrap();
    let found = defs
        .iter()
        .find(|def| def.get("key").and_then(Value::as_str) == Some("minimalUserTask"))
        .expect("deployed definition is listed");
    for field in ["id", "key", "name", "version", "resourceName", "deploymentId"] {
        assert!(found.get(field).is_some(), "definition JSON missing `{field}`");
    }

    let def_id = found.get("id").and_then(Value::as_str).unwrap().to_string();
    let xml = gateway
        .get_process_definition_xml(&def_id)
        .await
        .unwrap()
        .expect("xml present");
    assert!(xml.contains("<process"));

    let deployment_id = deployed
        .get("id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    assert!(
        gateway
            .delete_deployment(&deployment_id, true)
            .await
            .unwrap()
    );
    let defs = gateway.get_process_definitions(None, None).await.unwrap();
    assert!(defs.is_empty());
}
