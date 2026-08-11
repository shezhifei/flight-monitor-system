use super::batch::build_copilot_service;
use super::*;

async fn test_draft_catalog_includes_common_for_normal_user_with_read_permission() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

    // Common AI-enabled case type
    fake_repo
        .save(&BusinessCaseType {
            id: "common_ai".to_string(),
            code: "common_ai".to_string(),
            name: "Common AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": true }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    // Department AI-enabled case type
    fake_repo
        .save(&BusinessCaseType {
            id: "ops_ai".to_string(),
            code: "ops_ai".to_string(),
            name: "Ops AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Department,
            department_id: Some("ops-1".to_string()),
            department_name_snapshot: Some("ops".to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": true }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);

    // Normal user with read permission (include_common_case_types = true)
    // Should see both Common and department-specific AI-enabled types
    let catalog = service
        .load_case_type_catalog(Some("ops-1"), Some("ops"), true)
        .await
        .unwrap();
    let mut codes: Vec<String> = catalog.into_iter().map(|entry| entry.code).collect();
    codes.sort();
    assert_eq!(codes, vec!["common_ai".to_string(), "ops_ai".to_string()]);
}

#[tokio::test]
async fn test_commit_allows_common_ai_type_with_business_case_create_scope() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

    // Common AI-enabled case type
    fake_repo
        .save(&BusinessCaseType {
            id: "common_ai".to_string(),
            code: "common_ai".to_string(),
            name: "Common AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": true }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    // Department AI-enabled case type
    fake_repo
        .save(&BusinessCaseType {
            id: "ops_ai".to_string(),
            code: "ops_ai".to_string(),
            name: "Ops AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Department,
            department_id: Some("ops-1".to_string()),
            department_name_snapshot: Some("ops".to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": true }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);

    // User with business_case.create common scope (include_common_case_types = true)
    // Should see Common AI-enabled type in catalog for commit
    let catalog = service
        .load_case_type_catalog(Some("ops-1"), Some("ops"), true)
        .await
        .unwrap();
    let mut codes: Vec<String> = catalog.into_iter().map(|entry| entry.code).collect();
    codes.sort();
    assert_eq!(codes, vec!["common_ai".to_string(), "ops_ai".to_string()]);

    // User without common scope (include_common_case_types = false)
    // Should NOT see Common type
    let catalog_no_common = service
        .load_case_type_catalog(Some("ops-1"), Some("ops"), false)
        .await
        .unwrap();
    let codes_no_common: Vec<String> = catalog_no_common.into_iter().map(|entry| entry.code).collect();
    assert_eq!(codes_no_common, vec!["ops_ai".to_string()]);
}

#[tokio::test]
async fn test_commit_rejects_invisible_department_type() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

    // Tech department AI-enabled case type
    fake_repo
        .save(&BusinessCaseType {
            id: "tech_ai".to_string(),
            code: "tech_ai".to_string(),
            name: "Tech AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Department,
            department_id: Some("tech-1".to_string()),
            department_name_snapshot: Some("tech".to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": true }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);

    let batch_id = "test_batch_invisible".to_string();
    let batch = AiCopilotBusinessCaseBatch {
        batch_id: batch_id.clone(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: "summary".to_string(),
        transcript_text: "text".to_string(),
        draft_actions: serde_json::json!([]),
        status: AiCopilotBatchStatus::Draft,
        created_by: "tester".to_string(),
        committed_case_ids: vec![],
        idempotency_key: None,
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: None,
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: None,
        workflow_dispatch_status: "not_required".to_string(),
        workflow_dispatch_request: None,
        workflow_dispatch_error: None,
        workflow_dispatch_attempts: 0,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    service.repo.save(&batch).await.unwrap();

    let commit_req = AiCopilotCommitRequest {
        idempotency_key: None,
        actions: vec![AiCopilotApprovedAction {
            action_id: "act_1".to_string(),
            case_type: "tech_ai".to_string(),
            flight_id: "flight-1".to_string(),
            flight_no: "CZ1234".to_string(),
            bound_leg_type: Some("outbound".to_string()),
            bound_flight_no: None,
            description: None,
            remarks: None,
            fields: serde_json::json!({}),
            status: None,
        }],
    };

    // Ops user trying to commit Tech department type - should be rejected
    let res = service
        .commit_batch(
            &batch_id,
            commit_req,
            batch_access("tester"),
            WorkflowActor {
                actor: "tester".to_string(),
                ..Default::default()
            },
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
            false,
        )
        .await;

    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        matches!(err, DomainError::ValidationError(_)),
        "Expected ValidationError, got: {:?}",
        err
    );
    assert!(err
        .to_string()
        .contains("不在当前用户的 AI 抽取授权目录中，或未启用 AI 抽取"));
}

#[tokio::test]
async fn test_commit_rejects_non_ai_enabled_type() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

    // Non-AI-enabled case type
    fake_repo
        .save(&BusinessCaseType {
            id: "ops_no_ai".to_string(),
            code: "ops_no_ai".to_string(),
            name: "Ops No AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Department,
            department_id: Some("ops-1".to_string()),
            department_name_snapshot: Some("ops".to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": false }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);

    let batch_id = "test_batch_no_ai".to_string();
    let batch = AiCopilotBusinessCaseBatch {
        batch_id: batch_id.clone(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: "summary".to_string(),
        transcript_text: "text".to_string(),
        draft_actions: serde_json::json!([]),
        status: AiCopilotBatchStatus::Draft,
        created_by: "tester".to_string(),
        committed_case_ids: vec![],
        idempotency_key: None,
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: None,
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: None,
        workflow_dispatch_status: "not_required".to_string(),
        workflow_dispatch_request: None,
        workflow_dispatch_error: None,
        workflow_dispatch_attempts: 0,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    service.repo.save(&batch).await.unwrap();

    let commit_req = AiCopilotCommitRequest {
        idempotency_key: None,
        actions: vec![AiCopilotApprovedAction {
            action_id: "act_1".to_string(),
            case_type: "ops_no_ai".to_string(),
            flight_id: "flight-1".to_string(),
            flight_no: "CZ1234".to_string(),
            bound_leg_type: Some("outbound".to_string()),
            bound_flight_no: None,
            description: None,
            remarks: None,
            fields: serde_json::json!({}),
            status: None,
        }],
    };

    // Should reject non-AI-enabled type
    let res = service
        .commit_batch(
            &batch_id,
            commit_req,
            batch_access("tester"),
            WorkflowActor {
                actor: "tester".to_string(),
                ..Default::default()
            },
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
            false,
        )
        .await;

    assert!(res.is_err());
    let err = res.err().unwrap();
    assert!(
        matches!(err, DomainError::ValidationError(_)),
        "Expected ValidationError, got: {:?}",
        err
    );
    assert!(err
        .to_string()
        .contains("不在当前用户的 AI 抽取授权目录中，或未启用 AI 抽取"));
}

#[tokio::test]
async fn test_try_begin_commit_prevents_concurrent_submit() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    fake_repo
        .save(&BusinessCaseType {
            id: "common_ai".to_string(),
            code: "common_ai".to_string(),
            name: "Common AI".to_string(),
            bpmn_xml: None,
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({ "enabled": true }),
            case_properties: serde_json::json!({}),
        })
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);

    let batch_id = "test_batch_concurrent".to_string();
    let batch = AiCopilotBusinessCaseBatch {
        batch_id: batch_id.clone(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: "summary".to_string(),
        transcript_text: "text".to_string(),
        draft_actions: serde_json::json!([]),
        status: AiCopilotBatchStatus::Draft,
        created_by: "tester".to_string(),
        committed_case_ids: vec![],
        idempotency_key: None,
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: None,
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: None,
        workflow_dispatch_status: "not_required".to_string(),
        workflow_dispatch_request: None,
        workflow_dispatch_error: None,
        workflow_dispatch_attempts: 0,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    service.repo.save(&batch).await.unwrap();

    // First request acquires the lock
    let result1 = service.repo.try_begin_commit(&batch_id).await.unwrap();
    assert!(matches!(result1, BeginCommitResult::Acquired(_)));

    // Second request should get Conflict (batch is now 'committing')
    let result2 = service.repo.try_begin_commit(&batch_id).await.unwrap();
    assert!(matches!(result2, BeginCommitResult::Conflict(_)));
}

#[tokio::test]
async fn test_commit_saga_base_state_is_persisted_and_recoverable() {
    let service = build_copilot_service(FakeBusinessCaseTypeRepo::default());
    let batch_id = "test_batch_commit_saga_base".to_string();
    let mut batch = test_copilot_batch(&batch_id, "tester", AiCopilotBatchStatus::Draft);
    batch.updated_at = Utc::now() - Duration::minutes(20);
    service.repo.save(&batch).await.unwrap();

    let request = json!({
        "idempotency_key": "commit-saga-base",
        "actions": [{"action_id": "act_1"}]
    });
    let next_recovery_at = Utc::now() - Duration::minutes(1);
    let acquired = service
        .repo
        .try_begin_commit_with_request(&batch_id, &request, Some(next_recovery_at))
        .await
        .unwrap();

    let BeginCommitResult::Acquired(acquired) = acquired else {
        panic!("expected commit acquisition");
    };
    assert_eq!(acquired.status, AiCopilotBatchStatus::Committing);
    assert_eq!(acquired.commit_request.as_ref(), Some(&request));
    assert_eq!(acquired.created_action_case_ids, json!({}));
    assert_eq!(acquired.commit_attempts, 1);
    assert!(acquired.commit_started_at.is_some());

    let recorded = service
        .repo
        .record_created_action_case(&batch_id, "act_1", "case-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recorded.created_action_case_ids["act_1"], "case-1");

    let recovered = service.repo.recover_stale_committing(Utc::now(), 10).await.unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].batch_id, batch_id);
    assert_eq!(recovered[0].status, AiCopilotBatchStatus::Committing);
    assert_eq!(recovered[0].commit_attempts, 2);
    assert!(recovered[0].commit_next_recovery_at.unwrap() > Utc::now());
}

#[tokio::test]
async fn test_mark_committed_only_from_committing_state() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    let service = build_copilot_service(fake_repo);

    let batch_id = "test_batch_state_guard".to_string();
    let batch = AiCopilotBusinessCaseBatch {
        batch_id: batch_id.clone(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: "summary".to_string(),
        transcript_text: "text".to_string(),
        draft_actions: serde_json::json!([]),
        status: AiCopilotBatchStatus::Draft,
        created_by: "tester".to_string(),
        committed_case_ids: vec![],
        idempotency_key: None,
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: None,
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: None,
        workflow_dispatch_status: "not_required".to_string(),
        workflow_dispatch_request: None,
        workflow_dispatch_error: None,
        workflow_dispatch_attempts: 0,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    service.repo.save(&batch).await.unwrap();

    // mark_committed on a draft batch should fail (return None)
    let result = service
        .repo
        .mark_committed(&batch_id, &["c1".to_string()], &json!([]), None)
        .await
        .unwrap();
    assert!(result.is_none(), "Should not mark draft batch as committed directly");

    // Acquire lock first
    let acquired = service.repo.try_begin_commit(&batch_id).await.unwrap();
    assert!(matches!(acquired, BeginCommitResult::Acquired(_)));

    // Now mark_committed should succeed
    let result = service
        .repo
        .mark_committed(
            &batch_id,
            &["c1".to_string()],
            &json!([{"group_id":"g1","case_type":"case","case_ids":["c1"],"title":"t","body":"b"}]),
            None,
        )
        .await
        .unwrap();
    assert!(result.is_some());
    let committed = result.unwrap();
    assert_eq!(committed.status, AiCopilotBatchStatus::Committed);
    assert_eq!(committed.committed_case_ids, vec!["c1".to_string()]);
    assert_eq!(committed.notification_groups[0]["group_id"], "g1");

    // Already committed - try_begin_commit returns AlreadyCommitted
    let result = service.repo.try_begin_commit(&batch_id).await.unwrap();
    assert!(matches!(result, BeginCommitResult::AlreadyCommitted(_)));
}

#[tokio::test]
async fn test_workflow_dispatch_state_is_queryable_and_retry_snapshot_validated() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    let service = build_copilot_service(fake_repo);

    let batch_id = "test_workflow_dispatch_state".to_string();
    let batch = AiCopilotBusinessCaseBatch {
        batch_id: batch_id.clone(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: "summary".to_string(),
        transcript_text: "text".to_string(),
        draft_actions: serde_json::json!([]),
        status: AiCopilotBatchStatus::Committed,
        created_by: "tester".to_string(),
        committed_case_ids: vec!["case-1".to_string()],
        idempotency_key: Some("idem-1".to_string()),
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: None,
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: Some(Utc::now()),
        workflow_dispatch_status: "failed".to_string(),
        workflow_dispatch_request: Some(json!({
            "items": [{"template_code": "gate_baggage_check", "case_id": "case-1"}],
            "case_ids": ["case-1"]
        })),
        workflow_dispatch_error: Some(json!({"stage":"attach_workflow_batch"})),
        workflow_dispatch_attempts: 1,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    service.repo.save(&batch).await.unwrap();

    let filtered = service
        .list_batches(None, Some("failed"), 10, 0, batch_access("tester"))
        .await
        .unwrap();
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(filtered.items[0].batch_id, batch_id);

    let due = service.repo.list_due_workflow_dispatch_retries(10, 5).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].batch_id, batch_id);

    let metrics = service.operational_metrics(5, 10).await.unwrap();
    assert_eq!(metrics.batch_status.committed, 1);
    assert_eq!(metrics.workflow_dispatch.failed, 1);
    assert_eq!(metrics.workflow_dispatch.retry_due, 1);
    assert_eq!(metrics.recent_errors.len(), 1);
    assert_eq!(metrics.recent_errors[0].stage.as_deref(), Some("attach_workflow_batch"));

    let items = workflow_items_from_dispatch_request(batch.workflow_dispatch_request.as_ref().unwrap()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].template_code, "gate_baggage_check");
    assert_eq!(items[0].case_id, "case-1");

    let invalid = workflow_items_from_dispatch_request(&json!({"items":[{"case_id":"case-1"}]}));
    assert!(invalid.is_err());
}

#[tokio::test]
async fn test_failed_batch_resolution_paths() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    let service = build_copilot_service(fake_repo);

    let batch_id = "test_failed_batch_resolution".to_string();
    let batch = AiCopilotBusinessCaseBatch {
        batch_id: batch_id.clone(),
        entity_id: "flight-monitor-copilot".to_string(),
        source_page: "flight_monitor".to_string(),
        transcript_summary: "summary".to_string(),
        transcript_text: "text".to_string(),
        draft_actions: serde_json::json!([]),
        status: AiCopilotBatchStatus::Failed,
        created_by: "tester".to_string(),
        committed_case_ids: vec![],
        idempotency_key: None,
        notification_groups: json!([]),
        commit_request: None,
        created_action_case_ids: json!({}),
        commit_error: Some(json!({"stage":"attach_workflow_batch"})),
        commit_started_at: None,
        commit_attempts: 0,
        commit_next_recovery_at: None,
        committed_at: None,
        workflow_dispatch_status: "failed".to_string(),
        workflow_dispatch_request: None,
        workflow_dispatch_error: Some(json!({"stage":"attach_workflow_batch"})),
        workflow_dispatch_attempts: 1,
        workflow_dispatch_next_retry_at: None,
        workflow_dispatched_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };
    service.repo.save(&batch).await.unwrap();

    let reset = service
        .resolve_failed_batch(
            &batch_id,
            AiCopilotFailedBatchResolutionRequest {
                action: AiCopilotFailedBatchResolutionAction::ResetToDraft,
                note: Some("retry after provider recovery".to_string()),
            },
            "tester",
            ops_batch_access(),
        )
        .await
        .unwrap();
    assert_eq!(reset.status, AiCopilotBatchStatus::Draft);
    assert_eq!(reset.committed_case_ids.len(), 0);
    assert_eq!(reset.commit_error.unwrap()["resolution"], "reset_to_draft");

    let batch_id = "test_failed_batch_ack".to_string();
    let mut batch = batch;
    batch.batch_id = batch_id.clone();
    batch.status = AiCopilotBatchStatus::Failed;
    batch.committed_case_ids = vec!["case-1".to_string()];
    service.repo.save(&batch).await.unwrap();

    let reset_err = service
        .resolve_failed_batch(
            &batch_id,
            AiCopilotFailedBatchResolutionRequest {
                action: AiCopilotFailedBatchResolutionAction::ResetToDraft,
                note: None,
            },
            "tester",
            ops_batch_access(),
        )
        .await
        .expect_err("partial cases cannot be reset automatically");
    assert!(reset_err.to_string().contains("已有部分业务事项"));

    let resolved = service
        .resolve_failed_batch(
            &batch_id,
            AiCopilotFailedBatchResolutionRequest {
                action: AiCopilotFailedBatchResolutionAction::MarkResolved,
                note: Some("handled manually".to_string()),
            },
            "tester",
            ops_batch_access(),
        )
        .await
        .unwrap();
    assert_eq!(resolved.status, AiCopilotBatchStatus::FailedResolved);
    assert_eq!(resolved.committed_case_ids, vec!["case-1".to_string()]);
}

#[test]
fn test_case_flight_match_policy_optional_fields() {
    // All None => default
    let policy = CaseFlightMatchPolicy::default();
    assert!(policy.exclude_cancelled.is_none());
    assert!(policy.exclude_departed.is_none());
    assert!(policy.exclude_actual_departure.is_none());
    assert!(policy.time_window_hours_before.is_none());
    assert!(policy.time_window_hours_after.is_none());
    assert!(policy.min_auto_match_score.is_none());

    // Some values deserialize correctly
    let raw = serde_json::json!({
        "exclude_cancelled": false,
        "exclude_departed": false,
        "time_window_hours_before": 6,
        "min_auto_match_score": 0.72
    });
    let parsed: CaseFlightMatchPolicy = serde_json::from_value(raw).unwrap();
    assert_eq!(parsed.exclude_cancelled, Some(false));
    assert_eq!(parsed.exclude_departed, Some(false));
    assert_eq!(parsed.exclude_actual_departure, None);
    assert_eq!(parsed.time_window_hours_before, Some(6));
    assert_eq!(parsed.time_window_hours_after, None);
    assert_eq!(parsed.min_auto_match_score, Some(0.72));
}

#[test]
fn test_merge_copilot_flight_binding_case_properties_override() {
    // AI config with defaults
    let ai_matching = AiFlightMatchingConfig {
        allow_numeric_suffix: Some(true),
        prefer_leg: Some("outbound".to_string()),
        exclude_cancelled: Some(true),
        exclude_departed: Some(true),
        exclude_actual_departure: Some(true),
        window_hours_before: Some(3),
        window_hours_after: Some(8),
        min_auto_match_score: Some(0.85),
    };
    let ai_leg = AiLegBindingConfig {
        allowed: vec!["outbound".to_string()],
        default: Some("outbound".to_string()),
        required: false,
    };

    // case_properties overrides with stricter settings
    let case_props = BusinessCaseProperties {
        binding_policy: CaseBindingPolicy {
            allowed_leg_types: vec!["outbound".to_string(), "inbound".to_string()],
            default_leg_type: Some("inbound".to_string()),
            leg_type_required: true,
            flight_match_policy: CaseFlightMatchPolicy {
                exclude_cancelled: Some(false), // override: allow cancelled
                exclude_departed: None,         // no override
                exclude_actual_departure: None,
                time_window_hours_before: Some(6), // override
                time_window_hours_after: None,
                min_auto_match_score: Some(0.72), // override
                allow_numeric_suffix: Some(false),
            },
            flight_required: false,
        },
        ..Default::default()
    };

    let (merged_matching, merged_leg) = merge_copilot_flight_binding(&ai_matching, &ai_leg, &case_props);

    // case_properties overrides exclude_cancelled
    assert_eq!(merged_matching.exclude_cancelled, Some(false));
    assert_eq!(merged_matching.allow_numeric_suffix, Some(false));
    // no override from case_properties => fallback to AI config
    assert_eq!(merged_matching.exclude_departed, Some(true));
    assert_eq!(merged_matching.exclude_actual_departure, Some(true));
    // case_properties overrides time_window
    assert_eq!(merged_matching.window_hours_before, Some(6));
    // no override => AI config
    assert_eq!(merged_matching.window_hours_after, Some(8));
    // case_properties overrides min_score
    assert_eq!(merged_matching.min_auto_match_score, Some(0.72));

    // leg binding uses case_properties allowed_leg_types
    assert_eq!(merged_leg.allowed, vec!["outbound".to_string(), "inbound".to_string()]);
    // default_leg_type from case_properties
    assert_eq!(merged_leg.default, Some("inbound".to_string()));
    // required: case_properties true
    assert!(merged_leg.required);
}

#[test]
fn test_merge_copilot_flight_binding_no_case_properties_fallback_to_ai() {
    let ai_matching = AiFlightMatchingConfig {
        allow_numeric_suffix: Some(true),
        prefer_leg: None,
        exclude_cancelled: Some(true),
        exclude_departed: None,
        exclude_actual_departure: None,
        window_hours_before: Some(2),
        window_hours_after: Some(4),
        min_auto_match_score: Some(0.9),
    };
    let ai_leg = AiLegBindingConfig {
        allowed: vec!["outbound".to_string()],
        default: Some("outbound".to_string()),
        required: true,
    };

    let case_props = BusinessCaseProperties::default(); // nothing set

    let (merged_matching, merged_leg) = merge_copilot_flight_binding(&ai_matching, &ai_leg, &case_props);

    // all fall back to AI config
    assert_eq!(merged_matching.exclude_cancelled, Some(true));
    assert_eq!(merged_matching.allow_numeric_suffix, Some(true));
    assert_eq!(merged_matching.exclude_departed, None);
    assert_eq!(merged_matching.window_hours_before, Some(2));
    assert_eq!(merged_matching.min_auto_match_score, Some(0.9));
    assert_eq!(merged_leg.allowed, vec!["outbound".to_string()]);
    assert_eq!(merged_leg.default, Some("outbound".to_string()));
    assert!(merged_leg.required);
}

#[test]
fn test_case_properties_outbound_only_filters_inbound_flights() {
    use fms_domain::models::flight_leg::{FlightLeg, FlightTypeCode, LegType};
    use fms_domain::models::value_objects::{FlightId, FlightNumber, FlightStatus};

    let now = Utc::now();

    // Flight with both inbound and outbound legs
    let flight = Flight {
        flight_id: FlightId::from("FL100"),
        airline_code: Some("CA".to_string()),
        flight_number: Some(FlightNumber::from("CA9999")),
        registration: None,
        aircraft_type_detail: None,
        stand: None,
        gate: None,
        terminal: None,
        position: None,
        baggage_carousel: None,
        scheduled_departure: Some(now + Duration::hours(2)),
        scheduled_arrival: Some(now + Duration::hours(1)),
        estimated_departure: Some(now + Duration::hours(2)),
        estimated_arrival: Some(now + Duration::hours(1)),
        actual_departure: None,
        actual_arrival: None,
        cobt_time: None,
        codt: None,
        has_boarding_restriction: false,
        is_quick_turnaround: false,
        is_commercial_signed: true,
        status: FlightStatus::Scheduled,
        inbound_leg: Some(FlightLeg {
            leg_type: LegType::Inbound,
            flight_no: "CA8888".to_string(),
            flight_type: FlightTypeCode::Domestic,
            mission: None,
            origin_code: Some("PVG".to_string()),
            destination_code: Some("PEK".to_string()),
            origin_name: None,
            destination_name: None,
            is_vip: false,
            stand_type: None,
            scheduled_time: Some(now + Duration::hours(1)),
        }),
        outbound_leg: Some(FlightLeg {
            leg_type: LegType::Outbound,
            flight_no: "CA9999".to_string(),
            flight_type: FlightTypeCode::Domestic,
            mission: None,
            origin_code: Some("PEK".to_string()),
            destination_code: Some("PVG".to_string()),
            origin_name: None,
            destination_name: None,
            is_vip: false,
            stand_type: None,
            scheduled_time: Some(now + Duration::hours(2)),
        }),
        anomaly_summary: Default::default(),
        created_at: now,
        updated_at: now,
        version: 1,
        labels: vec![],
        flight_remarks: None,
        load_planning_remarks: None,
        aircraft_maintenance_remarks: None,
        aircraft_check_remarks: None,
        direction: None,
        flight_kind: "passenger".to_string(),
        is_draft: false,
        divert: false,
    };

    let config = AiFlightMatchingConfig {
        allow_numeric_suffix: Some(true),
        window_hours_before: Some(3),
        window_hours_after: Some(8),
        exclude_cancelled: Some(true),
        exclude_departed: Some(true),
        exclude_actual_departure: Some(true),
        prefer_leg: None,
        min_auto_match_score: None,
    };

    // Outbound-only binding (from case_properties)
    let leg_binding_outbound = AiLegBindingConfig {
        allowed: vec!["outbound".to_string()],
        default: Some("outbound".to_string()),
        required: true,
    };

    // Both legs allowed (from ai_extraction_config fallback)
    let leg_binding_both = AiLegBindingConfig {
        allowed: vec!["outbound".to_string(), "inbound".to_string()],
        default: Some("outbound".to_string()),
        required: true,
    };

    // With outbound-only: should match outbound CA9999
    let matched_outbound = match_flight(&flight, "CA9999", &config, &leg_binding_outbound);
    assert!(matched_outbound.is_some());
    assert_eq!(matched_outbound.unwrap().leg_type, "outbound");

    // With outbound-only: should NOT match inbound CA8888
    let matched_inbound_restricted = match_flight(&flight, "CA8888", &config, &leg_binding_outbound);
    assert!(
        matched_inbound_restricted.is_none(),
        "Outbound-only binding should not match inbound flight"
    );

    // With both legs allowed: should match inbound CA8888
    let matched_inbound_allowed = match_flight(&flight, "CA8888", &config, &leg_binding_both);
    assert!(matched_inbound_allowed.is_some());
    assert_eq!(matched_inbound_allowed.unwrap().leg_type, "inbound");
}

#[test]
fn test_merge_copilot_flight_binding_outbound_only_from_case_properties() {
    // AI config allows both inbound and outbound
    let ai_matching = AiFlightMatchingConfig {
        allow_numeric_suffix: Some(true),
        prefer_leg: None,
        exclude_cancelled: Some(true),
        exclude_departed: Some(true),
        exclude_actual_departure: Some(true),
        window_hours_before: Some(3),
        window_hours_after: Some(8),
        min_auto_match_score: Some(0.85),
    };
    let ai_leg = AiLegBindingConfig {
        allowed: vec!["outbound".to_string(), "inbound".to_string()],
        default: Some("outbound".to_string()),
        required: false,
    };

    // case_properties restricts to outbound only
    let case_props = BusinessCaseProperties {
        binding_policy: CaseBindingPolicy {
            allowed_leg_types: vec!["outbound".to_string()],
            default_leg_type: Some("outbound".to_string()),
            leg_type_required: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let (_, merged_leg) = merge_copilot_flight_binding(&ai_matching, &ai_leg, &case_props);

    // case_properties overrides: only outbound allowed
    assert_eq!(merged_leg.allowed, vec!["outbound".to_string()]);
    assert_eq!(merged_leg.default, Some("outbound".to_string()));
    assert!(merged_leg.required);
}

// --- E2E REGRESSION HARNESS FOR AUTO COPILOT VOICE ---

pub(super) const CANONICAL_GATE_BAGGAGE_TRANSCRIPT: &str = "A:调度，有3个登机口开包；\nB:请讲；\nA:7714,座位号23A，5352，座位号32F，6333，座位号1A\nB:7714,座位号23A，5352，座位号32F，6333，座位号1A,三个开包收到了。";

pub(super) const CANONICAL_GATE_BAGGAGE_LLM_JSON: &str = r#"{
  "summary": "三个航班登机口开包，座位号分别为23A、32F、1A",
  "actions": [
    {
      "case_type": "gate_baggage_check",
      "case_type_name": "登机口开包",
      "flight_number_raw": "7714",
      "leg_type_hint": "outbound",
      "description": "登机口开包，座位号23A",
      "remarks": "座位号23A",
      "fields": {"seat_no": "23A"},
      "confidence": 0.95
    },
    {
      "case_type": "gate_baggage_check",
      "case_type_name": "登机口开包",
      "flight_number_raw": "5352",
      "leg_type_hint": "outbound",
      "description": "登机口开包，座位号32F",
      "remarks": "座位号32F",
      "fields": {"seat_no": "32F"},
      "confidence": 0.95
    },
    {
      "case_type": "gate_baggage_check",
      "case_type_name": "登机口开包",
      "flight_number_raw": "6333",
      "leg_type_hint": "outbound",
      "description": "登机口开包，座位号1A",
      "remarks": "座位号1A",
      "fields": {"seat_no": "1A"},
      "confidence": 0.95
    }
  ]
}"#;

pub(super) const CANONICAL_GATE_BAGGAGE_BPMN_XML: &str = r#"
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:fm="http://flight-monitor/schema/bpmn">
  <bpmn:process id="gate_baggage_check" isExecutable="true">
<bpmn:extensionElements>
  <fm:workflowTemplate templateCode="gate_baggage_check" caseType="登机口开包" />
</bpmn:extensionElements>
<bpmn:userTask id="notify_departments">
  <bpmn:extensionElements>
    <fm:notificationRule action="dispatch_notify" severity="critical" receiptRequired="true" appendExtraInfo="true" title="通知 ${flight_no}" bodyTemplate="航班 ${flight_no}">
      <fm:targets>
        <fm:target department="运行控制" roles="dispatcher" />
      </fm:targets>
    </fm:notificationRule>
    <fm:receiptRule completionPolicy="all_notified_acknowledged" rejectPolicy="fail_on_any_reject" />
    <fm:recipientResolver source="department_roles" emptyPolicy="skip" deduplicate="true" />
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

impl FakeBusinessCaseTypeRepo {
    pub fn with_gate_baggage_check() -> Self {
        let repo = Self::default();
        let catalog = BusinessCaseType {
            id: "gate_baggage_check".to_string(),
            code: "gate_baggage_check".to_string(),
            name: "登机口开包".to_string(),
            bpmn_xml: Some(CANONICAL_GATE_BAGGAGE_BPMN_XML.to_string()),
            description: None,
            is_active: true,
            visibility_scope: VisibilityScope::Common,
            department_id: None,
            department_name_snapshot: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            ai_extraction_config: serde_json::json!({
                "enabled": true,
                "aliases": ["登机口开包", "开包"],
                "leg_binding": {
                    "allowed": ["outbound"],
                    "default": "outbound",
                    "required": true
                },
                "fields": {
                    "seat_no": {
                        "type": "string",
                        "label": "座位号",
                        "required": true
                    }
                }
            }),
            case_properties: serde_json::json!({
                "workflow_policy": {
                    "batch_notification_enabled": true,
                    "batch_receipt_mode": "shared_group"
                }
            }),
        };
        repo.items.lock().unwrap().insert(catalog.code.clone(), catalog);
        repo
    }
}
