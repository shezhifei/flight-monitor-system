//! `StartProcessInstanceCmd` 租户解析的 Java 奇偶回归测试。
//!
//! Java `StartProcessInstanceCmd#execute`：tenantId 为空时按
//! `findLatestProcessDefinitionByKey` 解析（不按租户过滤），只有显式租户
//! 才走 `findLatestProcessDefinitionByKeyAndTenantId` 精确匹配。
//! 回归背景：部署端默认带租户（如 COMMON）而启动端不传租户时，
//! 旧实现做 `tenant_id == builder.tenant_id` 严格相等过滤，导致
//! "No process definition found for process instance start request"。

use flowable_engine::engine::process_engine::ProcessEngine;

const SIMPLE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL"
             xmlns:flowable="http://flowable.org/bpmn"
             targetNamespace="Examples">
    <process id="tenantParityProcess" isExecutable="true">
        <startEvent id="startEvent" />
        <sequenceFlow id="flow1" sourceRef="startEvent" targetRef="endEvent" />
        <endEvent id="endEvent" />
    </process>
</definitions>"#;

#[test]
fn start_by_key_without_tenant_matches_tenant_scoped_definition() {
    let process_engine = ProcessEngine::new("default".to_string());
    let repository_service = process_engine.get_repository_service();
    let runtime_service = process_engine.get_runtime_service();

    repository_service
        .deploy(
            repository_service
                .create_deployment()
                .name("tenant scoped".to_string())
                .tenant_id("COMMON".to_string())
                .add_string("process.bpmn20.xml".to_string(), SIMPLE_XML.to_string()),
        )
        .unwrap();

    // 不传租户启动：Java 语义下不按租户过滤，必须能找到 COMMON 租户的定义。
    let builder = runtime_service
        .create_process_instance_builder()
        .process_definition_key("tenantParityProcess".to_string());
    let instance = runtime_service
        .start_process_instance(builder)
        .expect("start without tenant must resolve tenant-scoped definition (Java parity)");

    assert_eq!(instance.tenant_id.as_deref(), Some("COMMON"));
}

#[test]
fn start_by_key_with_tenant_only_matches_same_tenant() {
    let process_engine = ProcessEngine::new("default".to_string());
    let repository_service = process_engine.get_repository_service();
    let runtime_service = process_engine.get_runtime_service();

    repository_service
        .deploy(
            repository_service
                .create_deployment()
                .name("acme".to_string())
                .tenant_id("acme".to_string())
                .add_string("process.bpmn20.xml".to_string(), SIMPLE_XML.to_string()),
        )
        .unwrap();

    // 显式其他租户：Java `findLatestProcessDefinitionByKeyAndTenantId` 精确匹配，不应命中。
    let builder = runtime_service
        .create_process_instance_builder()
        .process_definition_key("tenantParityProcess".to_string())
        .tenant_id("other".to_string());
    let result = runtime_service.start_process_instance(builder);
    assert!(
        result.is_err(),
        "start with mismatched tenant must not resolve another tenant's definition"
    );
}
