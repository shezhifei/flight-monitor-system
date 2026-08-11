use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;
use fms_domain::ports::business_case_workflow_run_repository::BusinessCaseWorkflowRunRepository;

use super::service::{
    build_batch_notification_body, build_flowable_start_variables, build_notification_body,
    build_wait_receipt_completion_variables, build_workflow_start_payload, compute_recipient_set_hash,
    derive_batch_notification_idempotency_context, derive_flowable_run_status,
    derive_per_case_batch_notification_idempotency_context, mark_run_as_system_error, normalize_process_instance,
    normalize_workflow_extra_info, parse_bpmn_runtime_definition, parse_workflow_batch_policy,
    require_linked_business_case, resolve_wait_task, BusinessCaseWorkflowBatchItem, FlowableStartSnapshot,
    WorkflowActor, WorkflowBatchPlanItem, WorkflowBatchPolicy, WorkflowBatchReceiptMode, WorkflowBusinessCaseAction,
    WorkflowNotificationGroupKey, WorkflowRecipientResolverConfig, WorkflowRuntimeDefinition,
};
use std::collections::HashMap;

fn sample_run() -> BusinessCaseWorkflowRun {
    let now = Utc::now();
    BusinessCaseWorkflowRun {
        run_id: "run_001".to_string(),
        template_code: "gate_baggage_check".to_string(),
        case_id: "case_001".to_string(),
        flight_id: "flight_001".to_string(),
        process_definition_key: "gate_baggage_check".to_string(),
        process_instance_id: "proc_001".to_string(),
        waiting_task_id: Some("task_wait".to_string()),
        receipt_group_id: Some("receipt_001".to_string()),
        status: "waiting_receipts".to_string(),
        outcome: None,
        recipient_snapshot: Vec::new(),
        flight_context_snapshot: HashMap::new(),
        start_payload: HashMap::new(),
        started_by: "tester".to_string(),
        completed_at: None,
        failed_reason: None,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn missing_linked_business_case_is_not_silently_accepted() {
    let error = require_linked_business_case("bc_001", None).expect_err("should fail");
    assert_eq!(
        error.to_string(),
        "内部错误: Business case workflow linked case missing: bc_001"
    );
}

#[test]
fn parse_bpmn_runtime_definition_reads_append_extra_info_flag() {
    let xml = r#"
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:fm="http://flight-monitor/schema/bpmn">
  <bpmn:process id="gate_baggage_check" isExecutable="true">
    <bpmn:extensionElements>
      <fm:workflowTemplate templateCode="gate_baggage_check" caseType="gate_baggage_check" />
    </bpmn:extensionElements>
    <bpmn:userTask id="notify_departments">
      <bpmn:extensionElements>
        <fm:notificationRule action="dispatch_notify" severity="critical" receiptRequired="true" appendExtraInfo="true" title="通知 ${flight_no}" bodyTemplate="航班 ${flight_no}">
          <fm:targets>
            <fm:target department="地服调度" roles="dispatcher,supervisor" />
          </fm:targets>
        </fm:notificationRule>
        <fm:receiptRule completionPolicy="all_notified_acknowledged" rejectPolicy="fail_on_any_reject" />
        <fm:recipientResolver source="department_roles" emptyPolicy="fail" deduplicate="true" />
      </bpmn:extensionElements>
    </bpmn:userTask>
    <bpmn:userTask id="wait_receipts" />
    <bpmn:userTask id="complete_business_case">
      <bpmn:extensionElements>
        <fm:businessCaseAction action="complete_case" targetStatus="COMPLETED" />
      </bpmn:extensionElements>
    </bpmn:userTask>
    <bpmn:userTask id="fail_business_case">
      <bpmn:extensionElements>
        <fm:businessCaseAction action="fail_case" targetStatus="FAILED" />
      </bpmn:extensionElements>
    </bpmn:userTask>
  </bpmn:process>
</bpmn:definitions>
"#;

    let definition = parse_bpmn_runtime_definition(xml).expect("definition");

    assert!(definition.append_extra_info);
    assert_eq!(definition.notification_severity, "critical");
    assert_eq!(definition.case_type, "gate_baggage_check");
    assert_eq!(definition.recipient_resolver.source, "department_roles");
    assert_eq!(definition.recipient_resolver.empty_policy, "fail");
    assert!(definition.recipient_resolver.deduplicate);
}

#[test]
fn normalize_workflow_extra_info_backfills_summary_and_flight_fields() {
    let source = HashMap::from([
        (
            "trigger_reason".to_string(),
            serde_json::Value::String("发现违禁品".to_string()),
        ),
        (
            "extra_info".to_string(),
            serde_json::Value::String("旅客行李需要开包检查".to_string()),
        ),
    ]);
    let flight_context = HashMap::from([
        ("gate".to_string(), serde_json::Value::String("A12".to_string())),
        ("stand".to_string(), serde_json::Value::String("S01".to_string())),
    ]);

    let normalized = normalize_workflow_extra_info(&source, "旅客行李需要开包检查", &flight_context, None, None);

    assert_eq!(
        normalized.get("gate"),
        Some(&serde_json::Value::String("A12".to_string()))
    );
    assert_eq!(
        normalized.get("gate_no"),
        Some(&serde_json::Value::String("A12".to_string()))
    );
    assert_eq!(
        normalized.get("summary"),
        Some(&serde_json::Value::String("旅客行李需要开包检查".to_string()))
    );
}

#[test]
fn build_notification_body_appends_extra_info_once() {
    let variables = HashMap::from([("flight_no".to_string(), serde_json::Value::String("CA1234".to_string()))]);
    let extra_info = HashMap::from([(
        "extra_info".to_string(),
        serde_json::Value::String("旅客行李需要开包检查".to_string()),
    )]);

    let body = build_notification_body("航班 ${flight_no} 需要处理", &variables, true, &extra_info);
    let duplicated = build_notification_body(
        "航班 ${flight_no} 需要处理\n额外信息：旅客行李需要开包检查",
        &variables,
        true,
        &extra_info,
    );

    assert_eq!(body, "航班 CA1234 需要处理\n额外信息：旅客行李需要开包检查");
    assert_eq!(duplicated, "航班 CA1234 需要处理\n额外信息：旅客行李需要开包检查");
}

#[test]
fn build_flowable_start_variables_includes_operator_metadata_aliases() {
    let variables = build_flowable_start_variables(
        "gate_baggage_check",
        "case_001",
        "flight_001",
        &HashMap::new(),
        "desc",
        &HashMap::new(),
        "gate_baggage_check",
        &WorkflowActor {
            actor: "dispatcher".to_string(),
            user_id: Some("user_001".to_string()),
            username: Some("dispatcher".to_string()),
            name_snapshot: Some("调度员甲".to_string()),
            context_type: Some("department".to_string()),
            context_id: Some("dept_001".to_string()),
        },
        Some(Utc::now()),
    );

    assert_eq!(variables.get("startedBy"), Some(&serde_json::json!("dispatcher")));
    assert_eq!(variables.get("flight_id"), Some(&serde_json::json!("flight_001")));
    assert_eq!(variables.get("case_id"), Some(&serde_json::json!("case_001")));
    assert_eq!(variables.get("started_by"), Some(&serde_json::json!("dispatcher")));
    assert_eq!(variables.get("operator"), Some(&serde_json::json!("dispatcher")));
    assert_eq!(variables.get("operator_user_id"), Some(&serde_json::json!("user_001")));
    assert_eq!(
        variables.get("operator_username"),
        Some(&serde_json::json!("dispatcher"))
    );
    assert_eq!(
        variables.get("operator_name_snapshot"),
        Some(&serde_json::json!("调度员甲"))
    );
    assert_eq!(
        variables.get("operator_context_type"),
        Some(&serde_json::json!("department"))
    );
    assert_eq!(
        variables.get("operator_context_id"),
        Some(&serde_json::json!("dept_001"))
    );
    assert!(variables.contains_key("created_at"));
}

#[test]
fn derive_flowable_run_status_keeps_waiting_receipts_without_wait_task_snapshot() {
    let mut run = sample_run();
    run.waiting_task_id = None;

    let status = derive_flowable_run_status(&[], &None, &run, None);

    assert_eq!(status, "waiting_receipts");
}

#[test]
fn derive_flowable_run_status_preserves_completing_case_over_runtime_noise() {
    let mut run = sample_run();
    run.status = "completing_case".to_string();

    let status = derive_flowable_run_status(
        &[serde_json::json!({
            "id": "task_complete",
            "taskDefinitionKey": "complete_business_case"
        })],
        &None,
        &run,
        Some("receipt_001"),
    );

    assert_eq!(status, "completing_case");
}

#[test]
fn resolve_wait_task_does_not_fall_back_to_unrelated_active_task() {
    let mut run = sample_run();
    run.waiting_task_id = None;

    let wait_task = resolve_wait_task(
        &[serde_json::json!({
            "id": "task_notify",
            "taskDefinitionKey": "notify_departments"
        })],
        &run,
    );

    assert!(wait_task.is_none());
}

#[test]
fn build_workflow_start_payload_matches_python_shape() {
    let payload = build_workflow_start_payload(
        "case_001",
        Some("proc_def_001"),
        "gate_baggage_check",
        "db",
        &HashMap::from([("summary".to_string(), serde_json::Value::String("额外说明".to_string()))]),
    );

    assert_eq!(payload.len(), 5);
    assert_eq!(payload["business_key"], "case_001");
    assert_eq!(payload["process_definition_id"], "proc_def_001");
    assert_eq!(payload["process_definition_key"], "gate_baggage_check");
    assert_eq!(payload["bpmn_source"], "db");
    assert_eq!(payload["extra_info"]["summary"], "额外说明");
}

#[test]
fn mark_run_as_system_error_matches_python_failure_semantics() {
    let run = sample_run();

    let failed_run = mark_run_as_system_error(run, "mock executor failure");

    assert_eq!(failed_run.status, "failed");
    assert_eq!(failed_run.outcome.as_deref(), Some("system_error"));
    assert_eq!(failed_run.failed_reason.as_deref(), Some("mock executor failure"));
    assert!(failed_run.completed_at.is_some());
}

#[test]
fn wait_receipt_completion_variables_exclude_receipt_summary_payload() {
    let run = sample_run();
    let variables = build_wait_receipt_completion_variables(&run, "confirmed", None);

    assert_eq!(variables.get("workflowOutcome"), Some(&serde_json::json!("confirmed")));
    assert_eq!(variables.get("receiptGroupId"), Some(&serde_json::json!("receipt_001")));
    assert!(!variables.contains_key("receiptSummary"));
    assert!(!variables.contains_key("failedReason"));
}

#[test]
fn normalize_process_instance_keeps_flowable_payload_unchanged() {
    let process_instance = serde_json::json!({
        "id": "proc_001",
        "businessKey": "case_001",
        "processDefinitionId": "gate_baggage_check:1:def"
    });

    let normalized = normalize_process_instance(
        process_instance.clone(),
        &[serde_json::json!({"id": "task_001"})],
        &serde_json::Map::from_iter([(
            "receiptGroupId".to_string(),
            serde_json::Value::String("rg_001".to_string()),
        )]),
        Some(&serde_json::json!({"id": "wait_001"})),
    );

    assert_eq!(normalized, process_instance);
}

fn make_test_batch_plan_item(case_id: &str, flight_no: &str, extra: &str) -> WorkflowBatchPlanItem {
    let business_case = fms_domain::models::business_case::FlightBusinessCase {
        case_id: case_id.to_string(),
        case_type: "gate_baggage_check".to_string(),
        case_type_name: Some("登机口开包".to_string()),
        flight_id: "FL001".to_string(),
        flight_no: flight_no.to_string(),
        created_at: Utc::now(),
        created_by: "tester".to_string(),
        updated_by: "tester".to_string(),
        description: "test".to_string(),
        status: "PENDING".to_string(),
        stand: None,
        gate: None,
        visibility_scope: fms_domain::models::business_case::VisibilityScope::Common,
        department_id: None,
        department_name_snapshot: None,
        finished_at: None,
        cancelled_at: None,
        log: vec![],
        context: Default::default(),
        workflow_receipt: None,
        terminal_metadata: None,
        append_count: 0,
        latest_append: None,
        append_entries: vec![],
    };
    WorkflowBatchPlanItem {
        item: BusinessCaseWorkflowBatchItem {
            template_code: "gate_baggage_check".to_string(),
            case_id: case_id.to_string(),
        },
        business_case,
        run: BusinessCaseWorkflowRun {
            run_id: format!("run_{}", case_id),
            template_code: "gate_baggage_check".to_string(),
            case_id: case_id.to_string(),
            flight_id: "FL001".to_string(),
            process_definition_key: "gate_baggage_check".to_string(),
            process_instance_id: format!("pi_{}", case_id),
            waiting_task_id: None,
            receipt_group_id: None,
            status: "active".to_string(),
            outcome: None,
            recipient_snapshot: vec![],
            flight_context_snapshot: Default::default(),
            start_payload: Default::default(),
            started_by: "tester".to_string(),
            completed_at: None,
            failed_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        start_snapshot: FlowableStartSnapshot {
            process_instance_id: format!("pi_{}", case_id),
            process_definition_id: None,
            waiting_task_id: None,
            status: "active".to_string(),
        },
        definition: WorkflowRuntimeDefinition {
            case_type: "gate_baggage_check".to_string(),
            notification_task_id: "notify_task".to_string(),
            wait_task_id: "wait_receipts".to_string(),
            notification_title: "test".to_string(),
            notification_body: "test".to_string(),
            notification_severity: "warning".to_string(),
            append_extra_info: false,
            notification_targets: vec![],
            recipient_resolver: WorkflowRecipientResolverConfig {
                source: "department_roles".to_string(),
                empty_policy: "fail".to_string(),
                deduplicate: true,
            },
            receipt_required: true,
            completion_policy: "all_notified_acknowledged".to_string(),
            reject_policy: "fail_on_any_reject".to_string(),
            success_action: WorkflowBusinessCaseAction {
                node_id: "success_node".to_string(),
                action: "complete_case".to_string(),
                target_status: "RESOLVED".to_string(),
                reason_template: None,
                write_finished_at: true,
                require_case_id: true,
            },
            failure_action: WorkflowBusinessCaseAction {
                node_id: "fail_node".to_string(),
                action: "fail_case".to_string(),
                target_status: "REJECTED".to_string(),
                reason_template: None,
                write_finished_at: true,
                require_case_id: true,
            },
            dispatch_tasks: HashMap::new(),
        },
        recipients: vec![],
        notification_title: "test".to_string(),
        notification_body: "test".to_string(),
        receipt_required: true,
        notification_severity: "warning".to_string(),
        extra_info: {
            let mut m = HashMap::new();
            if !extra.is_empty() {
                m.insert("extra_info".to_string(), serde_json::Value::String(extra.to_string()));
            }
            m
        },
        start_payload: HashMap::new(),
        batch_policy: WorkflowBatchPolicy {
            notification_enabled: true,
            receipt_mode: WorkflowBatchReceiptMode::SharedGroup,
        },
    }
}

#[test]
fn build_batch_notification_body_uses_flight_no_and_extra_info() {
    // Test with extra_info
    let items = vec![
        make_test_batch_plan_item("c1", "CZ5352", "座位号 32F"),
        make_test_batch_plan_item("c2", "CZ7714", "座位号 23A"),
    ];
    let body = build_batch_notification_body(&items);
    assert!(body.contains("CZ5352"));
    assert!(body.contains("座位号 32F"));
    assert!(body.contains("CZ7714"));
    assert!(body.contains("座位号 23A"));
    assert!(!body.contains("FL001")); // should not use flight_id

    // Test without extra_info
    let items = vec![
        make_test_batch_plan_item("c1", "CZ5352", ""),
        make_test_batch_plan_item("c2", "CZ7714", ""),
    ];
    let body = build_batch_notification_body(&items);
    assert_eq!(body, "CZ5352\nCZ7714");
}

#[test]
fn workflow_notification_group_key_uses_definition_notification_task_id() {
    let key1 = WorkflowNotificationGroupKey {
        template_code: "gate_baggage_check".to_string(),
        case_type: "gate_baggage_check".to_string(),
        notification_task_id: "notify_gate_baggage".to_string(),
        recipient_set_hash: "u1,u2".to_string(),
        receipt_required: true,
        severity: "warning".to_string(),
    };
    let key2 = WorkflowNotificationGroupKey {
        template_code: "gate_baggage_check".to_string(),
        case_type: "gate_baggage_check".to_string(),
        notification_task_id: "notify_gate_baggage".to_string(),
        recipient_set_hash: "u1,u2".to_string(),
        receipt_required: true,
        severity: "warning".to_string(),
    };
    // Same notification_task_id, same recipients => same hash key
    assert_eq!(key1, key2);

    // Different notification_task_id => different hash key
    let key3 = WorkflowNotificationGroupKey {
        template_code: "gate_baggage_check".to_string(),
        case_type: "gate_baggage_check".to_string(),
        notification_task_id: "pi-1".to_string(), // would be process_instance_id (wrong)
        recipient_set_hash: "u1,u2".to_string(),
        receipt_required: true,
        severity: "warning".to_string(),
    };
    assert_ne!(key1, key3);
}

#[test]
fn compute_recipient_set_hash_is_stable() {
    let recipients = vec![
        [("user_id".to_string(), serde_json::json!("u2"))]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        [("user_id".to_string(), serde_json::json!("u1"))]
            .into_iter()
            .collect::<HashMap<_, _>>(),
    ];
    let hash = compute_recipient_set_hash(&recipients);
    // Output should be sorted: "u1,u2"
    assert_eq!(hash, "u1,u2");
}

#[test]
fn grouped_batch_notification_idempotency_context_is_order_stable() {
    let first = derive_batch_notification_idempotency_context(
        "batch-001",
        "gate_baggage_check",
        "gate_baggage_check",
        "notify_gate_baggage",
        &["case-2".to_string(), "case-1".to_string()],
        &["user-2".to_string(), "user-1".to_string()],
        true,
        "warning",
    );
    let reordered = derive_batch_notification_idempotency_context(
        "batch-001",
        "gate_baggage_check",
        "gate_baggage_check",
        "notify_gate_baggage",
        &["case-1".to_string(), "case-2".to_string()],
        &["user-1".to_string(), "user-2".to_string()],
        true,
        "warning",
    );
    let different_batch = derive_batch_notification_idempotency_context(
        "batch-002",
        "gate_baggage_check",
        "gate_baggage_check",
        "notify_gate_baggage",
        &["case-1".to_string(), "case-2".to_string()],
        &["user-1".to_string(), "user-2".to_string()],
        true,
        "warning",
    );

    assert_eq!(first, reordered);
    assert_ne!(first.notification_id_seed, different_batch.notification_id_seed);
    assert_ne!(
        first.receipt_group_id_override,
        different_batch.receipt_group_id_override
    );
    assert_eq!(first.receipt_group_id_override.as_deref().unwrap().len(), 26);
}

#[test]
fn per_case_batch_notification_idempotency_context_uses_batch_case_and_task() {
    let first = derive_per_case_batch_notification_idempotency_context(
        "batch-001",
        "case-001",
        "gate_baggage_check",
        "notify_gate_baggage",
        false,
    );
    let repeated = derive_per_case_batch_notification_idempotency_context(
        "batch-001",
        "case-001",
        "gate_baggage_check",
        "notify_gate_baggage",
        false,
    );
    let different_batch = derive_per_case_batch_notification_idempotency_context(
        "batch-002",
        "case-001",
        "gate_baggage_check",
        "notify_gate_baggage",
        false,
    );

    assert_eq!(first, repeated);
    assert_ne!(first.notification_id_seed, different_batch.notification_id_seed);
    assert!(first.receipt_group_id_override.is_none());
}

#[test]
fn three_same_group_items_produce_same_group_key() {
    let recipients = vec![[("user_id".to_string(), serde_json::json!("u1"))]
        .into_iter()
        .collect::<HashMap<_, _>>()];
    let hash = compute_recipient_set_hash(&recipients);

    let make_key = || WorkflowNotificationGroupKey {
        template_code: "gate_baggage_check".to_string(),
        case_type: "gate_baggage_check".to_string(),
        notification_task_id: "notify_gate_baggage".to_string(),
        recipient_set_hash: hash.clone(),
        receipt_required: true,
        severity: "warning".to_string(),
    };

    let key1 = make_key();
    let key2 = make_key();
    let key3 = make_key();
    assert_eq!(key1, key2);
    assert_eq!(key2, key3);

    // Verify they hash to the same bucket
    let mut map = std::collections::HashMap::new();
    map.entry(key1).or_insert_with(Vec::new).push("item1");
    map.entry(key2).or_insert_with(Vec::new).push("item2");
    map.entry(key3).or_insert_with(Vec::new).push("item3");
    assert_eq!(map.len(), 1, "All 3 items should be in the same group");
    assert_eq!(map.values().next().unwrap().len(), 3);
}

#[test]
fn workflow_batch_policy_requires_explicit_case_properties() {
    let default_policy = parse_workflow_batch_policy(&serde_json::json!({}));
    assert!(!default_policy.notification_enabled);
    assert!(!default_policy.should_group(true));
    assert!(!default_policy.should_group(false));

    let shared = parse_workflow_batch_policy(&serde_json::json!({
        "workflow_policy": {
            "batch_notification_enabled": true,
            "batch_receipt_mode": "shared_group"
        }
    }));
    assert_eq!(shared.receipt_mode, WorkflowBatchReceiptMode::SharedGroup);
    assert!(shared.should_group(true));
    assert!(shared.should_group(false));

    let per_case = parse_workflow_batch_policy(&serde_json::json!({
        "workflow_policy": {
            "batch_notification_enabled": true,
            "batch_receipt_mode": "per_case"
        }
    }));
    assert_eq!(per_case.receipt_mode, WorkflowBatchReceiptMode::PerCase);
    assert!(!per_case.should_group(true));
    assert!(per_case.should_group(false));
}

#[test]
fn build_batch_notification_body_three_items_with_flight_no_and_extra_info() {
    let items = vec![
        make_test_batch_plan_item("c1", "7714", "座位号23A"),
        make_test_batch_plan_item("c2", "5352", "座位号32F"),
        make_test_batch_plan_item("c3", "6333", "座位号1A"),
    ];
    let body = build_batch_notification_body(&items);
    assert!(body.contains("7714"));
    assert!(body.contains("座位号23A"));
    assert!(body.contains("5352"));
    assert!(body.contains("座位号32F"));
    assert!(body.contains("6333"));
    assert!(body.contains("座位号1A"));
    // Should NOT contain flight_id "FL001"
    assert!(!body.contains("FL001"));
}

#[derive(Default)]
struct FakeWorkflowRunRepository {
    runs: std::sync::Mutex<Vec<BusinessCaseWorkflowRun>>,
}

#[async_trait::async_trait]
impl fms_domain::ports::business_case_workflow_run_repository::BusinessCaseWorkflowRunRepository
    for FakeWorkflowRunRepository
{
    async fn save(&self, run: &BusinessCaseWorkflowRun) -> Result<BusinessCaseWorkflowRun, DomainError> {
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|r| r.run_id != run.run_id);
        runs.push(run.clone());
        Ok(run.clone())
    }

    async fn find_by_run_id(&self, run_id: &str) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        Ok(self.runs.lock().unwrap().iter().find(|r| r.run_id == run_id).cloned())
    }

    async fn find_by_case_id(&self, case_id: &str) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        Ok(self.runs.lock().unwrap().iter().find(|r| r.case_id == case_id).cloned())
    }

    async fn find_by_receipt_group_id(
        &self,
        receipt_group_id: &str,
    ) -> Result<Option<BusinessCaseWorkflowRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.receipt_group_id.as_deref() == Some(receipt_group_id))
            .cloned())
    }

    async fn list_by_receipt_group_id(
        &self,
        receipt_group_id: &str,
    ) -> Result<Vec<BusinessCaseWorkflowRun>, DomainError> {
        Ok(self
            .runs
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.receipt_group_id.as_deref() == Some(receipt_group_id))
            .cloned()
            .collect())
    }
}

#[tokio::test]
async fn sync_receipt_group_syncs_all_case_ids() {
    let repo = FakeWorkflowRunRepository::default();
    let shared_receipt_group_id = "rg_shared_001";
    let now = Utc::now();

    // Save 3 runs with the same receipt_group_id but different case_ids
    for i in 1..=3 {
        let run = BusinessCaseWorkflowRun {
            run_id: format!("run_{i}"),
            template_code: "gate_baggage_check".to_string(),
            case_id: format!("case_{i}"),
            flight_id: "FL001".to_string(),
            process_definition_key: "gate_baggage_check".to_string(),
            process_instance_id: format!("pi_{i}"),
            waiting_task_id: Some("wait_receipts".to_string()),
            receipt_group_id: Some(shared_receipt_group_id.to_string()),
            status: "notification_sent".to_string(),
            outcome: None,
            recipient_snapshot: vec![],
            flight_context_snapshot: HashMap::new(),
            start_payload: HashMap::new(),
            started_by: "tester".to_string(),
            completed_at: None,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        };
        repo.save(&run).await.unwrap();
    }

    // Verify list_by_receipt_group_id returns all 3 runs
    let runs = repo.list_by_receipt_group_id(shared_receipt_group_id).await.unwrap();
    assert_eq!(runs.len(), 3);
    let case_ids: Vec<String> = runs.iter().map(|r| r.case_id.clone()).collect();
    assert!(case_ids.contains(&"case_1".to_string()));
    assert!(case_ids.contains(&"case_2".to_string()));
    assert!(case_ids.contains(&"case_3".to_string()));
}
