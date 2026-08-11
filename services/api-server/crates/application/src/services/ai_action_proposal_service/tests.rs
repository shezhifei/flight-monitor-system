#[cfg(test)]
mod tests {
    use crate::services::ai_action_proposal_service::{
        AiActionProposalError, AiActionProposalService, ApproveProposalRequest, ExecuteProposalRequest,
        GenerateProposalRequest, ValidateProposalRequest,
    };
    use crate::services::ai_execution_allowlist::ExecutionAllowlist;
    use crate::services::ai_execution_readiness_service::AiExecutionReadinessService;
    use crate::services::ai_proposal_audit_recorder::AiProposalAuditEventRecorder;
    use crate::services::dispatch_service::DispatchService;
    use crate::types::ConcreteNotificationService;
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
                "change_stand",
                json!({"new_stand_id": "S02", "reason": "conflict"}),
                &["flight:read"],
            ))
            .await;

        assert!(matches!(result, Err(AiActionProposalError::Forbidden(_))));
    }

    #[tokio::test]
    async fn generate_proposal_rejects_object_policy_deny() {
        let policy_repo = Arc::new(SequenceObjectPolicyRepository::new(vec![AiObjectAccessDecision::Deny]));
        let service = AiActionProposalService::new().with_object_policy_repository(policy_repo);

        let result = service
            .generate_proposal(generate_request(
                "change_stand",
                json!({"new_stand_id": "S02", "reason": "conflict"}),
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
                "change_stand",
                json!({"new_stand_id": "S02", "reason": "conflict"}),
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
                "change_stand",
                json!({"new_stand_id": "S02", "reason": "conflict"}),
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
    async fn generate_proposal_normalizes_registry_approval_policy_by_risk() {
        let service = AiActionProposalService::new();
        let mut medium_request = generate_request("add_note", json!({"note_content": "ops note"}), &["flight:write"]);
        medium_request.risk_level = Some(RiskLevel::Medium);
        medium_request.approval_policy = Some(ApprovalPolicy::AutoExecute);

        let medium = service
            .generate_proposal(medium_request)
            .await
            .expect("authorized generator should create medium proposal");

        assert_eq!(medium.risk_level, RiskLevel::Medium);
        assert_eq!(medium.approval_policy, ApprovalPolicy::RequireApproval);

        let mut critical_request = generate_request("add_note", json!({"note_content": "ops note"}), &["flight:write"]);
        critical_request.risk_level = Some(RiskLevel::Critical);
        critical_request.approval_policy = Some(ApprovalPolicy::AutoExecute);

        let critical = service
            .generate_proposal(critical_request)
            .await
            .expect("authorized generator should create critical proposal");

        assert_eq!(critical.risk_level, RiskLevel::Critical);
        assert_eq!(critical.approval_policy, ApprovalPolicy::RequireSupervisorApproval);
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
    ) -> Arc<crate::services::domain_action_executor::DomainActionExecutor> {
        use crate::services::anomaly_service::AnomalyService;
        use crate::services::business_case_service::{BusinessCaseEventPublisher, BusinessCaseService};
        use crate::services::flight_service::FlightService;
        use crate::services::label_service::LabelService;
        use crate::services::notification_service::{
            NotificationDeliveryPublisher, NotificationMetricsRecorder, NotificationReceiptGroupSync,
            NotificationService,
        };
        use crate::services::todo_service::TodoService;
        use crate::types::{
            NoopBroadcaster, NoopBusinessCaseEventPublisher, NoopNotificationDeliveryPublisher,
            NoopNotificationMetricsRecorder, NoopNotificationReceiptGroupSync,
        };
        use fms_infrastructure::repositories::{
            pg_anomaly_repository::PgAnomalyRepository, pg_business_case_repository::PgBusinessCaseRepository,
            pg_dispatch_collaboration_repository::PgDispatchCollaborationRepository,
            pg_dispatch_order_repository::PgDispatchOrderRepository, pg_flight_repository::PgFlightRepository,
            pg_label_repository::PgLabelRepository, pg_notification_repository::PgNotificationRepository,
            pg_todo_repository::PgTodoRepository,
        };

        let flight_svc = Arc::new(FlightService::new(Arc::new(PgFlightRepository::new(pool.clone()))));
        let dispatch_svc = Arc::new(DispatchService::new(Arc::new(PgDispatchOrderRepository::new(
            pool.clone(),
        ))));
        let notif_repo = Arc::new(PgNotificationRepository::new(pool.clone()));
        let collab_repo = Arc::new(PgDispatchCollaborationRepository::new(pool.clone()));
        let notif_repo_port: Arc<dyn fms_domain::ports::notification_repository::NotificationRepository + Send + Sync> =
            notif_repo.clone();
        let notif_pref_repo_port: Arc<
            dyn fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync,
        > = notif_repo.clone();
        let collab_repo_port: Arc<
            dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
        > = collab_repo.clone();
        let notif_svc: Arc<ConcreteNotificationService> =
            Arc::new(
                NotificationService::new(notif_repo_port, notif_pref_repo_port)
                    .with_collaboration_repo(collab_repo_port)
                    .with_metrics_recorder(
                        Arc::new(NoopNotificationMetricsRecorder) as Arc<dyn NotificationMetricsRecorder>
                    )
                    .with_delivery_publisher(
                        Arc::new(NoopNotificationDeliveryPublisher) as Arc<dyn NotificationDeliveryPublisher>
                    )
                    .with_receipt_group_sync(
                        Arc::new(NoopNotificationReceiptGroupSync) as Arc<dyn NotificationReceiptGroupSync>
                    ),
            );
        let anomaly_svc = Arc::new(AnomalyService::new(Arc::new(PgAnomalyRepository::new(pool.clone()))));
        let label_svc = Arc::new(LabelService::new(
            Arc::new(PgLabelRepository::new(pool.clone())),
            Arc::new(NoopBroadcaster),
        ));
        let todo_svc = Arc::new(TodoService::new(Arc::new(PgTodoRepository::new(pool.clone()))));
        let business_case_repo_port: Arc<
            dyn fms_domain::ports::business_case_repository::BusinessCaseRepository + Send + Sync,
        > = Arc::new(PgBusinessCaseRepository::new(pool.clone()));
        let business_case_collab_repo_port: Arc<
            dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
        > = collab_repo;
        let bc_svc = Arc::new(
            BusinessCaseService::new(business_case_repo_port)
                .with_event_publisher(Arc::new(NoopBusinessCaseEventPublisher) as Arc<dyn BusinessCaseEventPublisher>)
                .with_dispatch_chat_repository(business_case_collab_repo_port),
        );

        Arc::new(crate::services::domain_action_executor::DomainActionExecutor::new(
            flight_svc,
            dispatch_svc,
            notif_svc,
            anomaly_svc,
            label_svc,
            todo_svc,
            bc_svc,
            pool,
        ))
    }

    fn build_smoke_proposal_service(
        pool: sqlx::PgPool,
        executor: Arc<crate::services::domain_action_executor::DomainActionExecutor>,
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
            .with_pool(pool)
            .with_readiness_service(readiness)
            .with_audit_recorder(audit)
    }

    fn smoke_todo_create_proposal(
        proposal_id: &str,
        correlation_id: &str,
    ) -> fms_domain::models::ai_proposal::AiActionProposal {
        fms_domain::models::ai_proposal::AiActionProposal {
            proposal_id: proposal_id.to_string(),
            job_id: format!("smoke_job_{proposal_id}"),
            run_id: format!("smoke_run_{proposal_id}"),
            ontology_version: "flight-ops.v1".to_string(),
            object_type: "Todo".to_string(),
            object_id: proposal_id.to_string(),
            action_name: "create".to_string(),
            arguments: json!({ "title": format!("Smoke test todo {correlation_id}") }),
            risk_level: RiskLevel::Low,
            required_permissions: vec!["todo:write".to_string()],
            approval_policy: ApprovalPolicy::AutoExecute,
            before_snapshot: None,
            after_preview: None,
            constraint_results: vec![],
            confidence: 0.95,
            reasoning: "staging smoke Todo.create".to_string(),
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

    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL — run via scripts/dev/run_aip_staging_smoke.ps1"]
    async fn staging_smoke_todo_create_executes_end_to_end() {
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
        let proposal = smoke_todo_create_proposal(&proposal_id, &correlation_id);

        // Persist the approved proposal to DB
        service.test_repository().unwrap().save(&proposal).await.unwrap();

        // Set env vars for execution
        std::env::set_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
        std::env::set_var("FMS_AI_EXECUTION_READINESS_OVERRIDE", "staging");
        std::env::set_var("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED", "false");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal_id.clone(),
                executor_id: "smoke_executor".to_string(),
                executor_permissions: vec!["todo:write".to_string()],
                executor_department_id: None,
            })
            .await;

        // Clean up env vars
        std::env::remove_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED");
        std::env::remove_var("FMS_AI_EXECUTION_READINESS_OVERRIDE");
        std::env::remove_var("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED");

        let executed = result.expect("smoke execution should succeed");

        // 1. Proposal status is Executed
        assert_eq!(executed.status, ActionProposalStatus::Executed);
        assert!(executed.execution_result.is_some());

        // 2. Todo business row exists
        let todo_row: Option<(String,)> =
            sqlx::query_as("SELECT todo_id FROM todos WHERE source_type = 'ai_action' AND source_id = $1 LIMIT 1")
                .bind(&proposal_id)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(todo_row.is_some(), "todo row should exist after smoke execution");

        // 3. Domain event outbox has a Todo.create event
        let outbox_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'Todo.create' AND aggregate_id = $1 AND payload->>'executor_id' = 'smoke_executor'",
        )
        .bind(&proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(outbox_count.0 >= 1, "outbox should contain Todo.create event");

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
        let proposal = smoke_todo_create_proposal(&proposal_id, &correlation_id);
        service.test_repository().unwrap().save(&proposal).await.unwrap();

        let todo_count_before: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM todos WHERE source_type = 'ai_action' AND source_id = $1")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_before: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'Todo.create' AND aggregate_id = $1",
        )
        .bind(&proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Execution DISABLED
        std::env::set_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "false");
        std::env::remove_var("FMS_AI_EXECUTION_READINESS_OVERRIDE");
        std::env::set_var("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED", "false");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal_id.clone(),
                executor_id: "smoke_executor".to_string(),
                executor_permissions: vec!["todo:write".to_string()],
                executor_department_id: None,
            })
            .await;

        std::env::remove_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED");
        std::env::remove_var("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED");

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

        let todo_count_after: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM todos WHERE source_type = 'ai_action' AND source_id = $1")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'Todo.create' AND aggregate_id = $1",
        )
        .bind(&proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            todo_count_after.0, todo_count_before.0,
            "execution-disabled smoke must not create todo rows"
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
        let proposal = smoke_todo_create_proposal(&proposal_id, &correlation_id);
        service.test_repository().unwrap().save(&proposal).await.unwrap();

        let todo_count_before: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM todos WHERE source_type = 'ai_action' AND source_id = $1")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_before: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'Todo.create' AND aggregate_id = $1",
        )
        .bind(&proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Execution enabled but NO staging override → readiness fails
        std::env::set_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED", "true");
        std::env::remove_var("FMS_AI_EXECUTION_READINESS_OVERRIDE");
        std::env::set_var("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED", "false");

        let result = service
            .execute_proposal(ExecuteProposalRequest {
                proposal_id: proposal_id.clone(),
                executor_id: "smoke_executor".to_string(),
                executor_permissions: vec!["todo:write".to_string()],
                executor_department_id: None,
            })
            .await;

        std::env::remove_var("FMS_AI_PROPOSAL_EXECUTION_ENABLED");
        std::env::remove_var("FMS_AI_LEGACY_TOOL_FALLBACK_ENABLED");

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

        let todo_count_after: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM todos WHERE source_type = 'ai_action' AND source_id = $1")
                .bind(&proposal_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let outbox_count_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'Todo.create' AND aggregate_id = $1",
        )
        .bind(&proposal_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            todo_count_after.0, todo_count_before.0,
            "readiness-not-ready smoke must not create todo rows"
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
}
