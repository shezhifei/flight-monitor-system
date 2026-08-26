use super::*;
use crate::services::business_case_service::{BusinessCaseMentionAudience, CollaborationMentionAudience};
use crate::services::dispatch_service::DispatchService;
use crate::types::ConcreteNotificationService;
use serde_json::json;
use std::sync::Arc;

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

// outbox 写入失败可能由 executor 层直接返回 Internal，也可能被内部 service 先包装为 Execution，
// 两者都携带 "outbox write failed" 标记，均视为符合预期的失败。
fn is_outbox_failure(res: &Result<DomainActionReceipt, DomainActionError>) -> bool {
    matches!(
        res,
        Err(DomainActionError::Internal(e) | DomainActionError::Execution(e))
            if e.contains("outbox write failed")
    )
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
}

/// 测试用航班插入（满足 anomalies/dispatch_orders 的 flight_id 外键）。
async fn insert_test_flight(pool: &sqlx::PgPool, flight_id: &str) {
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
    .expect("insert test flight");
}

async fn build_executor(pool: sqlx::PgPool) -> DomainActionExecutor<fms_infrastructure::db::transaction::PgUnitOfWork> {
    use crate::services::business_case_service::{BusinessCaseEventPublisher, BusinessCaseService, BusinessCaseWriter};
    use crate::services::dispatch_service::writer::DispatchOrderWriter;
    use crate::services::flight_service::FlightService;
    use crate::services::flight_writer::FlightWriter;
    use crate::services::label_service::LabelService;
    use crate::services::notification_service::{
        CollaborationEventRecorder, NotificationCollaborationEvents, NotificationDeliveryPublisher,
        NotificationMetricsRecorder, NotificationReceiptGroupSync, NotificationService,
    };
    use crate::services::todo_service::TodoWriter;
    use crate::types::{
        NoopBroadcaster, NoopBusinessCaseEventPublisher, NoopNotificationDeliveryPublisher,
        NoopNotificationMetricsRecorder, NoopNotificationReceiptGroupSync,
    };
    use fms_infrastructure::repositories::{
        pg_anomaly_repository::PgAnomalyRepository, pg_business_case_repository::PgBusinessCaseRepository,
        pg_dispatch_collaboration_repository::PgDispatchCollaborationRepository,
        pg_dispatch_order_repository::PgDispatchOrderRepository,
        pg_domain_event_outbox_repository::PgDomainEventOutboxRepository, pg_flight_repository::PgFlightRepository,
        pg_label_repository::PgLabelRepository, pg_notification_repository::PgNotificationRepository,
        pg_todo_repository::PgTodoRepository,
    };

    let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
    let outbox_repo = Arc::new(PgDomainEventOutboxRepository::new(pool.clone()));
    let flight_service = Arc::new(FlightService::new(flight_repo.clone()));
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

    let dispatch_order_repo = Arc::new(PgDispatchOrderRepository::new(pool.clone()));
    // 本测试只接 order_repo 与其事务变体；其余端口是桩（与接线前的 None 行为一致）。
    let mut dispatch_deps = crate::test_support::stub_dispatch_dependencies();
    dispatch_deps.order.order_repo = dispatch_order_repo.clone();
    let dispatch_service = Arc::new(DispatchService::new(dispatch_deps));
    let dispatch_writer: Arc<DispatchOrderWriter<sqlx::Transaction<'static, sqlx::Postgres>>> =
        Arc::new(DispatchOrderWriter::new(
            dispatch_order_repo.clone()
                as Arc<dyn fms_domain::ports::dispatch_repository::DispatchOrderRepository + Send + Sync>,
            dispatch_order_repo.clone()
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
            dispatch_service.clone(),
        ));

    let notification_repo = Arc::new(PgNotificationRepository::new(pool.clone()));
    let collaboration_repo = Arc::new(PgDispatchCollaborationRepository::new(pool.clone()));
    let notification_repo_port: Arc<
        dyn fms_domain::ports::notification_repository::NotificationRepository + Send + Sync,
    > = notification_repo.clone();
    let notification_pref_repo_port: Arc<
        dyn fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync,
    > = notification_repo.clone();
    let notification_collaboration_repo_port: Arc<
        dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
    > = collaboration_repo.clone();
    let notification_tx_repo_port: Arc<
        dyn fms_domain::ports::notification_repository::NotificationTransactionalRepository<
                sqlx::Transaction<'static, sqlx::Postgres>,
            > + Send
            + Sync,
    > = notification_repo.clone();
    let notification_service: Arc<ConcreteNotificationService> = Arc::new(NotificationService::new(
        notification_repo_port,
        notification_pref_repo_port,
        Arc::new(CollaborationEventRecorder::new(notification_collaboration_repo_port))
            as Arc<dyn NotificationCollaborationEvents>,
        Arc::new(NoopNotificationDeliveryPublisher) as Arc<dyn NotificationDeliveryPublisher>,
        Arc::new(NoopNotificationMetricsRecorder) as Arc<dyn NotificationMetricsRecorder>,
        Arc::new(NoopNotificationReceiptGroupSync) as Arc<dyn NotificationReceiptGroupSync>,
    ));

    let anomaly_repo = Arc::new(PgAnomalyRepository::new(pool.clone()));

    let label_repo = Arc::new(PgLabelRepository::new(pool.clone()));
    let label_service = Arc::new(LabelService::new(label_repo, Arc::new(NoopBroadcaster)));

    let todo_repo = Arc::new(PgTodoRepository::new(pool.clone()));
    let todo_writer: Arc<TodoWriter<sqlx::Transaction<'static, sqlx::Postgres>>> =
        Arc::new(TodoWriter::new(todo_repo.clone(), todo_repo));

    let business_case_pg_repo = Arc::new(PgBusinessCaseRepository::new(pool.clone()));
    let business_case_repo: Arc<dyn fms_domain::ports::business_case_repository::BusinessCaseRepository + Send + Sync> =
        business_case_pg_repo.clone();
    let business_case_writer: Arc<BusinessCaseWriter<sqlx::Transaction<'static, sqlx::Postgres>>> = Arc::new(
        BusinessCaseWriter::new(business_case_pg_repo.clone(), business_case_pg_repo),
    );
    let business_case_collaboration_repo: Arc<
        dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
    > = collaboration_repo;
    let business_case_service = Arc::new(BusinessCaseService::new(
        business_case_repo,
        Arc::new(NoopBusinessCaseEventPublisher) as Arc<dyn BusinessCaseEventPublisher>,
        Arc::new(CollaborationMentionAudience::new(business_case_collaboration_repo))
            as Arc<dyn BusinessCaseMentionAudience>,
    ));

    DomainActionExecutor::new(
        flight_service,
        flight_writer,
        dispatch_service,
        dispatch_writer,
        business_case_service,
        business_case_writer,
        outbox_repo,
        anomaly_repo,
        Arc::new(fms_infrastructure::db::transaction::PgUnitOfWork::new(pool)),
    )
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_add_note_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Flight", "FL123", "add_note", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_add_note_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "Flight",
            "FL_NONEXISTENT",
            "add_note",
            &json!({"note": "test note"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_update_status_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Flight", "FL123", "update_status", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_update_status_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "Flight",
            "FL_NONEXISTENT",
            "update_status",
            &json!({"status": "delayed"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_change_stand_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Flight", "FL123", "change_stand", &json!({}), "tester")
        .await;
    // change_stand 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_change_stand_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "Flight",
            "FL_NONEXISTENT",
            "change_stand",
            &json!({"new_stand_id": "ST101"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_notification_send_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Notification", "NT123", "send", &json!({}), "tester")
        .await;
    // Notification.send 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_notification_send_fail_closed() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let args = json!({
        "user_id": "test_user_1",
        "title": "Alert",
        "body": "This is a test notification."
    });
    let res = executor
        .execute_approved_action("Notification", "NT123", "send", &args, "tester_notif")
        .await;
    // Notification.send 已废止 -> 执行器 fail-closed（unknown action），不写入
    assert!(matches!(res, Err(DomainActionError::NotFound(_))), "got {:?}", res);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_anomaly_acknowledge_not_found() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    let executor = build_executor(pool).await;
    let res = executor
        .execute_approved_action("Anomaly", "AN_NONEXISTENT", "acknowledge", &json!({}), "tester")
        .await;
    assert!(
        res.is_ok(),
        "acknowledge of non-existent anomaly should succeed (no-op), got {:?}",
        res
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_anomaly_escalate_not_found() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    let executor = build_executor(pool).await;
    let res = executor
        .execute_approved_action("Anomaly", "AN_NONEXISTENT", "escalate", &json!({}), "tester")
        .await;
    assert!(
        res.is_ok(),
        "escalate of non-existent anomaly should succeed (no-op), got {:?}",
        res
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_recommend_replan_not_found() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    let executor = build_executor(pool).await;
    let res = executor
        .execute_approved_action(
            "DispatchOrder",
            "DP_NONEXISTENT",
            "recommend_replan",
            &json!({}),
            "tester",
        )
        .await;
    assert!(
        matches!(res, Err(DomainActionError::NotFound(_))),
        "expected NotFound for missing order, got {:?}",
        res
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_reassign_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("DispatchOrder", "DP123", "reassign", &json!({}), "tester")
        .await;
    // reassign 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_reassign_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "DispatchOrder",
            "DP_NONEXISTENT",
            "reassign",
            &json!({"assignee_id": "user_1"}),
            "tester",
        )
        .await;
    // reassign 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_publish_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("DispatchOrder", "DP_NONEXISTENT", "publish", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Execution(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_todo_create_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Todo", "TD123", "create", &json!({}), "tester")
        .await;
    // Todo.create 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_todo_create_fail_closed() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Todo", "TD123", "create", &json!({"title": "Test Todo"}), "tester_todo")
        .await;
    // Todo.create 已废止 -> 执行器 fail-closed（unknown action），不写入
    assert!(matches!(res, Err(DomainActionError::NotFound(_))), "got {:?}", res);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_todo_complete_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Todo", "TD_NONEXISTENT", "complete", &json!({}), "tester")
        .await;
    // Todo.complete 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_stand_reserve_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Stand", "ST101", "reserve", &json!({}), "tester")
        .await;
    // Stand.reserve 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_stand_reserve_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Stand", "ST101", "reserve", &json!({"flight_id": "FL9999"}), "tester")
        .await;
    // Stand.reserve 已废止 -> 执行器 fail-closed（unknown action）
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_business_case_create_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("BusinessCase", "BC123", "create", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_business_case_create_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let args = json!({
        "flight_id": "FL_NONEXISTENT",
        "case_type": "delay",
        "description": "test case"
    });
    let res = executor
        .execute_approved_action("BusinessCase", "BC123", "create", &args, "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Execution(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_business_case_close_case_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("BusinessCase", "BC_NONEXISTENT", "close_case", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Execution(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_outbox_write_on_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
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
        ) ON CONFLICT (flight_id) DO UPDATE SET updated_at = NOW()"#,
    )
    .bind("FL_OUTBOX_OK")
    .bind("CA1234")
    .execute(&pool)
    .await
    .expect("insert test flight");

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE aggregate_id = 'FL_OUTBOX_OK'")
            .fetch_one(&pool)
            .await
            .expect("count before");

    let executor = build_executor(pool.clone()).await;
    let args = json!({"status": "delayed"});
    let res = executor
        .execute_approved_action("Flight", "FL_OUTBOX_OK", "update_status", &args, "tester")
        .await;
    assert!(res.is_ok(), "expected Ok, got {:?}", res);

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE aggregate_id = 'FL_OUTBOX_OK'")
        .fetch_one(&pool)
        .await
        .expect("count after");
    // DomainActionExecutor writes action receipt + FlightService writes flight.*_v2
    assert!(
        after >= before + 2,
        "expected action + flight domain outbox events, before={before} after={after}"
    );

    let types: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM domain_event_outbox WHERE aggregate_id = 'FL_OUTBOX_OK' ORDER BY occurred_at DESC",
    )
    .fetch_all(&pool)
    .await
    .expect("fetch outbox event types");
    assert!(
        types.iter().any(|t| t == "Flight.update_status"),
        "missing action outbox event, types={types:?}"
    );
    assert!(
        types.iter().any(|t| t == "flight.status_updated_v2"),
        "missing flight domain outbox event, types={types:?}"
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_no_outbox_write_on_failure() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE aggregate_id = 'FL_OUTBOX_FAIL'")
            .fetch_one(&pool)
            .await
            .expect("count before");

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "Flight",
            "FL_OUTBOX_FAIL",
            "update_status",
            &json!({"status": "delayed"}),
            "tester",
        )
        .await;
    assert!(res.is_err(), "expected error for missing flight, got {:?}", res);

    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE aggregate_id = 'FL_OUTBOX_FAIL'")
            .fetch_one(&pool)
            .await
            .expect("count after");
    assert_eq!(after, before, "expected no outbox event for failed action");
}

struct OutboxFailGuard {
    pool: sqlx::PgPool,
}

impl OutboxFailGuard {
    async fn setup(pool: sqlx::PgPool) -> Self {
        let _ = sqlx::query("DROP TRIGGER IF EXISTS trg_test_outbox_fail ON domain_event_outbox")
            .execute(&pool)
            .await;
        let _ = sqlx::query("DROP FUNCTION IF EXISTS fn_test_outbox_fail()")
            .execute(&pool)
            .await;

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION fn_test_outbox_fail()
            RETURNS TRIGGER AS $$
            BEGIN
                IF NEW.aggregate_id IN ('FL_TX_ROLLBACK', 'NT_TX_ROLLBACK', 'TD_TX_ROLLBACK') THEN
                    RAISE EXCEPTION 'synthetic outbox failure for testing';
                END IF;
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql"#,
        )
        .execute(&pool)
        .await
        .expect("create trigger function");

        sqlx::query(
            r#"CREATE TRIGGER trg_test_outbox_fail
            BEFORE INSERT ON domain_event_outbox
            FOR EACH ROW EXECUTE FUNCTION fn_test_outbox_fail()"#,
        )
        .execute(&pool)
        .await
        .expect("create trigger");

        Self { pool }
    }

    async fn cleanup(&self) {
        let _ = sqlx::query("DROP TRIGGER IF EXISTS trg_test_outbox_fail ON domain_event_outbox")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("DROP FUNCTION IF EXISTS fn_test_outbox_fail()")
            .execute(&self.pool)
            .await;
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_outbox_failure_rollback() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    let guard = OutboxFailGuard::setup(pool.clone()).await;

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
    .bind("FL_TX_ROLLBACK")
    .bind("CA9999")
    .execute(&pool)
    .await
    .expect("insert test flight");

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, is_active) VALUES ($1, 'tx_rollback_user', 'tx@example.com', 'hashed_pwd', TRUE) ON CONFLICT (id) DO NOTHING",
    )
    .bind("USR_TX_ROLLBACK")
    .execute(&pool)
    .await
    .expect("insert test user");

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, is_active) VALUES ($1, 'tester_tx_rollback', 'tester_tx_rollback@example.com', 'hashed_pwd', TRUE) ON CONFLICT (id) DO NOTHING",
    )
    .bind("tester_tx_rollback")
    .execute(&pool)
    .await
    .expect("insert tester user");

    let executor = build_executor(pool.clone()).await;
    let args = json!({"status": "delayed"});
    let res = executor
        .execute_approved_action("Flight", "FL_TX_ROLLBACK", "update_status", &args, "tester_tx_rollback")
        .await;
    assert!(is_outbox_failure(&res), "expected outbox failure, got {:?}", res);

    let outbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE aggregate_id = 'FL_TX_ROLLBACK'")
            .fetch_one(&pool)
            .await
            .expect("count outbox");
    assert_eq!(outbox_count, 0, "expected no outbox event after rollback");

    let status: String = sqlx::query_scalar("SELECT status::TEXT FROM flights WHERE flight_id = 'FL_TX_ROLLBACK'")
        .fetch_one(&pool)
        .await
        .expect("fetch flight status");
    assert_eq!(
        status, "0",
        "expected flight status unchanged (rolled back), got {}",
        status
    );

    // --- Test Anomaly.acknowledge Rollback ---
    sqlx::query(
        "INSERT INTO anomalies (anomaly_id, flight_id, anomaly_type, status, severity, title, description, detected_at) VALUES ($1, $1, 'unknown', 'open', 'medium', 'Test', 'Test anomaly', NOW()) ON CONFLICT (anomaly_id) DO NOTHING"
    )
    .bind("FL_TX_ROLLBACK")
    .execute(&pool)
    .await
    .expect("insert test anomaly");

    let res_anomaly = executor
        .execute_approved_action(
            "Anomaly",
            "FL_TX_ROLLBACK",
            "acknowledge",
            &json!({}),
            "tester_tx_rollback",
        )
        .await;
    assert!(
        is_outbox_failure(&res_anomaly),
        "expected outbox failure for anomaly, got {:?}",
        res_anomaly
    );

    let anomaly_status: String = sqlx::query_scalar("SELECT status FROM anomalies WHERE anomaly_id = 'FL_TX_ROLLBACK'")
        .fetch_one(&pool)
        .await
        .expect("fetch anomaly status");
    assert_eq!(anomaly_status, "open", "expected anomaly status unchanged");

    // --- Test DispatchOrder.publish Rollback ---
    sqlx::query(
        "INSERT INTO dispatch_orders (id, flight_id, task_type, publication_state, status, assignee_type, source_type) VALUES ($1, $1, 'T', 'prepublished', 'pending', 'team', 'generated') ON CONFLICT DO NOTHING"
    )
    .bind("FL_TX_ROLLBACK")
    .execute(&pool)
    .await
    .expect("insert test order");

    let res_publish = executor
        .execute_approved_action(
            "DispatchOrder",
            "FL_TX_ROLLBACK",
            "publish",
            &json!({}),
            "tester_tx_rollback",
        )
        .await;
    assert!(
        is_outbox_failure(&res_publish),
        "expected outbox failure for order publish, got {:?}",
        res_publish
    );

    let pub_state: String =
        sqlx::query_scalar("SELECT publication_state FROM dispatch_orders WHERE id = 'FL_TX_ROLLBACK'")
            .fetch_one(&pool)
            .await
            .expect("fetch order pub state");
    assert_eq!(pub_state, "prepublished", "expected order publication_state unchanged");

    let chat_group_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM dispatch_chat_groups WHERE flight_id = 'FL_TX_ROLLBACK'")
            .fetch_one(&pool)
            .await
            .expect("count chat groups");
    assert_eq!(chat_group_count, 0, "expected no chat group created after rollback");

    // --- Test BusinessCase.create Rollback ---
    let res_bc = executor
        .execute_approved_action(
            "BusinessCase",
            "FL_TX_ROLLBACK",
            "create",
            &json!({
                "flight_id": "FL_TX_ROLLBACK",
                "case_type": "delay",
                "description": "test case"
            }),
            "tester_tx_rollback",
        )
        .await;
    assert!(
        is_outbox_failure(&res_bc),
        "expected outbox failure for business case, got {:?}",
        res_bc
    );

    let bc_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM flight_business_cases WHERE flight_id = 'FL_TX_ROLLBACK'")
            .fetch_one(&pool)
            .await
            .expect("count business cases");
    assert_eq!(bc_count, 0, "expected no business case created");

    // --- 注：Notification.send / Todo.create 已随合同退出（见 PR #本体两层改造），
    // 执行器对其 fail-closed，不再走事务/outbox 路径，故不再作为 rollback 用例。

    guard.cleanup().await;
}

// `Flight.update_delay` 至少一个时间且格式 RFC3339。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_update_delay_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Flight", "FL123", "update_delay", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));

    let res = executor
        .execute_approved_action(
            "Flight",
            "FL123",
            "update_delay",
            &json!({"estimated_departure": "not-a-datetime"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_flight_update_delay_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "Flight",
            "FL_NONEXISTENT",
            "update_delay",
            &json!({"estimated_departure": "2026-08-12T10:00:00Z"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

// `DispatchOrder.update_status` 枚举校验与 NotFound 映射。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_update_status_validation() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("DispatchOrder", "DP123", "update_status", &json!({}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));

    let res = executor
        .execute_approved_action(
            "DispatchOrder",
            "DP123",
            "update_status",
            &json!({"new_status": "bogus"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_update_status_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "DispatchOrder",
            "DP_NONEXISTENT",
            "update_status",
            &json!({"new_status": "completed"}),
            "tester",
        )
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_dispatch_order_update_status_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    sqlx::query(
        "INSERT INTO users (id, username, display_name, email, password_hash, is_active, is_verified) \
         VALUES ($1, $2, $3, $4, $5, TRUE, TRUE) ON CONFLICT DO NOTHING",
    )
    .bind("domain_action_status_usr")
    .bind("domain_action_status_usr")
    .bind("Domain Action Status Tester")
    .bind("domain_action_status_tester@example.com")
    .bind("hashed_password")
    .execute(&pool)
    .await
    .expect("insert tester user");
    sqlx::query("INSERT INTO teams (id, name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
        .bind("TEAM_STATUS_OK")
        .bind("Status Test Team")
        .execute(&pool)
        .await
        .expect("insert test team");
    sqlx::query(
        "INSERT INTO dispatch_orders (id, flight_id, task_type, publication_state, status, assignee_type, team_id, source_type) \
         VALUES ($1, $1, 'T', 'prepublished', 'pending', 'team', $2, 'generated') \
         ON CONFLICT (id) DO UPDATE SET team_id = EXCLUDED.team_id, status = EXCLUDED.status, updated_at = NOW()",
    )
    .bind("DP_STATUS_OK")
    .bind("TEAM_STATUS_OK")
    .execute(&pool)
    .await
    .expect("insert test order");

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "DispatchOrder",
            "DP_STATUS_OK",
            "update_status",
            &json!({"new_status": "in_progress", "notes": "started"}),
            "domain_action_status_usr",
        )
        .await;
    assert!(res.is_ok(), "expected Ok, got {:?}", res);

    let status: String = sqlx::query_scalar("SELECT status FROM dispatch_orders WHERE id = 'DP_STATUS_OK'")
        .fetch_one(&pool)
        .await
        .expect("fetch order status");
    assert_eq!(status, "in_progress");
}

// `Anomaly.resolve` 对不存在异常报 Execution。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_anomaly_resolve_not_found() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Anomaly", "AN_NONEXISTENT", "resolve", &json!({}), "tester")
        .await;
    assert!(
        matches!(res, Err(DomainActionError::Execution(ref msg)) if msg.contains("not found or already resolved")),
        "expected Execution not-found error, got {:?}",
        res
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_anomaly_resolve_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    insert_test_flight(&pool, "AN_RESOLVE_OK").await;
    sqlx::query(
        "INSERT INTO anomalies (anomaly_id, flight_id, anomaly_type, status, severity, title, description, detected_at) \
         VALUES ($1, $1, 'unknown', 'open', 'medium', 'Test', 'Test anomaly', NOW()) \
         ON CONFLICT (anomaly_id) DO UPDATE SET status = EXCLUDED.status",
    )
    .bind("AN_RESOLVE_OK")
    .execute(&pool)
    .await
    .expect("insert test anomaly");

    let executor = build_executor(pool.clone()).await;
    let res = executor
        .execute_approved_action(
            "Anomaly",
            "AN_RESOLVE_OK",
            "resolve",
            &json!({"resolution_note": "handled"}),
            "tester",
        )
        .await;
    assert!(res.is_ok(), "expected Ok, got {:?}", res);

    let status: String = sqlx::query_scalar("SELECT status FROM anomalies WHERE anomaly_id = 'AN_RESOLVE_OK'")
        .fetch_one(&pool)
        .await
        .expect("fetch anomaly status");
    assert_eq!(status, "resolved");
}

// `Label.add` 对象已随合同退出 -> fail-closed（unknown action）。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_label_add_fail_closed() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action("Label", "LB1", "add", &json!({"flight_id": "FL123"}), "tester")
        .await;
    assert!(matches!(res, Err(DomainActionError::NotFound(_))));
}

// `Workflow.start` 对象已随合同退出 -> fail-closed（unknown action）。
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_workflow_start_fail_closed() {
    if !has_pool() {
        return;
    }
    let executor = build_executor(create_pool().await).await;
    let res = executor
        .execute_approved_action(
            "Workflow",
            "WF1",
            "start",
            &json!({"workflow_template_id": "delay-process"}),
            "tester",
        )
        .await;
    assert!(
        matches!(res, Err(DomainActionError::NotFound(_))),
        "expected fail-closed NotFound, got {:?}",
        res
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_domain_action_executor_schema() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;

    let check_tables = vec![
        "dispatch_orders",
        "anomalies",
        "flight_business_cases",
        "todos",
        "users",
    ];

    for table in check_tables {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT FROM pg_tables WHERE schemaname = 'public' AND tablename = $1)")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap_or(false);

        assert!(exists, "Table {} must exist in database", table);
    }
}
