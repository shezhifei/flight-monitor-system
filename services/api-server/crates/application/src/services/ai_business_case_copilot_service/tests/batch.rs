use super::*;

pub(super) fn build_copilot_service(
    fake_case_type_repo: FakeBusinessCaseTypeRepo,
) -> AiBusinessCaseCopilotService<FakeAiCopilotBusinessCaseBatchRepository> {
    let copilot_batch_repo = Arc::new(FakeAiCopilotBusinessCaseBatchRepository::default());
    let ai_admin_service = Arc::new(AiAdminService::new(Arc::new(FakeAiEntityConfigRepository)));

    let flight_repo: Arc<dyn FlightRepository + Send + Sync> = Arc::new(FakeFlightRepository::default());
    let flight_service = Arc::new(FlightService::new(flight_repo.clone()));

    let business_case_service: Arc<dyn BusinessCaseServiceOps> = Arc::new(FakeBusinessCaseServiceOps::default());

    let business_case_type_service = Arc::new(BusinessCaseTypeService::new(Arc::new(fake_case_type_repo)));

    AiBusinessCaseCopilotService::new(
        copilot_batch_repo,
        ai_admin_service,
        flight_repo,
        flight_service,
        business_case_service,
    )
    .with_business_case_type_service(business_case_type_service)
}

#[tokio::test]
async fn test_batch_status_and_list_are_limited_to_owner() {
    let service = build_copilot_service(FakeBusinessCaseTypeRepo::default());
    service
        .repo
        .save(&test_copilot_batch(
            "batch-owner-a",
            "dispatcher-a",
            AiCopilotBatchStatus::Draft,
        ))
        .await
        .unwrap();
    service
        .repo
        .save(&test_copilot_batch(
            "batch-owner-b",
            "dispatcher-b",
            AiCopilotBatchStatus::Draft,
        ))
        .await
        .unwrap();

    let owner = service
        .get_batch_status("batch-owner-a", batch_access("dispatcher-a"))
        .await
        .unwrap();
    assert_eq!(owner.batch_id, "batch-owner-a");

    let cross_user = service
        .get_batch_status("batch-owner-a", batch_access("dispatcher-b"))
        .await
        .expect_err("cross-user batch reads must be hidden");
    assert!(matches!(cross_user, DomainError::NotFound { .. }));

    let listed = service
        .list_batches(None, None, 50, 0, batch_access("dispatcher-b"))
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].batch_id, "batch-owner-b");
}

#[tokio::test]
async fn test_commit_requires_batch_owner_before_validation() {
    let service = build_copilot_service(FakeBusinessCaseTypeRepo::default());
    service
        .repo
        .save(&test_copilot_batch(
            "batch-commit-owner",
            "dispatcher-a",
            AiCopilotBatchStatus::Draft,
        ))
        .await
        .unwrap();
    let request = AiCopilotCommitRequest {
        idempotency_key: None,
        actions: vec![],
    };

    let cross_user = service
        .commit_batch(
            "batch-commit-owner",
            request.clone(),
            batch_access("dispatcher-b"),
            WorkflowActor {
                actor: "dispatcher-b".to_string(),
                ..Default::default()
            },
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
            false,
        )
        .await
        .expect_err("cross-user commits must be hidden");
    assert!(matches!(cross_user, DomainError::NotFound { .. }));

    let owner = service
        .commit_batch(
            "batch-commit-owner",
            request,
            batch_access("dispatcher-a"),
            WorkflowActor {
                actor: "dispatcher-a".to_string(),
                ..Default::default()
            },
            VisibilityScope::Department,
            Some("ops-1"),
            Some("ops"),
            false,
        )
        .await
        .expect_err("owner should reach normal commit validation");
    assert!(matches!(owner, DomainError::ValidationError(_)));
}

#[tokio::test]
async fn test_failed_batch_resolution_requires_ops_access() {
    let service = build_copilot_service(FakeBusinessCaseTypeRepo::default());
    let mut batch = test_copilot_batch("batch-failed-resolution", "dispatcher-a", AiCopilotBatchStatus::Failed);
    batch.commit_error = Some(json!({"stage":"create_business_case"}));
    service.repo.save(&batch).await.unwrap();

    let request = AiCopilotFailedBatchResolutionRequest {
        action: AiCopilotFailedBatchResolutionAction::MarkResolved,
        note: Some("handled".to_string()),
    };
    let cross_user = service
        .resolve_failed_batch(
            "batch-failed-resolution",
            request.clone(),
            "dispatcher-b",
            batch_access("dispatcher-b"),
        )
        .await
        .expect_err("normal users cannot resolve failed batches");
    assert!(matches!(cross_user, DomainError::NotFound { .. }));

    let resolved = service
        .resolve_failed_batch("batch-failed-resolution", request, "ops-admin", ops_batch_access())
        .await
        .unwrap();
    assert_eq!(resolved.status, AiCopilotBatchStatus::FailedResolved);
}

#[tokio::test]
async fn test_copilot_catalog_includes_case_properties_ai_copilot_without_legacy_enabled() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    fake_repo
        .save(&test_business_case_type(
            "gate_cleaning_check",
            "登机口清洁检查",
            serde_json::json!({}),
            serde_json::json!({
                "ai_copilot": {
                    "enabled": true,
                    "aliases": ["清洁检查"],
                    "utterances": ["登机口清洁"],
                    "leg_type_hint": "outbound",
                    "required_fields": ["gate_no"],
                    "field_hints": {
                        "gate_no": {
                            "type": "string",
                            "label": "登机口",
                            "aliases": ["口", "gate"]
                        }
                    },
                    "remarks_template": "登机口 {{gate_no}} 清洁检查",
                    "confidence_threshold": 0.77
                }
            }),
        ))
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);
    let catalog = service.load_case_type_catalog(None, None, true).await.unwrap();

    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].code, "gate_cleaning_check");
    assert_eq!(catalog[0].config.aliases, vec!["清洁检查".to_string()]);
    assert_eq!(catalog[0].config.trigger_phrases, vec!["登机口清洁".to_string()]);
    assert_eq!(catalog[0].config.leg_binding.default.as_deref(), Some("outbound"));
    assert!(catalog[0].config.fields.get("gate_no").unwrap().required);
    assert_eq!(catalog[0].config.confidence_threshold, Some(0.77));

    let prompt = build_extraction_prompt("清洁检查", &catalog);
    assert!(prompt.contains("gate_cleaning_check"));
    assert!(prompt.contains("登机口清洁"));
    assert!(prompt.contains("confidence_threshold"));
}

#[tokio::test]
async fn test_copilot_case_properties_ai_copilot_overrides_legacy_prompt_metadata() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    fake_repo
        .save(&test_business_case_type(
            "gate_supply_check",
            "登机口物资检查",
            serde_json::json!({
                "enabled": true,
                "aliases": ["旧别名"],
                "trigger_phrases": ["旧触发"],
                "remarks_template": "旧模板 {{item}}"
            }),
            serde_json::json!({
                "ai_copilot": {
                    "enabled": true,
                    "aliases": ["新别名"],
                    "utterances": ["新触发"],
                    "remarks_template": "新模板 {{item}}"
                }
            }),
        ))
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);
    let catalog = service.load_case_type_catalog(None, None, true).await.unwrap();

    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].config.aliases, vec!["新别名".to_string()]);
    assert_eq!(catalog[0].config.trigger_phrases, vec!["新触发".to_string()]);
    assert_eq!(catalog[0].config.remarks_template.as_deref(), Some("新模板 {{item}}"));

    let prompt = build_extraction_prompt("新别名", &catalog);
    assert!(prompt.contains("新别名"));
    assert!(prompt.contains("新模板"));
    assert!(!prompt.contains("旧别名"));
    assert!(!prompt.contains("旧模板"));
}

#[tokio::test]
async fn test_copilot_case_properties_ai_copilot_disabled_excludes_legacy_enabled_type() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    fake_repo
        .save(&test_business_case_type(
            "disabled_gate_supply_check",
            "停用登机口物资检查",
            serde_json::json!({
                "enabled": true,
                "aliases": ["旧别名"]
            }),
            serde_json::json!({
                "ai_copilot": {
                    "enabled": false,
                    "aliases": ["不应出现"]
                }
            }),
        ))
        .await
        .unwrap();

    let service = build_copilot_service(fake_repo);
    let catalog = service.load_case_type_catalog(None, None, true).await.unwrap();

    assert!(catalog.is_empty());
}

#[tokio::test]
async fn test_draft_catalog_excludes_other_departments() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

    // 1. common_ai (Common, AI-enabled)
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

    // 2. ops_ai (ops-1, AI-enabled)
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

    // 3. tech_ai (tech-1, AI-enabled)
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

    // case A: ops-1 user, cannot manage common
    let catalog_a = service
        .load_case_type_catalog(Some("ops-1"), Some("ops"), false)
        .await
        .unwrap();
    let codes_a: Vec<String> = catalog_a.into_iter().map(|entry| entry.code).collect();
    assert_eq!(codes_a, vec!["ops_ai".to_string()]);

    // case B: ops-1 user, can manage common
    let catalog_b = service
        .load_case_type_catalog(Some("ops-1"), Some("ops"), true)
        .await
        .unwrap();
    let mut codes_b: Vec<String> = catalog_b.into_iter().map(|entry| entry.code).collect();
    codes_b.sort();
    assert_eq!(codes_b, vec!["common_ai".to_string(), "ops_ai".to_string()]);
}

#[tokio::test]
async fn test_commit_rejects_disabled_ai_case_type() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

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

    let batch_id = "test_batch_123".to_string();
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
    let batch_after = service.repo.find_by_id(&batch_id).await.unwrap().unwrap();
    assert_eq!(batch_after.status, AiCopilotBatchStatus::Draft);
}

#[tokio::test]
async fn test_commit_rejects_empty_actions_without_locking_batch() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();
    let service = build_copilot_service(fake_repo);

    let batch_id = "test_batch_empty_actions".to_string();
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

    let res = service
        .commit_batch(
            &batch_id,
            AiCopilotCommitRequest {
                idempotency_key: None,
                actions: vec![],
            },
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

    assert!(matches!(res, Err(DomainError::ValidationError(_))));
    let batch_after = service.repo.find_by_id(&batch_id).await.unwrap().unwrap();
    assert_eq!(batch_after.status, AiCopilotBatchStatus::Draft);
}

#[tokio::test]
async fn test_commit_rejects_invisible_case_type() {
    let fake_repo = FakeBusinessCaseTypeRepo::default();

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

    let batch_id = "test_batch_123".to_string();
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

#[test]
fn test_validate_and_enrich_action() {
    let mut fields = HashMap::new();
    fields.insert(
        "seat_no".to_string(),
        AiFieldConfig {
            field_type: Some("string".to_string()),
            label: Some("座位号".to_string()),
            required: true,
            ..Default::default()
        },
    );

    let catalog_entry = CopilotCaseTypeCatalogEntry {
        code: "gate_baggage_check".to_string(),
        name: "登机口开包".to_string(),
        description: None,
        config: BusinessCaseAiExtractionConfig {
            enabled: true,
            aliases: vec!["开包".to_string()],
            leg_binding: AiLegBindingConfig {
                allowed: vec!["outbound".to_string()],
                default: Some("outbound".to_string()),
                required: true,
            },
            fields,
            description_template: Some("开包座位 {{seat_no}}".to_string()),
            forbidden_fields: vec!["gate".to_string()],
            ..Default::default()
        },
        case_properties: BusinessCaseProperties::default(),
    };

    let mut catalog_by_code = HashMap::new();
    catalog_by_code.insert("gate_baggage_check", &catalog_entry);

    let action1 = LlmDraftAction {
        case_type: "gate_baggage_check".to_string(),
        case_type_name: None,
        flight_number_raw: "7714".to_string(),
        leg_type_hint: None,
        description: "".to_string(),
        remarks: "".to_string(),
        fields: serde_json::json!({
            "seat_no": "23A"
        }),
        confidence: Some(0.9),
    };
    let res1 = validate_and_enrich_action(action1, 0, &catalog_by_code);
    assert!(!res1.needs_review);
    assert_eq!(res1.action.description, "开包座位 23A");
    assert_eq!(res1.action.leg_type_hint.as_deref(), Some("outbound"));

    let action2 = LlmDraftAction {
        case_type: "gate_baggage_check".to_string(),
        case_type_name: None,
        flight_number_raw: "7714".to_string(),
        leg_type_hint: None,
        description: "".to_string(),
        remarks: "".to_string(),
        fields: serde_json::json!({}),
        confidence: Some(0.9),
    };
    let res2 = validate_and_enrich_action(action2, 0, &catalog_by_code);
    assert!(res2.needs_review);
    assert!(res2.review_reason.unwrap().contains("缺少必需字段: 座位号"));

    let action3 = LlmDraftAction {
        case_type: "gate_baggage_check".to_string(),
        case_type_name: None,
        flight_number_raw: "7714".to_string(),
        leg_type_hint: None,
        description: "".to_string(),
        remarks: "".to_string(),
        fields: serde_json::json!({
            "seat_no": "23A",
            "gate": "A12"
        }),
        confidence: Some(0.9),
    };
    let res3 = validate_and_enrich_action(action3, 0, &catalog_by_code);
    assert!(res3.needs_review);
    assert!(res3.review_reason.unwrap().contains("包含了被禁止的字段: gate"));
    assert!(res3.action.fields.get("gate").is_none());
}

#[test]
fn test_match_flight_logic() {
    use fms_domain::models::flight_leg::{FlightLeg, FlightTypeCode, LegType};
    use fms_domain::models::value_objects::{FlightId, FlightNumber, FlightStatus};

    let now = Utc::now();

    // 基础 Flight 数据模板
    let base_flight = Flight {
        flight_id: FlightId::from("FL123"),
        airline_code: Some("CA".to_string()),
        flight_number: Some(FlightNumber::from("CA1234")),
        registration: None,
        aircraft_type_detail: None,
        stand: None,
        gate: None,
        terminal: None,
        position: None,
        baggage_carousel: None,
        scheduled_departure: Some(now + Duration::hours(2)),
        scheduled_arrival: None,
        estimated_departure: Some(now + Duration::hours(2)),
        estimated_arrival: None,
        actual_departure: None,
        actual_arrival: None,
        cobt_time: None,
        codt: None,
        has_boarding_restriction: false,
        is_quick_turnaround: false,
        is_commercial_signed: true,
        status: FlightStatus::Scheduled,
        inbound_leg: None,
        outbound_leg: Some(FlightLeg {
            leg_type: LegType::Outbound,
            flight_no: "CA1234".to_string(),
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

    // 1. 测试成功匹配
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
    let leg_binding = AiLegBindingConfig {
        allowed: vec!["outbound".to_string()],
        default: Some("outbound".to_string()),
        required: true,
    };

    let matched = match_flight(&base_flight, "CA1234", &config, &leg_binding);
    assert!(matched.is_some());
    let m = matched.unwrap();
    assert_eq!(m.flight_id, "FL123");
    assert_eq!(m.leg_type, "outbound");

    // 2. 测试后缀匹配
    let matched_suffix = match_flight(&base_flight, "1234", &config, &leg_binding);
    assert!(matched_suffix.is_some());
    let no_suffix_config = AiFlightMatchingConfig {
        allow_numeric_suffix: Some(false),
        ..config.clone()
    };
    let blocked_suffix = match_flight(&base_flight, "1234", &no_suffix_config, &leg_binding);
    assert!(
        blocked_suffix.is_none(),
        "allow_numeric_suffix=false should reject pure numeric suffix matches"
    );

    // 3. 测试状态过滤 - 已取消
    let mut cancelled_flight = base_flight.clone();
    cancelled_flight.status = FlightStatus::Cancelled;
    let matched_cancelled = match_flight(&cancelled_flight, "CA1234", &config, &leg_binding);
    assert!(matched_cancelled.is_none());

    // 4. 测试时间窗口过滤 - 计划起飞超出 window_hours_before
    let mut early_flight = base_flight.clone();
    early_flight.scheduled_departure = Some(now + Duration::hours(5));
    early_flight.estimated_departure = Some(now + Duration::hours(5));
    if let Some(ref mut leg) = early_flight.outbound_leg {
        leg.scheduled_time = Some(now + Duration::hours(5));
    }
    let matched_early = match_flight(&early_flight, "CA1234", &config, &leg_binding);
    assert!(matched_early.is_none());

    // 5. 进港匹配与 prefer_leg
    let mut inbound_flight = base_flight.clone();
    inbound_flight.scheduled_arrival = Some(now + Duration::hours(1));
    inbound_flight.estimated_arrival = Some(now + Duration::hours(1));
    inbound_flight.inbound_leg = Some(FlightLeg {
        leg_type: LegType::Inbound,
        flight_no: "CA1233".to_string(),
        flight_type: FlightTypeCode::Domestic,
        mission: None,
        origin_code: Some("PVG".to_string()),
        destination_code: Some("PEK".to_string()),
        origin_name: None,
        destination_name: None,
        is_vip: false,
        stand_type: None,
        scheduled_time: Some(now + Duration::hours(1)),
    });

    let config_prefer = AiFlightMatchingConfig {
        prefer_leg: Some("inbound".to_string()),
        ..config.clone()
    };
    let leg_binding_both = AiLegBindingConfig {
        allowed: vec!["outbound".to_string(), "inbound".to_string()],
        default: Some("outbound".to_string()),
        required: true,
    };

    // 匹配进港 CA1233
    let matched_inbound = match_flight(&inbound_flight, "CA1233", &config_prefer, &leg_binding_both);
    assert!(matched_inbound.is_some());
    let m_in = matched_inbound.unwrap();
    assert_eq!(m_in.leg_type, "inbound");
    // 得分应该加上了 prefer_leg 的加成
    assert!(m_in.score > 1.0); // 1.0 (exact) + time_score * 0.15 + 0.05
}
