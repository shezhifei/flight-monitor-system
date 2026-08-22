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

#[tokio::test]
async fn runtime_instance_variables_and_executions_roundtrip() {
    let gateway = EmbeddedFlowableEngine::try_new_from_env().unwrap();
    let bpmn = include_str!("fixtures/minimal_user_task.bpmn20.xml");
    gateway
        .deploy_process(bpmn, Some("runtime-test"), None, None)
        .await
        .unwrap();

    let mut variables = serde_json::Map::new();
    variables.insert("initiator".to_string(), Value::from("tester"));
    variables.insert("count".to_string(), Value::from(3));
    let instance_id = gateway
        .start_process_instance(
            "minimalUserTask",
            Some(&variables),
            Some("biz-key-1"),
            None,
        )
        .await
        .unwrap()
        .expect("instance started");

    // 单查实例：字段形状（Java REST 超集）
    let instance = gateway
        .get_process_instance(&instance_id)
        .await
        .unwrap()
        .expect("instance found");
    assert_eq!(
        instance.get("businessKey").and_then(Value::as_str),
        Some("biz-key-1")
    );
    assert_eq!(
        instance.get("processDefinitionKey").and_then(Value::as_str),
        Some("minimalUserTask")
    );
    assert!(instance.get("startTime").is_some());

    // 列表过滤
    let listed = gateway
        .get_process_instances(&[("businessKey", "biz-key-1".to_string())])
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);

    // 变量往返
    let vars = gateway
        .get_process_instance_variables(&instance_id)
        .await
        .unwrap();
    let vars = vars.as_array().expect("variables array");
    let initiator = vars
        .iter()
        .find(|v| v.get("name").and_then(Value::as_str) == Some("initiator"))
        .expect("initiator variable");
    assert_eq!(
        initiator.get("value").and_then(Value::as_str),
        Some("tester")
    );

    let mut updates = serde_json::Map::new();
    updates.insert("reviewed".to_string(), Value::from(true));
    assert!(
        gateway
            .set_process_instance_variables(&instance_id, &updates)
            .await
            .unwrap()
    );
    let vars = gateway
        .get_process_instance_variables(&instance_id)
        .await
        .unwrap();
    assert!(
        vars.as_array()
            .unwrap()
            .iter()
            .any(|v| v.get("name").and_then(Value::as_str) == Some("reviewed"))
    );

    // executions 过滤
    let executions = gateway
        .get_executions(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert!(!executions.is_empty());
    assert!(
        executions
            .iter()
            .all(|e| e.get("processInstanceId").and_then(Value::as_str) == Some(&instance_id))
    );

    // 删除实例
    assert!(
        gateway
            .delete_process_instance(&instance_id, Some("test cleanup"))
            .await
            .unwrap()
    );
    assert!(
        gateway
            .get_process_instance(&instance_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn task_lifecycle_claim_complete_roundtrip() {
    let gateway = EmbeddedFlowableEngine::try_new_from_env().unwrap();
    let bpmn = include_str!("fixtures/minimal_user_task.bpmn20.xml");
    gateway
        .deploy_process(bpmn, Some("task-test"), None, None)
        .await
        .unwrap();
    let instance_id = gateway
        .start_process_instance("minimalUserTask", None, None, None)
        .await
        .unwrap()
        .expect("instance started");

    let tasks = gateway
        .get_tasks(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    let task = &tasks[0];
    for field in ["id", "name", "processInstanceId", "createTime"] {
        assert!(task.get(field).is_some(), "task JSON missing `{field}`");
    }
    assert!(task.get("assignee").unwrap().is_null());
    let task_id = task.get("id").and_then(Value::as_str).unwrap().to_string();

    assert!(gateway.claim_task(&task_id, "kermit").await.unwrap());
    let claimed = gateway.get_task(&task_id).await.unwrap().expect("task");
    assert_eq!(
        claimed.get("assignee").and_then(Value::as_str),
        Some("kermit")
    );

    assert!(gateway.unclaim_task(&task_id).await.unwrap());
    assert!(gateway.claim_task(&task_id, "kermit").await.unwrap());

    let mut variables = serde_json::Map::new();
    variables.insert("approved".to_string(), Value::from(true));
    assert!(
        gateway
            .complete_task(&task_id, Some(&variables))
            .await
            .unwrap()
    );

    let tasks = gateway
        .get_tasks(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn history_records_after_completed_process() {
    let gateway = EmbeddedFlowableEngine::try_new_from_env().unwrap();
    let bpmn = include_str!("fixtures/minimal_user_task.bpmn20.xml");
    gateway
        .deploy_process(bpmn, Some("history-test"), None, None)
        .await
        .unwrap();
    let mut variables = serde_json::Map::new();
    variables.insert("initiator".to_string(), Value::from("tester"));
    let instance_id = gateway
        .start_process_instance("minimalUserTask", Some(&variables), Some("hist-biz-1"), None)
        .await
        .unwrap()
        .expect("instance started");

    // 完成流程：claim + complete 唯一任务
    let tasks = gateway
        .get_tasks(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    let task_id = tasks[0].get("id").and_then(Value::as_str).unwrap().to_string();
    gateway.claim_task(&task_id, "kermit").await.unwrap();
    gateway.complete_task(&task_id, None).await.unwrap();

    // 历史流程实例
    let historic = gateway
        .get_historic_process_instances(&[])
        .await
        .unwrap();
    let found = historic
        .iter()
        .find(|i| i.get("id").and_then(Value::as_str) == Some(&instance_id))
        .expect("historic instance recorded");
    assert!(found.get("endTime").is_some());
    assert_eq!(
        found.get("processDefinitionKey").and_then(Value::as_str),
        Some("minimalUserTask")
    );
    assert_eq!(
        found.get("businessKey").and_then(Value::as_str),
        Some("hist-biz-1")
    );

    // 按 businessKey 过滤
    let filtered = gateway
        .get_historic_process_instances(&[("businessKey", "hist-biz-1".to_string())])
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);

    // 单查历史实例
    let single = gateway
        .get_historic_process_instance(&instance_id)
        .await
        .unwrap()
        .expect("historic instance");
    assert!(single.get("durationInMillis").is_some());

    // 历史任务
    let historic_tasks = gateway
        .get_historic_tasks(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert!(!historic_tasks.is_empty());
    assert!(
        historic_tasks
            .iter()
            .all(|t| t.get("endTime").is_some() && !t.get("endTime").unwrap().is_null())
    );

    // 历史变量
    let historic_vars = gateway
        .get_historic_variable_instances(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert!(
        historic_vars
            .iter()
            .any(|v| v.get("name").and_then(Value::as_str) == Some("initiator"))
    );
}

/// 端到端契约测试：单个测试串起 FlowableGateway 全部 22 个方法，
/// 每步断言 camelCase JSON 形状与关键值。
#[tokio::test]
async fn full_gateway_contract_end_to_end() {
    let gateway = EmbeddedFlowableEngine::try_new_from_env().unwrap();
    let bpmn = include_str!("fixtures/minimal_user_task.bpmn20.xml");

    // 1. deploy
    let deployed = gateway
        .deploy_process(bpmn, Some("e2e-deployment"), Some("e2e"), None)
        .await
        .unwrap();
    let deployment_id = deployed.get("id").and_then(Value::as_str).unwrap().to_string();
    assert_eq!(
        deployed.get("category").and_then(Value::as_str),
        Some("e2e")
    );

    // 2. list deployments
    let deployments = gateway
        .get_deployments(Some("e2e-deployment"), None)
        .await
        .unwrap();
    assert_eq!(deployments.len(), 1);
    assert!(deployments[0].get("deploymentTime").is_some());

    // 3. list definitions
    let defs = gateway
        .get_process_definitions(Some("minimalUserTask"), None)
        .await
        .unwrap();
    assert_eq!(defs.len(), 1);
    let def_id = defs[0].get("id").and_then(Value::as_str).unwrap().to_string();

    // 4. get single definition
    let def = gateway
        .get_process_definition(&def_id)
        .await
        .unwrap()
        .expect("definition");
    assert_eq!(def.get("key").and_then(Value::as_str), Some("minimalUserTask"));

    // 5. definition xml
    let xml = gateway
        .get_process_definition_xml(&def_id)
        .await
        .unwrap()
        .expect("xml");
    assert!(xml.contains("minimalUserTask"));

    // 6. start instance（带变量 + businessKey）
    let mut start_vars = serde_json::Map::new();
    start_vars.insert("requester".to_string(), Value::from("e2e"));
    let instance_id = gateway
        .start_process_instance("minimalUserTask", Some(&start_vars), Some("e2e-biz"), None)
        .await
        .unwrap()
        .expect("started");

    // 7. list instances
    let instances = gateway
        .get_process_instances(&[("businessKey", "e2e-biz".to_string())])
        .await
        .unwrap();
    assert_eq!(instances.len(), 1);

    // 8. get instance
    let instance = gateway
        .get_process_instance(&instance_id)
        .await
        .unwrap()
        .expect("instance");
    assert_eq!(
        instance.get("processDefinitionId").and_then(Value::as_str),
        Some(def_id.as_str())
    );

    // 9. executions
    let executions = gateway
        .get_executions(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert!(!executions.is_empty());

    // 10/11. variables get/set
    let vars = gateway
        .get_process_instance_variables(&instance_id)
        .await
        .unwrap();
    assert!(
        vars.as_array()
            .unwrap()
            .iter()
            .any(|v| v.get("name").and_then(Value::as_str) == Some("requester"))
    );
    let mut set_vars = serde_json::Map::new();
    set_vars.insert("step".to_string(), Value::from(1));
    assert!(
        gateway
            .set_process_instance_variables(&instance_id, &set_vars)
            .await
            .unwrap()
    );

    // 12/13. task list + single
    let tasks = gateway
        .get_tasks(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    let task_id = tasks[0].get("id").and_then(Value::as_str).unwrap().to_string();
    let task = gateway.get_task(&task_id).await.unwrap().expect("task");
    assert_eq!(
        task.get("taskDefinitionKey").and_then(Value::as_str),
        Some("theTask")
    );

    // 14/15/16. claim / unclaim / complete
    assert!(gateway.claim_task(&task_id, "e2e-user").await.unwrap());
    assert!(gateway.unclaim_task(&task_id).await.unwrap());
    assert!(gateway.claim_task(&task_id, "e2e-user").await.unwrap());
    let mut done_vars = serde_json::Map::new();
    done_vars.insert("result".to_string(), Value::from("ok"));
    assert!(
        gateway
            .complete_task(&task_id, Some(&done_vars))
            .await
            .unwrap()
    );

    // 17-20. history
    let historic = gateway
        .get_historic_process_instances(&[("businessKey", "e2e-biz".to_string())])
        .await
        .unwrap();
    assert_eq!(historic.len(), 1);
    assert!(!historic[0].get("endTime").unwrap().is_null());

    let historic_tasks = gateway
        .get_historic_tasks(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert_eq!(historic_tasks.len(), 1);
    assert_eq!(
        historic_tasks[0].get("assignee").and_then(Value::as_str),
        Some("e2e-user")
    );

    let historic_instance = gateway
        .get_historic_process_instance(&instance_id)
        .await
        .unwrap()
        .expect("historic instance");
    assert_eq!(
        historic_instance.get("processDefinitionKey").and_then(Value::as_str),
        Some("minimalUserTask")
    );

    let historic_vars = gateway
        .get_historic_variable_instances(&[("processInstanceId", instance_id.clone())])
        .await
        .unwrap();
    assert!(
        historic_vars
            .iter()
            .any(|v| v.get("name").and_then(Value::as_str) == Some("requester"))
    );

    // 21. delete instance（已结束实例幂等语义：存在与否均不报错）
    let _ = gateway
        .delete_process_instance(&instance_id, Some("e2e done"))
        .await;

    // 22. delete deployment
    assert!(
        gateway
            .delete_deployment(&deployment_id, true)
            .await
            .unwrap()
    );
    assert!(
        gateway
            .get_process_definitions(Some("minimalUserTask"), None)
            .await
            .unwrap()
            .is_empty()
    );
}
