#[cfg(test)]
mod tests {
    use crate::services::ai_action_proposal_service::{
        AiActionProposalError, AiActionProposalService, ApproveProposalRequest, ExecuteProposalRequest,
        GenerateProposalRequest, ValidateProposalRequest,
    };
    use crate::services::ai_execution_allowlist::ExecutionAllowlist;
    use crate::services::ai_execution_readiness_service::AiExecutionReadinessService;
    use crate::services::ai_proposal_audit_recorder::AiProposalAuditEventRecorder;
    use crate::services::business_case_service::{BusinessCaseMentionAudience, CollaborationMentionAudience};
    use crate::services::dispatch_service::DispatchService;
    use fms_domain::models::ai_proposal::{ActionProposalStatus, ApprovalPolicy, RiskLevel};
    use fms_domain::ports::ai_object_policy_repository::{
        AiObjectAccessDecision, AiObjectAccessRequest, AiObjectPolicyRepository, AiObjectPolicyRepositoryError,
    };
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    struct SequenceObjectPolicyRepository {
        decisions: Mutex<Vec<AiObjectAccessDecision>>,
    }

    impl SequenceObjectPolicyRepository {
        fn new(decisions: Vec<AiObjectAccessDecision>) -> Self {
            Self {
                decisions: Mutex::new(decisions),
            }
        }
    }

    #[async_trait::async_trait]
    impl AiObjectPolicyRepository for SequenceObjectPolicyRepository {
        async fn evaluate_access(
            &self,
            _request: &AiObjectAccessRequest,
        ) -> Result<AiObjectAccessDecision, AiObjectPolicyRepositoryError> {
            let mut decisions = self.decisions.lock().expect("policy lock");
            if decisions.is_empty() {
                Ok(AiObjectAccessDecision::NoPolicy)
            } else {
                Ok(decisions.remove(0))
            }
        }
    }

    fn generate_request(
        action_name: &str,
        arguments: serde_json::Value,
        permissions: &[&str],
    ) -> GenerateProposalRequest {
        GenerateProposalRequest {
            job_id: "job_1".to_string(),
            run_id: "run_1".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: "Flight".to_string(),
            object_id: "flt_1".to_string(),
            action_name: action_name.to_string(),
            arguments,
            reasoning: Some("test proposal".to_string()),
            confidence: Some(0.9),
            requester_user_id: Some("requester_1".to_string()),
            requester_user_roles: permissions.iter().map(|item| item.to_string()).collect(),
            requester_department_id: Some("ops-1".to_string()),
            correlation_id: Some("corr_1".to_string()),
            idempotency_key: None,
            expected_object_version: Some(1),
            risk_level: None,
            approval_policy: None,
            required_permissions: None,
        }
    }

    #[tokio::test]
    async fn generate_proposal_rejects_missing_action_permission() {
        let service = AiActionProposalService::new();

        let result = service
            .generate_proposal(generate_request(
                "update_status",
                json!({"new_status": "delayed", "reason": "conflict"}),
                &["flight:read"],
            ))
            .await;

        assert!(matches!(result, Err(AiActionProposalError::Forbidden(_))));
    }

    #[tokio::test]
    async fn generate_proposal_rejects_action_missing_from_ontology_schema() {
        let service = AiActionProposalService::new();
        let result = service
            .generate_proposal(generate_request(
                "update_stand",
                json!({"new_stand_id": "S02"}),
                &["flight:write"],
            ))
            .await;

        assert!(
            matches!(&result, Err(AiActionProposalError::Validation(message)) if message.contains("not declared")),
            "unknown ontology action must fail closed: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn generate_proposal_rejects_permissions_that_differ_from_schema() {
        let service = AiActionProposalService::new();
        let mut request = generate_request("update_status", json!({"new_status": "delayed"}), &["flight:write"]);
        request.required_permissions = Some(vec!["flight:manage".to_string()]);

        let result = service.generate_proposal(request).await;
        assert!(
            matches!(&result, Err(AiActionProposalError::Validation(message)) if message.contains("must match the ontology schema")),
            "caller must not override schema permissions: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn generate_proposal_rejects_risk_level_that_differs_from_schema() {
        let service = AiActionProposalService::new();
        let mut request = generate_request("update_status", json!({"new_status": "delayed"}), &["flight:write"]);
        request.risk_level = Some(RiskLevel::Low);

        let result = service.generate_proposal(request).await;
        assert!(
            matches!(&result, Err(AiActionProposalError::Validation(message)) if message.contains("Risk level") && message.contains("ontology schema")),
            "caller must not override schema risk: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn generate_proposal_rejects_approval_policy_that_differs_from_schema() {
        let service = AiActionProposalService::new();
        let mut request = generate_request("update_status", json!({"new_status": "delayed"}), &["flight:write"]);
        request.approval_policy = Some(ApprovalPolicy::AutoExecute);

        let result = service.generate_proposal(request).await;
        assert!(
            matches!(&result, Err(AiActionProposalError::Validation(message)) if message.contains("Approval policy") && message.contains("ontology schema")),
            "caller must not override schema approval policy: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn generate_proposal_rejects_object_policy_deny() {
        let policy_repo = Arc::new(SequenceObjectPolicyRepository::new(vec![AiObjectAccessDecision::Deny]));
        let service = AiActionProposalService::new().with_object_policy_repository(policy_repo);

        let result = service
            .generate_proposal(generate_request(
                "update_status",
                json!({"new_status": "delayed", "reason": "conflict"}),
                &["flight:write"],
            ))
            .await;

        assert!(matches!(result, Err(AiActionProposalError::Forbidden(_))));
    }

    #[tokio::test]
    async fn approve_proposal_revalidates_current_actor_permission() {
        let service = AiActionProposalService::new();
        let proposal = service
            .generate_proposal(generate_request(
                "update_status",
                json!({"new_status": "delayed", "reason": "conflict"}),
                &["flight:write"],
            ))
            .await
            .expect("authorized generator should create proposal");

        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({"stand": "S01"})),
                after_preview: Some(json!({"stand": "S02"})),
                constraint_results: None,
            })
            .await
            .expect("proposal should validate");
        assert_eq!(proposal.status, ActionProposalStatus::Pending);

        let denied = service
            .approve_proposal(ApproveProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                approver_id: "approver_1".to_string(),
                approver_permissions: vec!["flight:read".to_string()],
                approver_department_id: Some("ops-1".to_string()),
                modified_arguments: None,
            })
            .await;
        assert!(matches!(denied, Err(AiActionProposalError::Forbidden(_))));

        let approved = service
            .approve_proposal(ApproveProposalRequest {
                proposal_id: proposal.proposal_id,
                approver_id: "approver_2".to_string(),
                approver_permissions: vec!["flight:write".to_string()],
                approver_department_id: Some("ops-1".to_string()),
                modified_arguments: None,
            })
            .await
            .expect("approver with current action permission should pass");
        assert_eq!(approved.status, ActionProposalStatus::Approved);
    }

    #[tokio::test]
    async fn approve_proposal_revalidates_object_policy() {
        let policy_repo = Arc::new(SequenceObjectPolicyRepository::new(vec![
            AiObjectAccessDecision::Allow,
            AiObjectAccessDecision::Deny,
        ]));
        let service = AiActionProposalService::new().with_object_policy_repository(policy_repo);
        let proposal = service
            .generate_proposal(generate_request(
                "update_status",
                json!({"new_status": "delayed", "reason": "conflict"}),
                &["flight:write"],
            ))
            .await
            .expect("object policy allows generator");

        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({"stand": "S01"})),
                after_preview: Some(json!({"stand": "S02"})),
                constraint_results: None,
            })
            .await
            .expect("proposal should validate");

        let denied = service
            .approve_proposal(ApproveProposalRequest {
                proposal_id: proposal.proposal_id,
                approver_id: "approver_1".to_string(),
                approver_permissions: vec!["flight:write".to_string()],
                approver_department_id: Some("ops-1".to_string()),
                modified_arguments: None,
            })
            .await;

        assert!(matches!(denied, Err(AiActionProposalError::Forbidden(_))));
    }

    #[tokio::test]
    async fn generate_proposal_accepts_governance_values_that_match_schema() {
        let service = AiActionProposalService::new();
        let mut request = generate_request("add_note", json!({"note_content": "ops note"}), &["flight:write"]);
        request.risk_level = Some(RiskLevel::Low);
        request.approval_policy = Some(ApprovalPolicy::AutoExecute);

        let proposal = service
            .generate_proposal(request)
            .await
            .expect("governance values matching the schema should be accepted");

        assert_eq!(proposal.risk_level, RiskLevel::Low);
        assert_eq!(proposal.approval_policy, ApprovalPolicy::AutoExecute);
    }

    #[tokio::test]
    async fn execute_proposal_revalidates_current_actor_permission_before_feature_flag() {
        let service = AiActionProposalService::new();
        let proposal = service
            .generate_proposal(generate_request(
                "add_note",
                json!({"note_content": "ops note"}),
                &["flight:write"],
            ))
            .await
            .expect("authorized generator should create proposal");

        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({})),
                after_preview: Some(json!({})),
                constraint_results: None,
            })
            .await
            .expect("low risk add_note should auto-approve");
        assert_eq!(proposal.status, ActionProposalStatus::Approved);

        let denied = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal.proposal_id,
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:read".to_string()],
                executor_department_id: Some("ops-1".to_string()),
            })
            .await;

        assert!(matches!(denied, Err(AiActionProposalError::Forbidden(_))));
    }

    #[tokio::test]
    async fn execute_proposal_rejects_when_readiness_gate_fails() {
        let readiness = Arc::new(
            crate::services::ai_execution_readiness_service::AiExecutionReadinessService::always_not_ready_for_test(
                "feature_flags",
            ),
        );
        let service = AiActionProposalService::new()
            .with_proposal_execution_enabled_for_test(true)
            .with_readiness_service(readiness);
        let proposal = service
            .generate_proposal(generate_request(
                "add_note",
                json!({"note_content": "ops note"}),
                &["flight:write"],
            ))
            .await
            .expect("authorized generator should create proposal");

        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({})),
                after_preview: Some(json!({})),
                constraint_results: None,
            })
            .await
            .expect("low risk should auto-approve");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal.proposal_id,
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: Some("ops-1".to_string()),
            })
            .await;

        assert!(
            matches!(result, Err(AiActionProposalError::Forbidden(ref msg)) if msg.contains("readiness")),
            "expected Forbidden readiness error, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn execute_proposal_records_readiness_block_event() {
        let recorder =
            Arc::new(crate::services::ai_proposal_audit_recorder::InMemoryAiProposalAuditEventRecorder::new());
        let readiness = Arc::new(
            crate::services::ai_execution_readiness_service::AiExecutionReadinessService::always_not_ready_for_test(
                "feature_flags",
            ),
        );
        let service = AiActionProposalService::new()
            .with_proposal_execution_enabled_for_test(true)
            .with_audit_recorder(recorder.clone())
            .with_readiness_service(readiness);
        let proposal = service
            .generate_proposal(generate_request(
                "add_note",
                json!({"note_content": "ops note"}),
                &["flight:write"],
            ))
            .await
            .expect("authorized generator should create proposal");

        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({})),
                after_preview: Some(json!({})),
                constraint_results: None,
            })
            .await
            .expect("low risk should auto-approve");

        let _ = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal.proposal_id,
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: Some("ops-1".to_string()),
            })
            .await;

        assert!(recorder.contains("proposal.execution_requested").await);
        assert!(recorder.contains("proposal.execution_blocked_readiness").await);
    }

    // ── Staging Smoke Helpers ──────────────────────────────────────────

    async fn smoke_pool() -> sqlx::PgPool {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
        sqlx::PgPool::connect(&url).await.expect("connect to test db")
    }

    async fn build_smoke_executor(
        pool: sqlx::PgPool,
    ) -> Arc<
        crate::services::domain_action_executor::DomainActionExecutor<
            fms_infrastructure::db::transaction::PgUnitOfWork,
        >,
    > {
        use crate::services::business_case_service::{
            BusinessCaseEventPublisher, BusinessCaseService, BusinessCaseWriter,
        };
        use crate::services::dispatch_service::writer::DispatchOrderWriter;
        use crate::services::flight_service::FlightService;
        use crate::services::flight_writer::FlightWriter;
        use crate::services::ontology_service::{OntologyService, OntologyTransactions, OntologyWriter};
        use crate::types::NoopBusinessCaseEventPublisher;
        use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
        use fms_domain::ports::flight_repository::{FlightRepository, FlightTransactionalRepository};
        use fms_domain::ports::ontology_repository::{
            AircraftRepository, CarouselAssignmentRepository, GateAssignmentRepository, OntologyTransactionalRepository,
            ResourceAdjustmentSuggestionRepository, StandOccupationRepository, TurnaroundLinkRepository,
        };
        use fms_infrastructure::repositories::pg_ontology_repository::{
            PgAircraftRepository, PgCarouselAssignmentRepository, PgGateAssignmentRepository,
            PgResourceAdjustmentSuggestionRepository, PgStandOccupationRepository, PgTurnaroundLinkRepository,
        };
        use fms_infrastructure::repositories::{
            pg_anomaly_repository::PgAnomalyRepository, pg_business_case_repository::PgBusinessCaseRepository,
            pg_dispatch_collaboration_repository::PgDispatchCollaborationRepository,
            pg_dispatch_order_repository::PgDispatchOrderRepository,
            pg_domain_event_outbox_repository::PgDomainEventOutboxRepository, pg_flight_repository::PgFlightRepository,
            pg_terminal_repository::PgTerminalRepository,
        };

        let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
        let outbox_repo = Arc::new(PgDomainEventOutboxRepository::new(pool.clone()));
        let flight_svc = Arc::new(FlightService::new(flight_repo.clone()));
        let flight_writer: Arc<FlightWriter<sqlx::Transaction<'static, sqlx::Postgres>>> = Arc::new(FlightWriter::new(
            flight_repo.clone() as Arc<dyn fms_domain::ports::flight_repository::FlightRepository + Send + Sync>,
            flight_repo.clone()
                as Arc<
                    dyn fms_domain::ports::flight_repository::FlightTransactionalRepository<
                            sqlx::Transaction<'static, sqlx::Postgres>,
                        > + Send
                        + Sync,
                >,
            outbox_repo.clone()
                as Arc<
                    dyn fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository<
                            sqlx::Transaction<'static, sqlx::Postgres>,
                        > + Send
                        + Sync,
                >,
        ));
        let dispatch_repo = Arc::new(PgDispatchOrderRepository::new(pool.clone()));
        // 本测试只接 order_repo 与其事务变体；其余端口是桩（与接线前的 None 行为一致）。
        let mut dispatch_deps = crate::test_support::stub_dispatch_dependencies();
        dispatch_deps.order.order_repo = dispatch_repo.clone();
        let dispatch_svc = Arc::new(DispatchService::new(dispatch_deps));
        let dispatch_writer: Arc<DispatchOrderWriter<sqlx::Transaction<'static, sqlx::Postgres>>> =
            Arc::new(DispatchOrderWriter::new(
                dispatch_repo.clone()
                    as Arc<dyn fms_domain::ports::dispatch_repository::DispatchOrderRepository + Send + Sync>,
                dispatch_repo.clone()
                    as Arc<
                        dyn fms_domain::ports::dispatch_repository::DispatchOrderTransactionalRepository<
                                sqlx::Transaction<'static, sqlx::Postgres>,
                            > + Send
                            + Sync,
                    >,
                Arc::new(crate::test_support::UnwiredRepository)
                    as Arc<dyn fms_domain::ports::dispatch_repository::DispatchOrderMemberRepository + Send + Sync>,
                Arc::new(crate::test_support::UnwiredRepository)
                    as Arc<
                        dyn fms_domain::ports::dispatch_repository::DispatchOrderMemberTransactionalRepository<
                                sqlx::Transaction<'static, sqlx::Postgres>,
                            > + Send
                            + Sync,
                    >,
                Arc::new(crate::test_support::UnwiredRepository)
                    as Arc<dyn fms_domain::ports::dispatch_repository::TeamRepository + Send + Sync>,
                dispatch_svc.clone(),
            ));
        let collab_repo = Arc::new(PgDispatchCollaborationRepository::new(pool.clone()));

        let anomaly_repo = Arc::new(PgAnomalyRepository::new(pool.clone()));

        let business_case_pg_repo = Arc::new(PgBusinessCaseRepository::new(pool.clone()));
        let business_case_repo_port: Arc<
            dyn fms_domain::ports::business_case_repository::BusinessCaseRepository + Send + Sync,
        > = business_case_pg_repo.clone();
        let business_case_writer: Arc<BusinessCaseWriter<sqlx::Transaction<'static, sqlx::Postgres>>> = Arc::new(
            BusinessCaseWriter::new(business_case_pg_repo.clone(), business_case_pg_repo),
        );
        let business_case_collab_repo_port: Arc<
            dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
        > = collab_repo;
        let bc_svc = Arc::new(BusinessCaseService::new(
            business_case_repo_port,
            Arc::new(NoopBusinessCaseEventPublisher) as Arc<dyn BusinessCaseEventPublisher>,
            Arc::new(CollaborationMentionAudience::new(business_case_collab_repo_port))
                as Arc<dyn BusinessCaseMentionAudience>,
        ));

        let ontology_flight_port: Arc<dyn FlightRepository + Send + Sync> = flight_repo.clone();
        let aircraft_repo = Arc::new(PgAircraftRepository::new(pool.clone()));
        let occupation_repo = Arc::new(PgStandOccupationRepository::new(pool.clone()));
        let assignment_repo = Arc::new(PgGateAssignmentRepository::new(pool.clone()));
        let link_repo = Arc::new(PgTurnaroundLinkRepository::new(pool.clone()));
        let suggestion_repo = Arc::new(PgResourceAdjustmentSuggestionRepository::new(pool.clone()));
        let carousel_repo = Arc::new(PgCarouselAssignmentRepository::new(pool.clone()));

        let ontology_tx: Arc<
            dyn OntologyTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync,
        > = aircraft_repo.clone();
        let flight_tx: Arc<dyn FlightTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync> =
            flight_repo.clone();
        let outbox_tx: Arc<
            dyn DomainEventOutboxTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync,
        > = outbox_repo.clone();
        let ontology_writer: Arc<dyn OntologyTransactions> = Arc::new(OntologyWriter::new(
            ontology_flight_port.clone(),
            link_repo.clone(),
            ontology_tx,
            flight_tx,
            outbox_tx,
            Arc::new(fms_infrastructure::db::transaction::PgUnitOfWork::new(pool.clone())),
        ));
        let ontology_svc = Arc::new(OntologyService::new(
            ontology_flight_port.clone(),
            aircraft_repo,
            occupation_repo,
            assignment_repo,
            link_repo,
            suggestion_repo,
            carousel_repo,
            Arc::new(PgTerminalRepository::new(pool.clone())),
            ontology_writer,
        ));

        Arc::new(crate::services::domain_action_executor::DomainActionExecutor::new(
            flight_svc,
            flight_writer,
            dispatch_svc,
            dispatch_writer,
            bc_svc,
            business_case_writer,
            ontology_svc,
            outbox_repo,
            anomaly_repo,
            Arc::new(fms_infrastructure::db::transaction::PgUnitOfWork::new(pool)),
        ))
    }

    fn build_smoke_proposal_service(
        pool: sqlx::PgPool,
        executor: Arc<
            crate::services::domain_action_executor::DomainActionExecutor<
                fms_infrastructure::db::transaction::PgUnitOfWork,
            >,
        >,
        readiness: Arc<AiExecutionReadinessService>,
        audit: Arc<dyn AiProposalAuditEventRecorder>,
    ) -> AiActionProposalService {
        use fms_infrastructure::repositories::{
            pg_ai_object_policy_repository::PgAiObjectPolicyRepository,
            pg_ai_ontology_repository::PgAiOntologyRepository, pg_ai_proposal_repository::PgAiProposalRepository,
        };

        AiActionProposalService::new()
            .with_repository(Arc::new(PgAiProposalRepository::new(pool.clone())))
            .with_domain_action_executor(executor)
            .with_object_policy_repository(Arc::new(PgAiObjectPolicyRepository::new(pool.clone())))
            .with_ontology_repository(Arc::new(PgAiOntologyRepository::new(pool.clone())))
            .with_readiness_service(readiness)
            .with_audit_recorder(audit)
    }

    fn smoke_add_note_proposal(
        proposal_id: &str,
        correlation_id: &str,
        flight_id: &str,
    ) -> fms_domain::models::ai_proposal::AiActionProposal {
        fms_domain::models::ai_proposal::AiActionProposal {
            proposal_id: proposal_id.to_string(),
            job_id: format!("smoke_job_{proposal_id}"),
            run_id: format!("smoke_run_{proposal_id}"),
            ontology_version: "flight-ops.v1".to_string(),
            object_type: "Flight".to_string(),
            object_id: flight_id.to_string(),
            action_name: "add_note".to_string(),
            arguments: json!({ "note_content": format!("Smoke test note {correlation_id}") }),
            risk_level: RiskLevel::Low,
            required_permissions: vec!["flight:write".to_string()],
            approval_policy: ApprovalPolicy::AutoExecute,
            before_snapshot: None,
            after_preview: None,
            constraint_results: vec![],
            confidence: 0.95,
            reasoning: "staging smoke Flight.add_note".to_string(),
            status: ActionProposalStatus::Approved,
            pending_action_id: None,
            approved_by: Some("smoke_approver".to_string()),
            approved_at: Some(chrono::Utc::now()),
            rejected_by: None,
            rejected_reason: None,
            rejected_at: None,
            executed_by: None,
            executed_at: None,
            execution_result: None,
            execution_error: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            correlation_id: Some(correlation_id.to_string()),
            metadata: json!({ "smoke": true }),
        }
    }

    async fn seed_smoke_flight(pool: &sqlx::PgPool, flight_id: &str) {
        sqlx::query(
            r#"INSERT INTO flights (
                flight_id, airline_code, flight_number, registration,
                aircraft_type_detail, status,
                scheduled_departure, scheduled_arrival,
                estimated_departure, estimated_arrival,
                actual_departure, actual_arrival,
                cobt_time, codt,
                gate, stand, terminal, position, baggage_carousel,
                has_boarding_restriction, is_quick_turnaround, is_commercial_signed,
                created_at, updated_at, version,
                flight_remarks, load_planning_remarks,
                aircraft_maintenance_remarks, aircraft_check_remarks
            ) VALUES (
                $1, 'CA', $2, NULL,
                NULL, 0,
                NOW(), NOW() + INTERVAL '2 hours',
                NULL, NULL,
                NULL, NULL,
                NULL, NULL,
                'A12', 'S1', 'T1', NULL, NULL,
                FALSE, FALSE, TRUE,
                NOW(), NOW(), 1,
                NULL, NULL, NULL, NULL
            ) ON CONFLICT (flight_id) DO NOTHING"#,
        )
        .bind(flight_id)
        .bind(format!("CA{}", &flight_id[..flight_id.len().min(4)]))
        .execute(pool)
        .await
        .unwrap();
    }

    /// 烟测用唯一航班 id（满足 seed_smoke_flight 的 flight_number 切片 ≥4 位）。
    fn smoke_flight_id(correlation_id: &str) -> String {
        format!("SMK{}", &correlation_id[correlation_id.len() - 8..])
    }

    async fn smoke_audit_event_types(
        pool: &sqlx::PgPool,
        proposal: &fms_domain::models::ai_proposal::AiActionProposal,
    ) -> Vec<String> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT event_type FROM ai_run_events WHERE job_id = $1 AND run_id = $2 ORDER BY created_at ASC",
        )
        .bind(&proposal.job_id)
        .bind(&proposal.run_id)
        .fetch_all(pool)
        .await
        .unwrap();

        rows.into_iter().map(|row| row.0).collect()
    }

    // ── Staging Smoke Tests ────────────────────────────────────────────

    // 三个 smoke 测试都会读写 FMS_AI_PROPOSAL_EXECUTION_ENABLED 等进程级环境变量，
    // 并行执行会互相覆盖导致 flaky，用静态锁串行化环境变量相关段落。
    static SMOKE_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL — run via scripts/dev/run_aip_staging_smoke.ps1"]
    async fn staging_smoke_add_note_executes_end_to_end() {
        use crate::services::ai_proposal_audit_recorder::PgAiProposalAuditEventRecorder;

        let pool = smoke_pool().await;
        let executor = build_smoke_executor(pool.clone()).await;
        let readiness = Arc::new(AiExecutionReadinessService::new(
            Some(Arc::new(fms_infrastructure::PgDatabaseMetadataAdapter::new(
                pool.clone(),
            ))),
            None,
        ));
        let audit = Arc::new(PgAiProposalAuditEventRecorder::new(Arc::new(
            fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
        )));

        let service = build_smoke_proposal_service(pool.clone(), executor, readiness, audit.clone());

        let correlation_id = ulid::Ulid::new().to_string();
        let proposal_id = ulid::Ulid::new().to_string();
        let flight_id = smoke_flight_id(&correlation_id);
        seed_smoke_flight(&pool, &flight_id).await;
        let proposal = smoke_add_note_proposal(&proposal_id, &correlation_id, &flight_id);

        // Persist the approved proposal to DB
        service.test_repository().unwrap().save(&proposal).await.unwrap();

        let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
        // Set env vars for execution
        std::env::set_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
        std::env::set_var("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal_id.clone(),
                executor_id: "smoke_executor".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;

        // Clean up env vars
        std::env::remove_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED");
        std::env::remove_var("FMS_AI_EXECUTION_READINESS_OVERRIDE");

        let executed = result.expect("smoke execution should succeed");

        // 1. Proposal status is Executed
        assert_eq!(executed.status, ActionProposalStatus::Executed);
        assert!(executed.execution_result.is_some());

        // 2. Flight business row reflects the note (Flight.add_note 写回 flight_remarks)
        let remarks: Option<String> =
            sqlx::query_scalar("SELECT flight_remarks FROM flights WHERE flight_id = $1")
                .bind(&flight_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            remarks.as_deref(),
            Some(format!("Smoke test note {correlation_id}").as_str()),
            "flight_remarks should be written by Flight.add_note smoke execution"
        );

        // 3. Domain event outbox has a flight.remarks_updated_v2 event for the flight
        let outbox_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'flight.remarks_updated_v2' AND aggregate_id = $1",
        )
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            outbox_count.0 >= 1,
            "outbox should contain flight.remarks_updated_v2 event"
        );

        // 4. Audit events recorded
        let event_types = smoke_audit_event_types(&pool, &proposal).await;
        assert!(event_types.contains(&"proposal.execution_requested".to_string()));
        assert!(event_types.contains(&"proposal.execution_started".to_string()));
        assert!(event_types.contains(&"proposal.execution_succeeded".to_string()));
        assert!(!event_types.contains(&"proposal.execution_blocked_readiness".to_string()));
        assert!(!event_types.contains(&"proposal.execution_failed".to_string()));
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL — run via scripts/dev/run_aip_staging_smoke.ps1"]
    async fn staging_smoke_execution_disabled_blocks_proposal() {
        use crate::services::ai_proposal_audit_recorder::PgAiProposalAuditEventRecorder;

        let pool = smoke_pool().await;
        let executor = build_smoke_executor(pool.clone()).await;
        let readiness = Arc::new(AiExecutionReadinessService::new(
            Some(Arc::new(fms_infrastructure::PgDatabaseMetadataAdapter::new(
                pool.clone(),
            ))),
            None,
        ));
        let audit = Arc::new(PgAiProposalAuditEventRecorder::new(Arc::new(
            fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
        )));

        let service = build_smoke_proposal_service(pool.clone(), executor, readiness, audit.clone());

        let correlation_id = ulid::Ulid::new().to_string();
        let proposal_id = ulid::Ulid::new().to_string();
        let flight_id = smoke_flight_id(&correlation_id);
        seed_smoke_flight(&pool, &flight_id).await;
        let proposal = smoke_add_note_proposal(&proposal_id, &correlation_id, &flight_id);
        service.test_repository().unwrap().save(&proposal).await.unwrap();

        let remarks_before: Option<String> =
            sqlx::query_scalar("SELECT flight_remarks FROM flights WHERE flight_id = $1")
                .bind(&flight_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_before: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'flight.remarks_updated_v2' AND aggregate_id = $1",
        )
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
        // Execution DISABLED
        std::env::set_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "false");
        std::env::remove_var("FMS_AI_EXECUTION_READINESS_OVERRIDE");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal_id.clone(),
                executor_id: "smoke_executor".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;

        std::env::remove_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED");

        // Must be rejected
        assert!(result.is_err(), "execution must be blocked when flag is off");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("FMS_AI_PROPOSAL_EXECUTION_ENABLED"),
            "error should mention the feature flag, got: {err}"
        );

        // Proposal remains Approved (not Executed)
        let reloaded = service
            .test_repository()
            .unwrap()
            .find_by_id(&proposal_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            reloaded.status,
            ActionProposalStatus::Approved,
            "proposal should remain Approved"
        );

        let remarks_after: Option<String> =
            sqlx::query_scalar("SELECT flight_remarks FROM flights WHERE flight_id = $1")
                .bind(&flight_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'flight.remarks_updated_v2' AND aggregate_id = $1",
        )
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            remarks_after, remarks_before,
            "execution-disabled smoke must not modify flight remarks"
        );
        assert_eq!(
            outbox_count_after.0, outbox_count_before.0,
            "execution-disabled smoke must not create outbox events"
        );

        let event_types = smoke_audit_event_types(&pool, &proposal).await;
        assert!(event_types.contains(&"proposal.execution_requested".to_string()));
        assert!(!event_types.contains(&"proposal.execution_started".to_string()));
        assert!(!event_types.contains(&"proposal.execution_succeeded".to_string()));
    }

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL — run via scripts/dev/run_aip_staging_smoke.ps1"]
    async fn staging_smoke_readiness_not_ready_blocks_and_records_audit() {
        use crate::services::ai_proposal_audit_recorder::PgAiProposalAuditEventRecorder;

        let pool = smoke_pool().await;
        let executor = build_smoke_executor(pool.clone()).await;
        // No pool → static checks only, no DB checks. With execution enabled but
        // no staging override, readiness will fail.
        let readiness = Arc::new(AiExecutionReadinessService::new(None, None));
        let audit = Arc::new(PgAiProposalAuditEventRecorder::new(Arc::new(
            fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository::new(pool.clone()),
        )));

        let service = build_smoke_proposal_service(pool.clone(), executor, readiness, audit.clone());

        let correlation_id = ulid::Ulid::new().to_string();
        let proposal_id = ulid::Ulid::new().to_string();
        let flight_id = smoke_flight_id(&correlation_id);
        seed_smoke_flight(&pool, &flight_id).await;
        let proposal = smoke_add_note_proposal(&proposal_id, &correlation_id, &flight_id);
        service.test_repository().unwrap().save(&proposal).await.unwrap();

        let remarks_before: Option<String> =
            sqlx::query_scalar("SELECT flight_remarks FROM flights WHERE flight_id = $1")
                .bind(&flight_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_before: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'flight.remarks_updated_v2' AND aggregate_id = $1",
        )
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let _env_guard = SMOKE_ENV_LOCK.lock().unwrap();
        // Execution enabled but NO staging override → readiness fails
        std::env::set_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
        std::env::remove_var("FMS_AI_EXECUTION_READINESS_OVERRIDE");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal_id.clone(),
                executor_id: "smoke_executor".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;

        std::env::remove_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED");

        assert!(result.is_err(), "execution must be blocked when readiness is not ready");

        // Audit event recorded
        let event_types = smoke_audit_event_types(&pool, &proposal).await;
        assert!(
            event_types.contains(&"proposal.execution_blocked_readiness".to_string()),
            "should record readiness block audit event, got: {:?}",
            event_types
        );

        // No execution_started or execution_succeeded
        assert!(!event_types.contains(&"proposal.execution_started".to_string()));
        assert!(!event_types.contains(&"proposal.execution_succeeded".to_string()));

        // Proposal remains Approved
        let reloaded = service
            .test_repository()
            .unwrap()
            .find_by_id(&proposal_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reloaded.status, ActionProposalStatus::Approved);

        let remarks_after: Option<String> =
            sqlx::query_scalar("SELECT flight_remarks FROM flights WHERE flight_id = $1")
                .bind(&flight_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'flight.remarks_updated_v2' AND aggregate_id = $1",
        )
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            remarks_after, remarks_before,
            "readiness-not-ready smoke must not modify flight remarks"
        );
        assert_eq!(
            outbox_count_after.0, outbox_count_before.0,
            "readiness-not-ready smoke must not create outbox events"
        );
    }

    #[test]
    fn execution_allowlist_blocks_unlisted_action() {
        let allowlist = ExecutionAllowlist::parse("Todo.create");
        assert!(!allowlist.allows("Notification", "send"));
    }

    #[test]
    fn execution_allowlist_allows_listed_action() {
        let allowlist = ExecutionAllowlist::parse("Todo.create");
        assert!(allowlist.allows("Todo", "create"));
    }

    // ── proposal 管线消费 schema 的接线用例 ──────────

    fn generate_request_for(
        job_id: &str,
        object_type: &str,
        object_id: &str,
        action_name: &str,
        arguments: serde_json::Value,
        permissions: &[&str],
    ) -> GenerateProposalRequest {
        GenerateProposalRequest {
            job_id: job_id.to_string(),
            run_id: "run_1".to_string(),
            ontology_version: Some("flight-ops.v1".to_string()),
            object_type: object_type.to_string(),
            object_id: object_id.to_string(),
            action_name: action_name.to_string(),
            arguments,
            reasoning: Some("test proposal".to_string()),
            confidence: Some(0.9),
            requester_user_id: Some("requester_1".to_string()),
            requester_user_roles: permissions.iter().map(|item| item.to_string()).collect(),
            requester_department_id: Some("ops-1".to_string()),
            correlation_id: Some("corr_1".to_string()),
            idempotency_key: None,
            expected_object_version: None,
            risk_level: None,
            approval_policy: None,
            required_permissions: None,
        }
    }

    fn schema_risk_policy(def: &fms_domain::models::ai_ontology::OntologyActionDef) -> (RiskLevel, ApprovalPolicy) {
        let risk = match def.risk_level.as_str() {
            "critical" => RiskLevel::Critical,
            "high" => RiskLevel::High,
            "medium" => RiskLevel::Medium,
            _ => RiskLevel::Low,
        };
        let declared_policy = match def.approval_policy.as_str() {
            "require_supervisor_approval" => ApprovalPolicy::RequireSupervisorApproval,
            "require_flowable_approval" => ApprovalPolicy::RequireFlowableApproval,
            "require_approval" => ApprovalPolicy::RequireApproval,
            _ => ApprovalPolicy::AutoExecute,
        };
        (risk, declared_policy)
    }

    // 每个 schema 动作的 proposal 必须携带 schema 声明的
    // 风险等级、审批策略与权限（单一事实来源，不得硬编码漂移）。
    #[tokio::test]
    async fn generated_proposals_carry_schema_risk_policy_and_permissions_for_every_action() {
        let schema = fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema();
        let service = AiActionProposalService::new();
        let mut checked = 0usize;

        for (object_type, object) in &schema.objects {
            for (action_name, def) in &object.actions {
                checked += 1;
                let perms: Vec<&str> = def.required_permissions.iter().map(|item| item.as_str()).collect();
                let request = generate_request_for(
                    &format!("job_schema_{checked}"),
                    object_type,
                    "obj_1",
                    action_name,
                    json!({}),
                    &perms,
                );
                let proposal = service
                    .generate_proposal(request)
                    .await
                    .unwrap_or_else(|e| panic!("{object_type}.{action_name} should generate: {e}"));

                let (expected_risk, expected_policy) = schema_risk_policy(def);
                assert_eq!(
                    proposal.required_permissions, def.required_permissions,
                    "{object_type}.{action_name} permissions must come from schema"
                );
                assert_eq!(
                    proposal.risk_level, expected_risk,
                    "{object_type}.{action_name} risk must come from schema"
                );
                assert_eq!(
                    proposal.approval_policy, expected_policy,
                    "{object_type}.{action_name} approval policy must come from schema"
                );
            }
        }

        assert!(
            checked >= 30,
            "expected the full flight-ops.v1 action set, got {checked}"
        );
    }

    // 每个 write 动作在无权限时必须被拒绝（禁止绕过资源权限）。
    #[tokio::test]
    async fn every_schema_write_action_is_forbidden_without_permissions() {
        let schema = fms_domain::ontology::flight_ops_v1::build_flight_ops_v1_schema();
        let service = AiActionProposalService::new();
        let mut checked = 0usize;

        for (object_type, object) in &schema.objects {
            for (action_name, def) in &object.actions {
                if def.category != "write" {
                    continue;
                }
                checked += 1;
                assert!(
                    !def.required_permissions.is_empty(),
                    "{object_type}.{action_name} write action must declare required permissions"
                );
                let request = generate_request_for(
                    &format!("job_forbid_{checked}"),
                    object_type,
                    "obj_1",
                    action_name,
                    json!({}),
                    &[],
                );
                let result = service.generate_proposal(request).await;
                assert!(
                    matches!(result, Err(AiActionProposalError::Forbidden(_))),
                    "{object_type}.{action_name} must be forbidden without permissions, got {:?}",
                    result
                );
            }
        }

        assert!(
            checked >= 10,
            "expected at least the 10 contract write actions, got {checked}"
        );
    }

    // 过期 proposal 不能审批也不能执行。
    #[tokio::test]
    async fn expired_proposal_cannot_be_approved_or_executed() {
        use crate::services::in_memory_ai_proposal_repository::InMemoryAiProposalRepository;
        use chrono::Utc;
        use fms_domain::models::ai_proposal::AiActionProposal;
        use fms_domain::ports::ai_proposal_repository::AiProposalRepository;

        let repo = Arc::new(InMemoryAiProposalRepository::new());
        let service = AiActionProposalService::new().with_repository(repo.clone());

        // 过期且停在 Pending：审批必须被拒。
        let mut pending = AiActionProposal::new(
            "prop_expired_pending",
            "job_expired",
            "run_expired",
            "Flight",
            "flt_1",
            "update_status",
            json!({"new_status": "delayed", "reason": "conflict"}),
        )
        .with_expires_at(Utc::now() - chrono::Duration::minutes(1));
        pending.required_permissions = vec!["flight:write".to_string()];
        pending
            .transition_to(ActionProposalStatus::Validating)
            .expect("to validating");
        pending
            .transition_to(ActionProposalStatus::Pending)
            .expect("to pending");
        repo.save(&pending).await.expect("persist pending proposal");

        let approve_result = service
            .approve_proposal(ApproveProposalRequest {
                proposal_id: pending.proposal_id.clone(),
                approver_id: "approver_1".to_string(),
                approver_permissions: vec!["flight:write".to_string()],
                approver_department_id: None,
                modified_arguments: None,
            })
            .await;
        assert!(
            matches!(approve_result, Err(AiActionProposalError::Conflict(ref msg)) if msg.contains("expired")),
            "expected expired conflict on approve, got {:?}",
            approve_result
        );

        // 过期且已 Approved：执行必须被拒。
        let mut approved = AiActionProposal::new(
            "prop_expired_approved",
            "job_expired",
            "run_expired",
            "Flight",
            "flt_1",
            "update_status",
            json!({"new_status": "delayed", "reason": "conflict"}),
        )
        .with_expires_at(Utc::now() - chrono::Duration::minutes(1));
        approved.required_permissions = vec!["flight:write".to_string()];
        approved
            .transition_to(ActionProposalStatus::Validating)
            .expect("to validating");
        approved
            .transition_to(ActionProposalStatus::Pending)
            .expect("to pending");
        approved.approve("approver_1").expect("approve");
        repo.save(&approved).await.expect("persist approved proposal");

        let execute_result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: approved.proposal_id.clone(),
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;
        assert!(
            matches!(execute_result, Err(AiActionProposalError::Conflict(ref msg)) if msg.contains("expired")),
            "expected expired conflict on execute, got {:?}",
            execute_result
        );
    }

    // 同一 idempotency key 已执行过的动作不得重复执行。
    #[tokio::test]
    async fn execute_proposal_rejects_duplicate_idempotency_key() {
        use crate::services::in_memory_ai_proposal_repository::InMemoryAiProposalRepository;
        use fms_domain::models::ai_proposal::AiActionProposal;
        use fms_domain::ports::ai_proposal_repository::AiProposalRepository;

        let repo = Arc::new(InMemoryAiProposalRepository::new());

        // 预置一条已执行、携带同一幂等键的 proposal。
        let mut executed = AiActionProposal::new(
            "prop_dup_first",
            "job_dup",
            "run_dup",
            "Flight",
            "FL_DUP",
            "update_status",
            json!({"new_status": "delayed"}),
        )
        .with_metadata(json!({"idempotency_key": "dup_key_1"}));
        executed.required_permissions = vec!["flight:write".to_string()];
        executed
            .transition_to(ActionProposalStatus::Validating)
            .expect("to validating");
        executed
            .transition_to(ActionProposalStatus::Pending)
            .expect("to pending");
        executed.approve("approver_1").expect("approve");
        executed
            .transition_to(ActionProposalStatus::Executing)
            .expect("to executing");
        executed.mark_executed("executor_1", json!({}));
        repo.save(&executed).await.expect("persist executed proposal");

        let service = AiActionProposalService::new()
            .with_repository(repo.clone())
            .with_proposal_execution_enabled_for_test(true);

        let mut request = generate_request_for(
            "job_dup",
            "Flight",
            "FL_DUP",
            "update_status",
            json!({"new_status": "delayed"}),
            &["flight:write"],
        );
        request.idempotency_key = Some("dup_key_1".to_string());
        let proposal = service
            .generate_proposal(request)
            .await
            .expect("generate duplicate proposal");

        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({})),
                after_preview: Some(json!({})),
                constraint_results: None,
            })
            .await
            .expect("proposal should validate");

        // 低风险动作 validate 后可能已自动批准，仅当仍处 Pending 时才显式审批。
        let proposal = if proposal.status == ActionProposalStatus::Pending {
            service
                .approve_proposal(ApproveProposalRequest {
                    proposal_id: proposal.proposal_id.clone(),
                    approver_id: "approver_1".to_string(),
                    approver_permissions: vec!["flight:write".to_string()],
                    approver_department_id: None,
                    modified_arguments: None,
                })
                .await
                .expect("proposal should approve")
        } else {
            proposal
        };

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal.proposal_id,
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;
        assert!(
            matches!(result, Err(AiActionProposalError::Conflict(ref msg)) if msg.contains("already been executed")),
            "expected duplicate idempotency conflict, got {:?}",
            result
        );
    }

    // 执行前重验对象版本，版本不一致必须拒绝执行。
    #[tokio::test]
    async fn execute_proposal_rejects_flight_version_conflict() {
        use crate::services::ontology_actions::test_support::FakeFlightRepo;

        let flight_repo = Arc::new(FakeFlightRepo::default());
        flight_repo.flights.lock().unwrap().push(versioned_flight("flt_1", 2));

        let service = AiActionProposalService::new()
            .with_proposal_execution_enabled_for_test(true)
            .with_flight_repository(flight_repo);

        let mut request = generate_request_for(
            "job_version",
            "Flight",
            "flt_1",
            "update_status",
            json!({"new_status": "delayed", "reason": "conflict"}),
            &["flight:write"],
        );
        request.expected_object_version = Some(1);

        let proposal = service.generate_proposal(request).await.expect("generate proposal");
        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({"stand": "S01"})),
                after_preview: Some(json!({"stand": "S02"})),
                constraint_results: None,
            })
            .await
            .expect("proposal should validate");
        let proposal = service
            .approve_proposal(ApproveProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                approver_id: "approver_1".to_string(),
                approver_permissions: vec!["flight:write".to_string()],
                approver_department_id: None,
                modified_arguments: None,
            })
            .await
            .expect("proposal should approve");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal.proposal_id,
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;
        assert!(
            matches!(result, Err(AiActionProposalError::Conflict(ref msg)) if msg.contains("version mismatch")),
            "expected version conflict, got {:?}",
            result
        );
    }

    // 拒绝是终态：被拒绝的 proposal 不得再被执行。
    #[tokio::test]
    async fn rejected_proposal_cannot_be_executed() {
        let service = AiActionProposalService::new();
        let proposal = service
            .generate_proposal(generate_request(
                "update_status",
                json!({"new_status": "delayed", "reason": "conflict"}),
                &["flight:write"],
            ))
            .await
            .expect("generate proposal");
        let proposal = service
            .validate_proposal(ValidateProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                before_snapshot: Some(json!({})),
                after_preview: Some(json!({})),
                constraint_results: None,
            })
            .await
            .expect("proposal should validate");

        let rejected = service
            .reject_proposal(crate::services::ai_action_proposal_service::RejectProposalRequest {
                proposal_id: proposal.proposal_id.clone(),
                rejecter_id: "rejecter_1".to_string(),
                reason: "not needed".to_string(),
            })
            .await
            .expect("proposal should reject");
        assert_eq!(rejected.status, ActionProposalStatus::Rejected);

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal.proposal_id,
                executor_id: "executor_1".to_string(),
                executor_permissions: vec!["flight:write".to_string()],
                executor_department_id: None,
            })
            .await;
        assert!(
            matches!(result, Err(AiActionProposalError::Conflict(_))),
            "expected conflict executing rejected proposal, got {:?}",
            result
        );
    }

    fn versioned_flight(flight_id: &str, version: i32) -> fms_domain::models::flight::Flight {
        use fms_domain::models::flight::Flight;
        use fms_domain::models::value_objects::{FlightId, FlightNumber, FlightStatus};

        let now = chrono::Utc::now();
        Flight {
            flight_id: FlightId::from(flight_id),
            airline_code: Some("CZ".to_string()),
            flight_number: Some(FlightNumber::from("CZ3000")),
            registration: None,
            aircraft_type_detail: None,
            stand: None,
            gate: None,
            terminal: None,
            position: None,
            baggage_carousel: None,
            scheduled_departure: Some(now),
            scheduled_arrival: Some(now),
            estimated_departure: Some(now),
            estimated_arrival: Some(now),
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            status: FlightStatus::default(),
            inbound_leg: None,
            outbound_leg: None,
            anomaly_summary: Default::default(),
            created_at: now,
            updated_at: now,
            version,
            labels: vec![],
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
            direction: None,
            flight_kind: "passenger".to_string(),
            is_draft: false,
            divert: false,
        }
    }
}
