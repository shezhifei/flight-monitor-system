use super::*;
use crate::services::dispatch_service::DispatchService;
use crate::types::ConcreteNotificationService;
use serde_json::json;
use std::sync::Arc;

fn has_pool() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

async fn create_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db")
}

async fn build_executor(pool: sqlx::PgPool) -> DomainActionExecutor {
    use crate::services::anomaly_service::AnomalyService;
    use crate::services::business_case_service::{BusinessCaseEventPublisher, BusinessCaseService};
    use crate::services::flight_service::FlightService;
    use crate::services::label_service::LabelService;
    use crate::services::notification_service::{
        NotificationDeliveryPublisher, NotificationMetricsRecorder, NotificationReceiptGroupSync, NotificationService,
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

    let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
    let flight_service = Arc::new(FlightService::new(flight_repo.clone()).with_transactional_repository(flight_repo));

    let dispatch_order_repo = Arc::new(PgDispatchOrderRepository::new(pool.clone()));
    let dispatch_service = Arc::new(DispatchService::new(dispatch_order_repo));

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
    let notification_service: Arc<ConcreteNotificationService> = Arc::new(
        NotificationService::new(notification_repo_port, notification_pref_repo_port)
            .with_collaboration_repo(notification_collaboration_repo_port)
            .with_metrics_recorder(Arc::new(NoopNotificationMetricsRecorder) as Arc<dyn NotificationMetricsRecorder>)
            .with_delivery_publisher(
                Arc::new(NoopNotificationDeliveryPublisher) as Arc<dyn NotificationDeliveryPublisher>
            )
            .with_receipt_group_sync(
                Arc::new(NoopNotificationReceiptGroupSync) as Arc<dyn NotificationReceiptGroupSync>
            ),
    );

    let anomaly_repo = Arc::new(PgAnomalyRepository::new(pool.clone()));
    let anomaly_service = Arc::new(AnomalyService::new(anomaly_repo));

    let label_repo = Arc::new(PgLabelRepository::new(pool.clone()));
    let label_service = Arc::new(LabelService::new(label_repo, Arc::new(NoopBroadcaster)));

    let todo_repo = Arc::new(PgTodoRepository::new(pool.clone()));
    let todo_service = Arc::new(TodoService::new(todo_repo));

    let business_case_repo: Arc<dyn fms_domain::ports::business_case_repository::BusinessCaseRepository + Send + Sync> =
        Arc::new(PgBusinessCaseRepository::new(pool.clone()));
    let business_case_collaboration_repo: Arc<
        dyn fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository + Send + Sync,
    > = collaboration_repo;
    let business_case_service = Arc::new(
        BusinessCaseService::new(business_case_repo)
            .with_event_publisher(Arc::new(NoopBusinessCaseEventPublisher) as Arc<dyn BusinessCaseEventPublisher>)
            .with_dispatch_chat_repository(business_case_collaboration_repo),
    );

    DomainActionExecutor::new(
        flight_service,
        dispatch_service,
        notification_service,
        anomaly_service,
        label_service,
        todo_service,
        business_case_service,
        pool,
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
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
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
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_notification_send_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    sqlx::query(
        "INSERT INTO users (id, username, display_name, email, password_hash, is_active, is_verified) \
         VALUES ($1, $2, $3, $4, $5, TRUE, TRUE) ON CONFLICT (id) DO NOTHING",
    )
    .bind("test_user_1")
    .bind("test_user_1")
    .bind("Test User 1")
    .bind("test_user_1@example.com")
    .bind("hashed_password")
    .execute(&pool)
    .await
    .expect("insert test user");

    sqlx::query(
        "INSERT INTO users (id, username, display_name, email, password_hash, is_active, is_verified) \
         VALUES ($1, $2, $3, $4, $5, TRUE, TRUE) ON CONFLICT (id) DO NOTHING",
    )
    .bind("tester_notif")
    .bind("tester_notif")
    .bind("Tester Notification")
    .bind("tester_notif@example.com")
    .bind("hashed_password")
    .execute(&pool)
    .await
    .expect("insert tester notif user");

    let executor = build_executor(pool).await;
    let args = json!({
        "user_id": "test_user_1",
        "title": "Alert",
        "body": "This is a test notification."
    });
    let res = executor
        .execute_approved_action("Notification", "NT123", "send", &args, "tester_notif")
        .await;
    assert!(res.is_ok());
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
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
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
    assert!(matches!(res, Err(DomainActionError::Execution(_))));
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
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn test_todo_create_success() {
    if !has_pool() {
        return;
    }
    let pool = create_pool().await;
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, is_active, is_verified) \
         VALUES ($1, $2, $3, $4, TRUE, TRUE) ON CONFLICT (id) DO NOTHING",
    )
    .bind("tester_todo")
    .bind("tester_todo")
    .bind("tester_todo@example.com")
    .bind("hashed_password")
    .execute(&pool)
    .await
    .expect("insert test user");

    let executor = build_executor(pool).await;
    let res = executor
        .execute_approved_action("Todo", "TD123", "create", &json!({"title": "Test Todo"}), "tester_todo")
        .await;
    assert!(res.is_ok());
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
    assert!(matches!(res, Err(DomainActionError::Execution(_))));
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
    assert!(matches!(res, Err(DomainActionError::Validation(_))));
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
    assert!(matches!(res, Err(DomainActionError::Execution(_))));
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
    assert!(
        matches!(res, Err(DomainActionError::Internal(ref e)) if e.contains("outbox write failed")),
        "expected Internal outbox error, got {:?}",
        res
    );

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
        matches!(res_anomaly, Err(DomainActionError::Internal(ref e)) if e.contains("outbox write failed")),
        "expected Internal outbox error for anomaly, got {:?}",
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
        matches!(res_publish, Err(DomainActionError::Internal(ref e)) if e.contains("outbox write failed")),
        "expected Internal outbox error for order publish, got {:?}",
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
        matches!(res_bc, Err(DomainActionError::Internal(ref e)) if e.contains("outbox write failed")),
        "expected Internal outbox error for business case, got {:?}",
        res_bc
    );

    let bc_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM flight_business_cases WHERE flight_id = 'FL_TX_ROLLBACK'")
            .fetch_one(&pool)
            .await
            .expect("count business cases");
    assert_eq!(bc_count, 0, "expected no business case created");

    // --- Test Notification.send Rollback ---
    let res_notification = executor
        .execute_approved_action(
            "Notification",
            "NT_TX_ROLLBACK",
            "send",
            &json!({
                "user_id": "USR_TX_ROLLBACK",
                "title": "rollback test notification",
                "body": "should not persist"
            }),
            "tester_tx_rollback",
        )
        .await;
    assert!(
        matches!(res_notification, Err(DomainActionError::Internal(ref e)) if e.contains("outbox write failed")),
        "expected Internal outbox error for notification, got {:?}",
        res_notification
    );

    let notification_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE related_entity_id = 'NT_TX_ROLLBACK'")
            .fetch_one(&pool)
            .await
            .expect("count notifications");
    assert_eq!(notification_count, 0, "expected no notification created after rollback");

    // --- Test Todo.create Rollback ---
    let res_todo = executor
        .execute_approved_action(
            "Todo",
            "TD_TX_ROLLBACK",
            "create",
            &json!({
                "title": "rollback test todo"
            }),
            "tester_tx_rollback",
        )
        .await;
    assert!(
        matches!(res_todo, Err(DomainActionError::Internal(ref e)) if e.contains("outbox write failed")),
        "expected Internal outbox error for todo, got {:?}",
        res_todo
    );

    let todo_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todos WHERE source_id = 'TD_TX_ROLLBACK'")
        .fetch_one(&pool)
        .await
        .expect("count todos");
    assert_eq!(todo_count, 0, "expected no todo created after rollback");

    guard.cleanup().await;
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
