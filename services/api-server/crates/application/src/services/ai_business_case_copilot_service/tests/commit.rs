use super::batch::build_copilot_service;
use super::saga::*;
use super::*;

use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;

#[derive(Default)]
struct FakeBusinessCaseWorkflowRunRepository {
    runs: Arc<Mutex<Vec<BusinessCaseWorkflowRun>>>,
}

#[async_trait::async_trait]
impl fms_domain::ports::business_case_workflow_run_repository::BusinessCaseWorkflowRunRepository
    for FakeBusinessCaseWorkflowRunRepository
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

fn make_flight_for_test(
    flight_id: &str,
    flight_no: &str,
    scheduled_offset_mins: i64,
    status: fms_domain::models::value_objects::FlightStatus,
    leg_type: fms_domain::models::flight_leg::LegType,
    actual_departure_offset_mins: Option<i64>,
) -> Flight {
    use fms_domain::models::flight_leg::{FlightLeg, FlightTypeCode};
    use fms_domain::models::value_objects::{FlightId, FlightNumber};

    let now = Utc::now();
    let scheduled_time = now + Duration::minutes(scheduled_offset_mins);
    let actual_departure = actual_departure_offset_mins.map(|m| now + Duration::minutes(m));

    let mut flight = Flight {
        flight_id: FlightId::from(flight_id),
        airline_code: Some(flight_no[..2].to_string()),
        flight_number: Some(FlightNumber::from(flight_no)),
        registration: None,
        aircraft_type_detail: None,
        stand: None,
        gate: None,
        terminal: None,
        position: None,
        baggage_carousel: None,
        scheduled_departure: Some(scheduled_time),
        scheduled_arrival: Some(scheduled_time),
        estimated_departure: Some(scheduled_time),
        estimated_arrival: Some(scheduled_time),
        actual_departure,
        actual_arrival: None,
        cobt_time: None,
        codt: None,
        has_boarding_restriction: false,
        is_quick_turnaround: false,
        is_commercial_signed: true,
        status,
        inbound_leg: None,
        outbound_leg: None,
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

    let leg = FlightLeg {
        leg_type: leg_type.clone(),
        flight_no: flight_no.to_string(),
        flight_type: FlightTypeCode::Domestic,
        mission: None,
        origin_code: Some("PEK".to_string()),
        destination_code: Some("PVG".to_string()),
        origin_name: None,
        destination_name: None,
        is_vip: false,
        stand_type: None,
        scheduled_time: Some(scheduled_time),
    };

    match leg_type {
        fms_domain::models::flight_leg::LegType::Inbound => flight.inbound_leg = Some(leg),
        fms_domain::models::flight_leg::LegType::Outbound => flight.outbound_leg = Some(leg),
    }

    flight
}

fn make_outbound_flight(flight_id: &str, flight_no: &str, scheduled_offset_mins: i64) -> Flight {
    make_flight_for_test(
        flight_id,
        flight_no,
        scheduled_offset_mins,
        fms_domain::models::value_objects::FlightStatus::Scheduled,
        fms_domain::models::flight_leg::LegType::Outbound,
        None,
    )
}

fn make_inbound_flight(flight_id: &str, flight_no: &str, scheduled_offset_mins: i64) -> Flight {
    make_flight_for_test(
        flight_id,
        flight_no,
        scheduled_offset_mins,
        fms_domain::models::value_objects::FlightStatus::Scheduled,
        fms_domain::models::flight_leg::LegType::Inbound,
        None,
    )
}

fn make_cancelled_outbound_flight(flight_id: &str, flight_no: &str, scheduled_offset_mins: i64) -> Flight {
    make_flight_for_test(
        flight_id,
        flight_no,
        scheduled_offset_mins,
        fms_domain::models::value_objects::FlightStatus::Cancelled,
        fms_domain::models::flight_leg::LegType::Outbound,
        None,
    )
}

fn make_departed_outbound_flight(flight_id: &str, flight_no: &str, scheduled_offset_mins: i64) -> Flight {
    make_flight_for_test(
        flight_id,
        flight_no,
        scheduled_offset_mins,
        fms_domain::models::value_objects::FlightStatus::Departed,
        fms_domain::models::flight_leg::LegType::Outbound,
        Some(scheduled_offset_mins),
    )
}

async fn seed_canonical_gate_baggage_flights(flight_repo: &dyn FlightRepository) {
    flight_repo
        .save(&make_outbound_flight("flight-7714", "CA7714", 30))
        .await
        .unwrap();
    flight_repo
        .save(&make_outbound_flight("flight-5352", "MU5352", 45))
        .await
        .unwrap();
    flight_repo
        .save(&make_outbound_flight("flight-6333", "CZ6333", 60))
        .await
        .unwrap();
    flight_repo
        .save(&make_inbound_flight("flight-inbound-7714", "CA7714", 30))
        .await
        .unwrap();
    flight_repo
        .save(&make_cancelled_outbound_flight("flight-cancelled-7714", "HU7714", 20))
        .await
        .unwrap();
    flight_repo
        .save(&make_departed_outbound_flight("flight-departed-5352", "3U5352", -20))
        .await
        .unwrap();
}

async fn draft_canonical_gate_baggage(
    service: &AiBusinessCaseCopilotService<FakeAiCopilotBusinessCaseBatchRepository>,
) -> AiCopilotDraftResponse {
    seed_canonical_gate_baggage_flights(&*service.flight_repo).await;
    service
        .ai_admin_service
        .set_next_chat_completion(CANONICAL_GATE_BAGGAGE_LLM_JSON);

    service
        .draft_from_transcript(
            AiCopilotDraftRequest {
                entity_id: "flight-monitor-copilot".to_string(),
                transcript: CANONICAL_GATE_BAGGAGE_TRANSCRIPT.to_string(),
                source_page: Some("flight_monitor".to_string()),
                context: serde_json::json!({"now": Utc::now()}),
            },
            "dispatcher",
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap()
}

fn approved_actions_from_draft(draft: &AiCopilotDraftResponse) -> Vec<AiCopilotApprovedAction> {
    draft
        .actions
        .iter()
        .map(|action| {
            let matched = action.matched_flight.as_ref().unwrap();
            AiCopilotApprovedAction {
                action_id: action.action_id.clone(),
                case_type: action.case_type.clone(),
                flight_id: matched.flight_id.clone(),
                flight_no: matched.flight_no.clone(),
                bound_leg_type: Some(matched.leg_type.clone()),
                bound_flight_no: Some(matched.flight_no.clone()),
                description: Some(action.description.clone()),
                remarks: Some(action.remarks.clone()),
                fields: action.fields.clone(),
                status: None,
            }
        })
        .collect()
}

async fn begin_stale_commit_for_recovery(
    service: &AiBusinessCaseCopilotService<FakeAiCopilotBusinessCaseBatchRepository>,
    batch_id: &str,
    request: &AiCopilotCommitRequest,
) {
    let request_value = serde_json::to_value(request).unwrap();
    let acquired = service
        .repo
        .try_begin_commit_with_request(batch_id, &request_value, None)
        .await
        .unwrap();
    assert!(matches!(acquired, BeginCommitResult::Acquired(_)));
    let mut batch = service.repo.find_by_id(batch_id).await.unwrap().unwrap();
    batch.commit_started_at = Some(Utc::now() - Duration::minutes(10));
    batch.commit_next_recovery_at = None;
    service.repo.save(&batch).await.unwrap();
}

async fn create_existing_case_for_recovery(
    service: &AiBusinessCaseCopilotService<FakeAiCopilotBusinessCaseBatchRepository>,
    batch_id: &str,
    request: &AiCopilotCommitRequest,
    action_index: usize,
) -> FlightBusinessCase {
    let batch = service.repo.find_by_id(batch_id).await.unwrap().unwrap();
    let prepared = service
        .prepare_commit_actions(&batch, request, None, None, true, false)
        .await
        .unwrap();
    let action = &prepared[action_index];
    service
        .business_case_service
        .create_for_viewer(
            &action.action.case_type,
            &action.flight_id,
            &action.flight_no,
            &action.description,
            action.context.clone(),
            action.status.as_deref(),
            "test-dispatcher",
            VisibilityScope::Common,
            None,
            None,
        )
        .await
        .unwrap()
}

fn build_copilot_service_with_fake_workflow(
    fake_case_type_repo: FakeBusinessCaseTypeRepo,
) -> (
    AiBusinessCaseCopilotService<FakeAiCopilotBusinessCaseBatchRepository>,
    Arc<dyn FlightRepository + Send + Sync>,
    Arc<FakeBusinessCaseServiceOps>,
    Arc<BusinessCaseWorkflowService>,
) {
    let copilot_batch_repo = Arc::new(FakeAiCopilotBusinessCaseBatchRepository::default());
    let ai_admin_service = Arc::new(AiAdminService::new(Arc::new(FakeAiEntityConfigRepository)));

    let flight_repo: Arc<dyn FlightRepository + Send + Sync> = Arc::new(FakeFlightRepository::default());
    let flight_service = Arc::new(FlightService::new(flight_repo.clone()));

    let fake_bcs: Arc<FakeBusinessCaseServiceOps> = Arc::new(FakeBusinessCaseServiceOps::default());
    let business_case_service: Arc<dyn BusinessCaseServiceOps> = fake_bcs.clone();

    let business_case_type_service = Arc::new(BusinessCaseTypeService::new(Arc::new(fake_case_type_repo)));

    let workflow_run_repo = Arc::new(FakeBusinessCaseWorkflowRunRepository::default());
    let workflow_service = Arc::new(
        BusinessCaseWorkflowService::new(workflow_run_repo, business_case_service.clone(), flight_service.clone())
            .with_business_case_type_service(business_case_type_service.clone()),
    );
    *workflow_service.mock_flowable_start.lock().unwrap() = true;
    *workflow_service.mock_batch_notification_result.lock().unwrap() = Some(serde_json::json!({
        "receipt_group_id": ulid::Ulid::new().to_string(),
        "items": [],
    }));

    let copilot_service = AiBusinessCaseCopilotService::new(
        copilot_batch_repo,
        ai_admin_service,
        flight_repo.clone(),
        flight_service,
        business_case_service,
    )
    .with_business_case_type_service(business_case_type_service)
    .with_workflow_service(workflow_service.clone());

    (copilot_service, flight_repo, fake_bcs, workflow_service)
}

#[tokio::test]
async fn test_canonical_gate_baggage_transcript_drafts_three_outbound_actions() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let service = build_copilot_service(fake_repo);
    seed_canonical_gate_baggage_flights(&*service.flight_repo).await;
    service
        .ai_admin_service
        .set_next_chat_completion(CANONICAL_GATE_BAGGAGE_LLM_JSON);

    let response = service
        .draft_from_transcript(
            AiCopilotDraftRequest {
                entity_id: "flight-monitor-copilot".to_string(),
                transcript: CANONICAL_GATE_BAGGAGE_TRANSCRIPT.to_string(),
                source_page: Some("flight_monitor".to_string()),
                context: serde_json::json!({"now": Utc::now()}),
            },
            "dispatcher",
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    assert_eq!(response.actions.len(), 3);
    let expected = [("7714", "23A"), ("5352", "32F"), ("6333", "1A")];
    for (action, (flight_suffix, seat_no)) in response.actions.iter().zip(expected) {
        assert_eq!(action.case_type, "gate_baggage_check");
        let matched = action.matched_flight.as_ref().unwrap();
        assert_eq!(matched.leg_type, "outbound");
        assert!(matched.flight_no.ends_with(flight_suffix));
        assert_eq!(action.fields.get("seat_no").and_then(|v| v.as_str()), Some(seat_no));
        assert!(action.remarks.contains(seat_no));
        assert!(!action.needs_review);
    }
}

#[tokio::test]
async fn test_canonical_gate_baggage_commit_creates_three_cases_with_extra_info() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, _workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    seed_canonical_gate_baggage_flights(&*service.flight_repo).await;
    service
        .ai_admin_service
        .set_next_chat_completion(CANONICAL_GATE_BAGGAGE_LLM_JSON);

    let draft = service
        .draft_from_transcript(
            AiCopilotDraftRequest {
                entity_id: "flight-monitor-copilot".to_string(),
                transcript: CANONICAL_GATE_BAGGAGE_TRANSCRIPT.to_string(),
                source_page: Some("flight_monitor".to_string()),
                context: serde_json::json!({"now": Utc::now()}),
            },
            "dispatcher",
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    let approved_actions = draft
        .actions
        .iter()
        .map(|action| {
            let matched = action.matched_flight.as_ref().unwrap();
            AiCopilotApprovedAction {
                action_id: action.action_id.clone(),
                case_type: action.case_type.clone(),
                flight_id: matched.flight_id.clone(),
                flight_no: matched.flight_no.clone(),
                bound_leg_type: Some(matched.leg_type.clone()),
                bound_flight_no: Some(matched.flight_no.clone()),
                description: Some(action.description.clone()),
                remarks: Some(action.remarks.clone()),
                fields: action.fields.clone(),
                status: None,
            }
        })
        .collect::<Vec<_>>();

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-e2e".to_string()),
                actions: approved_actions.clone(),
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    assert_eq!(committed.case_ids.len(), 3);
    assert!(!committed.already_committed);

    let status = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
    assert_eq!(status.status, AiCopilotBatchStatus::Committed);
    assert!(status.commit_request.is_some());
    let created_action_case_ids = status.created_action_case_ids.as_object().unwrap();
    assert_eq!(created_action_case_ids.len(), 3);
    for action in &approved_actions {
        assert!(created_action_case_ids.contains_key(&action.action_id));
    }

    for case_id in &committed.case_ids {
        let case = business_case_repo.get(case_id).await.unwrap().unwrap();
        assert_eq!(case.case_type, "gate_baggage_check");
        let seat_no = case.context.get("seat_no").and_then(|v| v.as_str()).unwrap();

        assert!(case.description.contains(seat_no));
        assert_eq!(
            case.context.get("source").and_then(|v| v.as_str()),
            Some("ai_copilot_voice")
        );
        assert_eq!(
            case.context.get("bound_leg_type").and_then(|v| v.as_str()),
            Some("outbound")
        );
        assert_eq!(
            case.context.get("copilot_batch_id").and_then(|v| v.as_str()),
            Some(draft.batch_id.as_str())
        );
        assert!(case.context.get("copilot_action_id").is_some());
    }
}

#[tokio::test]
async fn test_canonical_gate_baggage_commit_groups_workflow_notification_once() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, _business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    seed_canonical_gate_baggage_flights(&*service.flight_repo).await;
    service
        .ai_admin_service
        .set_next_chat_completion(CANONICAL_GATE_BAGGAGE_LLM_JSON);

    let draft = service
        .draft_from_transcript(
            AiCopilotDraftRequest {
                entity_id: "flight-monitor-copilot".to_string(),
                transcript: CANONICAL_GATE_BAGGAGE_TRANSCRIPT.to_string(),
                source_page: Some("flight_monitor".to_string()),
                context: serde_json::json!({"now": Utc::now()}),
            },
            "dispatcher",
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    let approved_actions = draft
        .actions
        .iter()
        .map(|action| {
            let matched = action.matched_flight.as_ref().unwrap();
            AiCopilotApprovedAction {
                action_id: action.action_id.clone(),
                case_type: action.case_type.clone(),
                flight_id: matched.flight_id.clone(),
                flight_no: matched.flight_no.clone(),
                bound_leg_type: Some(matched.leg_type.clone()),
                bound_flight_no: Some(matched.flight_no.clone()),
                description: Some(action.description.clone()),
                remarks: Some(action.remarks.clone()),
                fields: action.fields.clone(),
                status: None,
            }
        })
        .collect::<Vec<_>>();

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-workflow".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    assert_eq!(committed.case_ids.len(), 3);
    if committed.workflow_dispatch_status != "succeeded" {
        let status = service
            .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
            .await
            .unwrap();
        panic!("workflow dispatch failed: {:?}", status.workflow_dispatch_error);
    }
    assert_eq!(committed.notification_groups.len(), 1);
    assert_eq!(committed.notification_groups[0].case_ids.len(), 3);
    assert_eq!(committed.notification_groups[0].case_type, "登机口开包");
    assert!(committed.notification_groups[0].body.contains("23A"));
    assert!(committed.notification_groups[0].body.contains("32F"));
    assert!(committed.notification_groups[0].body.contains("1A"));
    assert!(committed.notification_groups[0].title.contains("3"));
    assert!(committed.notification_groups[0].title.contains("登机口开包"));

    let sent_batches = workflow_service.mock_batch_notifications.lock().unwrap().clone();
    assert_eq!(sent_batches.len(), 1);
    assert_eq!(sent_batches[0].title, committed.notification_groups[0].title);
    assert_eq!(sent_batches[0].body, committed.notification_groups[0].body);
    assert!(sent_batches[0].receipt_required);
}

#[tokio::test]
async fn test_canonical_gate_baggage_workflow_failure_keeps_committed_cases() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    seed_canonical_gate_baggage_flights(&*service.flight_repo).await;
    service
        .ai_admin_service
        .set_next_chat_completion(CANONICAL_GATE_BAGGAGE_LLM_JSON);

    let draft = service
        .draft_from_transcript(
            AiCopilotDraftRequest {
                entity_id: "flight-monitor-copilot".to_string(),
                transcript: CANONICAL_GATE_BAGGAGE_TRANSCRIPT.to_string(),
                source_page: Some("flight_monitor".to_string()),
                context: serde_json::json!({"now": Utc::now()}),
            },
            "dispatcher",
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    let approved_actions = draft
        .actions
        .iter()
        .map(|action| {
            let matched = action.matched_flight.as_ref().unwrap();
            AiCopilotApprovedAction {
                action_id: action.action_id.clone(),
                case_type: action.case_type.clone(),
                flight_id: matched.flight_id.clone(),
                flight_no: matched.flight_no.clone(),
                bound_leg_type: Some(matched.leg_type.clone()),
                bound_flight_no: Some(matched.flight_no.clone()),
                description: Some(action.description.clone()),
                remarks: Some(action.remarks.clone()),
                fields: action.fields.clone(),
                status: None,
            }
        })
        .collect::<Vec<_>>();

    // Setup workflow mock dispatch failure
    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-workflow-fails".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    assert_eq!(committed.case_ids.len(), 3);
    assert_eq!(committed.workflow_dispatch_status, "failed");

    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.status, AiCopilotBatchStatus::Committed);
    assert_eq!(status.committed_case_ids.len(), 3);
    assert_eq!(status.workflow_dispatch_status, "failed");
    assert!(status.workflow_dispatch_error.is_some());

    for case_id in &committed.case_ids {
        let case = business_case_repo.get(case_id).await.unwrap().unwrap();
        assert_eq!(case.case_type, "gate_baggage_check");
    }
}

#[tokio::test]
async fn test_commit_recovery_zero_of_n_creates_cases_and_commits() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);
    let draft = draft_canonical_gate_baggage(&service).await;
    let request = AiCopilotCommitRequest {
        idempotency_key: Some("recovery-zero-of-n".to_string()),
        actions: approved_actions_from_draft(&draft),
    };
    begin_stale_commit_for_recovery(&service, &draft.batch_id, &request).await;

    let summary = service
        .recover_stale_commits_once(10, 120, DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS)
        .await
        .unwrap();

    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.committed, 1);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.status, AiCopilotBatchStatus::Committed);
    assert_eq!(status.committed_case_ids.len(), 3);
    assert_eq!(status.workflow_dispatch_status, "succeeded");
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_commit_recovery_one_of_n_reuses_existing_case_without_duplication() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, _workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);
    let draft = draft_canonical_gate_baggage(&service).await;
    let request = AiCopilotCommitRequest {
        idempotency_key: Some("recovery-one-of-n".to_string()),
        actions: approved_actions_from_draft(&draft),
    };
    begin_stale_commit_for_recovery(&service, &draft.batch_id, &request).await;
    let existing = create_existing_case_for_recovery(&service, &draft.batch_id, &request, 0).await;
    service
        .repo
        .record_created_action_case(&draft.batch_id, &request.actions[0].action_id, &existing.case_id)
        .await
        .unwrap();

    let summary = service
        .recover_stale_commits_once(10, 120, DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS)
        .await
        .unwrap();

    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.committed, 1);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.committed_case_ids.len(), 3);
    assert!(status.committed_case_ids.contains(&existing.case_id));
}

#[tokio::test]
async fn test_commit_recovery_all_cases_workflow_succeeded_marks_committed_without_dispatch() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);
    let draft = draft_canonical_gate_baggage(&service).await;
    let request = AiCopilotCommitRequest {
        idempotency_key: Some("recovery-all-succeeded".to_string()),
        actions: approved_actions_from_draft(&draft),
    };
    begin_stale_commit_for_recovery(&service, &draft.batch_id, &request).await;
    for index in 0..request.actions.len() {
        let existing = create_existing_case_for_recovery(&service, &draft.batch_id, &request, index).await;
        service
            .repo
            .record_created_action_case(&draft.batch_id, &request.actions[index].action_id, &existing.case_id)
            .await
            .unwrap();
    }
    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_status = "succeeded".to_string();
        batch.notification_groups = json!([{
            "group_id": "existing-group",
            "case_type": "gate_baggage_check",
            "case_ids": [],
            "title": "existing",
            "body": "existing",
        }]);
        service.repo.save(&batch).await.unwrap();
    }

    let summary = service
        .recover_stale_commits_once(10, 120, DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS)
        .await
        .unwrap();

    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.committed, 1);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 0);
    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.status, AiCopilotBatchStatus::Committed);
    assert_eq!(status.workflow_dispatch_status, "succeeded");
    assert_eq!(
        status.notification_groups[0]["group_id"].as_str(),
        Some("existing-group")
    );
}

#[tokio::test]
async fn test_commit_recovery_missing_commit_request_fails_legacy_batch_without_cases() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, _workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);
    let draft = draft_canonical_gate_baggage(&service).await;
    let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
    batch.status = AiCopilotBatchStatus::Committing;
    batch.commit_request = None;
    batch.commit_started_at = Some(Utc::now() - Duration::minutes(10));
    batch.commit_next_recovery_at = None;
    service.repo.save(&batch).await.unwrap();

    let summary = service
        .recover_stale_commits_once(10, 120, DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS)
        .await
        .unwrap();

    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.legacy_missing_request, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 0);
    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.status, AiCopilotBatchStatus::Failed);
    assert_eq!(
        status
            .commit_error
            .as_ref()
            .and_then(|value| value.get("stage"))
            .and_then(Value::as_str),
        Some("legacy_missing_request")
    );
}

#[tokio::test]
async fn test_commit_recovery_persistent_error_fails_after_max_attempts_and_keeps_known_cases() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, _workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);
    let draft = draft_canonical_gate_baggage(&service).await;
    let request = AiCopilotCommitRequest {
        idempotency_key: Some("recovery-persistent-error-max-attempts".to_string()),
        actions: approved_actions_from_draft(&draft),
    };
    begin_stale_commit_for_recovery(&service, &draft.batch_id, &request).await;

    let first_existing = create_existing_case_for_recovery(&service, &draft.batch_id, &request, 0).await;
    let second_existing = create_existing_case_for_recovery(&service, &draft.batch_id, &request, 1).await;
    service
        .repo
        .record_created_action_case(&draft.batch_id, &request.actions[0].action_id, &second_existing.case_id)
        .await
        .unwrap();
    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.commit_attempts = DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS - 1;
        batch.commit_started_at = Some(Utc::now() - Duration::minutes(10));
        batch.commit_next_recovery_at = None;
        service.repo.save(&batch).await.unwrap();
    }

    let summary = service
        .recover_stale_commits_once(10, 120, DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS)
        .await
        .unwrap();

    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.committed, 0);
    assert_eq!(summary.failed, 1);
    let status = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
    assert_eq!(status.status, AiCopilotBatchStatus::Failed);
    assert_eq!(
        status
            .commit_error
            .as_ref()
            .and_then(|value| value.get("stage"))
            .and_then(Value::as_str),
        Some("commit_recovery_max_attempts_exhausted")
    );
    assert_eq!(
        status
            .commit_error
            .as_ref()
            .and_then(|value| value.get("commit_attempts"))
            .and_then(Value::as_i64),
        Some(DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS as i64)
    );
    assert!(status.committed_case_ids.contains(&first_existing.case_id));
    assert!(status.committed_case_ids.contains(&second_existing.case_id));
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 2);
    assert!(business_case_repo.get(&first_existing.case_id).await.unwrap().is_some());
    assert!(business_case_repo
        .get(&second_existing.case_id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn test_canonical_gate_baggage_workflow_failure_retries_without_duplicate_cases() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    let draft = draft_canonical_gate_baggage(&service).await;
    let approved_actions = approved_actions_from_draft(&draft);

    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-workflow-retry-success".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(committed.workflow_dispatch_status, "failed");

    let created_case_ids = committed.case_ids.clone();
    let cases_after_commit = business_case_repo.cases.lock().unwrap().len();
    assert_eq!(cases_after_commit, 3);

    *workflow_service.mock_dispatch_result.lock().unwrap() = None;

    let retried = service
        .retry_workflow_dispatch(
            &draft.batch_id,
            WorkflowActor {
                actor: "retry-worker".to_string(),
                ..Default::default()
            },
            ops_batch_access(),
        )
        .await
        .unwrap();

    assert_eq!(retried.status, AiCopilotBatchStatus::Committed);
    assert_eq!(retried.workflow_dispatch_status, "succeeded");
    assert_eq!(retried.committed_case_ids, created_case_ids);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 1);

    let attempts_after_success = retried.workflow_dispatch_attempts;
    let retried_again = service
        .retry_workflow_dispatch(
            &draft.batch_id,
            WorkflowActor {
                actor: "retry-worker".to_string(),
                ..Default::default()
            },
            ops_batch_access(),
        )
        .await
        .unwrap();

    assert_eq!(retried_again.workflow_dispatch_status, "succeeded");
    assert_eq!(retried_again.workflow_dispatch_attempts, attempts_after_success);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_canonical_gate_baggage_pending_retry_is_not_dispatched_again() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    let draft = draft_canonical_gate_baggage(&service).await;
    let approved_actions = approved_actions_from_draft(&draft);

    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-workflow-pending-retry".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(committed.workflow_dispatch_status, "failed");

    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_status = "pending".to_string();
        batch.workflow_dispatch_error = None;
        service.repo.save(&batch).await.unwrap();
    }
    *workflow_service.mock_dispatch_result.lock().unwrap() = None;

    let retry_result = service
        .retry_workflow_dispatch(
            &draft.batch_id,
            WorkflowActor {
                actor: "retry-worker".to_string(),
                ..Default::default()
            },
            ops_batch_access(),
        )
        .await
        .unwrap();

    assert_eq!(retry_result.workflow_dispatch_status, "pending");
    assert_eq!(retry_result.committed_case_ids, committed.case_ids);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn test_due_retry_skips_fresh_pending_workflow_dispatch() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    let draft = draft_canonical_gate_baggage(&service).await;
    let approved_actions = approved_actions_from_draft(&draft);

    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-fresh-pending-scheduler".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(committed.workflow_dispatch_status, "failed");
    let attempts_after_commit = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap()
        .workflow_dispatch_attempts;

    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_status = "pending".to_string();
        batch.workflow_dispatch_error = None;
        batch.workflow_dispatch_next_retry_at = None;
        batch.updated_at = Utc::now();
        service.repo.save(&batch).await.unwrap();
    }

    *workflow_service.mock_dispatch_result.lock().unwrap() = None;
    let summary = service.retry_due_workflow_dispatches_once(10, 5).await.unwrap();

    assert_eq!(summary.scanned, 0);
    assert_eq!(summary.succeeded, 0);
    assert_eq!(summary.skipped, 0);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 0);

    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.workflow_dispatch_status, "pending");
    assert_eq!(status.workflow_dispatch_attempts, attempts_after_commit);
    assert!(status.workflow_dispatch_next_retry_at.is_none());
}

#[tokio::test]
async fn test_due_retry_recovers_stale_pending_workflow_dispatch() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    let draft = draft_canonical_gate_baggage(&service).await;
    let approved_actions = approved_actions_from_draft(&draft);

    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-stale-pending-scheduler".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(committed.workflow_dispatch_status, "failed");
    let attempts_after_commit = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap()
        .workflow_dispatch_attempts;

    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_status = "pending".to_string();
        batch.workflow_dispatch_error = None;
        batch.workflow_dispatch_next_retry_at = None;
        batch.updated_at = Utc::now() - Duration::minutes(16);
        service.repo.save(&batch).await.unwrap();
    }

    *workflow_service.mock_dispatch_result.lock().unwrap() = None;
    let summary = service.retry_due_workflow_dispatches_once(10, 5).await.unwrap();

    assert_eq!(summary.scanned, 1);
    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 0);
    assert_eq!(summary.batch_ids, vec![draft.batch_id.clone()]);
    assert_eq!(business_case_repo.cases.lock().unwrap().len(), 3);
    assert_eq!(workflow_service.mock_batch_notifications.lock().unwrap().len(), 1);

    let status = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap();
    assert_eq!(status.workflow_dispatch_status, "succeeded");
    assert_eq!(status.committed_case_ids, committed.case_ids);
    assert_eq!(status.workflow_dispatch_attempts, attempts_after_commit + 1);
    assert!(status.workflow_dispatch_error.is_none());
    assert!(status.workflow_dispatch_next_retry_at.is_none());
}

#[tokio::test]
async fn test_stale_pending_recovery_marks_failed_due_without_counting_attempt() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, _business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    let draft = draft_canonical_gate_baggage(&service).await;
    let approved_actions = approved_actions_from_draft(&draft);

    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-stale-pending-recovery-state".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(committed.workflow_dispatch_status, "failed");
    let attempts_after_commit = service
        .get_batch_status(&draft.batch_id, batch_access("dispatcher"))
        .await
        .unwrap()
        .workflow_dispatch_attempts;

    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_status = "pending".to_string();
        batch.workflow_dispatch_error = None;
        batch.workflow_dispatch_next_retry_at = None;
        batch.updated_at = Utc::now() - Duration::minutes(16);
        service.repo.save(&batch).await.unwrap();
    }

    let recovered = service
        .repo
        .recover_stale_workflow_dispatch_pending(Utc::now() - Duration::minutes(15), 10)
        .await
        .unwrap();

    assert_eq!(recovered.len(), 1);
    let recovered = &recovered[0];
    assert_eq!(recovered.batch_id, draft.batch_id);
    assert_eq!(recovered.workflow_dispatch_status, "failed");
    assert_eq!(recovered.workflow_dispatch_attempts, attempts_after_commit);
    assert!(recovered.workflow_dispatch_next_retry_at.is_none());
    assert_eq!(
        recovered
            .workflow_dispatch_error
            .as_ref()
            .and_then(|value| value.get("stage"))
            .and_then(|value| value.as_str()),
        Some("workflow_dispatch_stale_pending")
    );
}

#[tokio::test]
async fn test_canonical_gate_baggage_workflow_failure_metrics() {
    let fake_repo = FakeBusinessCaseTypeRepo::with_gate_baggage_check();
    let (service, _flight_repo, _business_case_repo, workflow_service) =
        build_copilot_service_with_fake_workflow(fake_repo);

    seed_canonical_gate_baggage_flights(&*service.flight_repo).await;
    service
        .ai_admin_service
        .set_next_chat_completion(CANONICAL_GATE_BAGGAGE_LLM_JSON);

    let draft = service
        .draft_from_transcript(
            AiCopilotDraftRequest {
                entity_id: "flight-monitor-copilot".to_string(),
                transcript: CANONICAL_GATE_BAGGAGE_TRANSCRIPT.to_string(),
                source_page: Some("flight_monitor".to_string()),
                context: serde_json::json!({"now": Utc::now()}),
            },
            "dispatcher",
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    let approved_actions = draft
        .actions
        .iter()
        .map(|action| {
            let matched = action.matched_flight.as_ref().unwrap();
            AiCopilotApprovedAction {
                action_id: action.action_id.clone(),
                case_type: action.case_type.clone(),
                flight_id: matched.flight_id.clone(),
                flight_no: matched.flight_no.clone(),
                bound_leg_type: Some(matched.leg_type.clone()),
                bound_flight_no: Some(matched.flight_no.clone()),
                description: Some(action.description.clone()),
                remarks: Some(action.remarks.clone()),
                fields: action.fields.clone(),
                status: None,
            }
        })
        .collect::<Vec<_>>();

    // Setup workflow mock dispatch failure
    *workflow_service.mock_dispatch_result.lock().unwrap() = Some(Err("Workflow engine unavailable".to_string()));

    let _committed = service
        .commit_batch(
            &draft.batch_id,
            AiCopilotCommitRequest {
                idempotency_key: Some("canonical-gate-baggage-workflow-metrics".to_string()),
                actions: approved_actions,
            },
            batch_access("dispatcher"),
            WorkflowActor {
                actor: "test-dispatcher".to_string(),
                ..Default::default()
            },
            VisibilityScope::Common,
            Some("ops"),
            Some("运行控制"),
            true,
        )
        .await
        .unwrap();

    // Mutate next retry time to past so it is considered due for retry immediately
    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_next_retry_at = Some(Utc::now() - chrono::Duration::seconds(10));
        service.repo.save(&batch).await.unwrap();
    }

    let metrics = service.operational_metrics(5, 10).await.unwrap();
    assert_eq!(metrics.batch_status.committed, 1);
    assert_eq!(metrics.workflow_dispatch.failed, 1);
    assert_eq!(metrics.workflow_dispatch.retry_due, 1);
    assert_eq!(metrics.workflow_dispatch.retry_exhausted, 0);
    assert_eq!(metrics.recent_errors.len(), 1);
    assert_eq!(metrics.recent_errors[0].batch_id, draft.batch_id);

    // Mutate the batch to simulate retry exhaustion state
    {
        let mut batch = service.repo.find_by_id(&draft.batch_id).await.unwrap().unwrap();
        batch.workflow_dispatch_attempts = 5;
        service.repo.save(&batch).await.unwrap();
    }

    let metrics_after = service.operational_metrics(5, 10).await.unwrap();
    assert_eq!(metrics_after.workflow_dispatch.retry_exhausted, 1);
}
