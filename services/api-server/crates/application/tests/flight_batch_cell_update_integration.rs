//! PostgreSQL integration tests for `FlightBatchCellUpdateService`.
//!
//! Requires a migrated FMS test database with `flights`, `domain_event_outbox`,
//! and `flight_dispatch_timeline_events`.
//!
//! Run:
//! ```powershell
//! $env:TEST_DATABASE_URL = "postgres://USER:PASS@localhost:5432/flight_monitor_test"
//! cargo test -p fms-application --test flight_batch_cell_update_integration -- --ignored --nocapture
//! ```
//!
//! Prefer `scripts/dev/run_flight_batch_cell_db_tests.ps1` which loads DB_* from `.env`
//! without printing the password.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use fms_application::schemas::flight_schemas::{
    FlightBatchCellTarget, FlightBatchCellUpdateRequest, FlightBatchEditableField,
};
use fms_application::services::flight_batch_cell_update_service::{
    FlightBatchCellError, FlightBatchCellUpdateService, MANUAL_BATCH_EDIT_SOURCE,
};
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::flight_timeline_event_repository::FlightTimelineEventRepository;
use fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository;
use fms_infrastructure::repositories::pg_flight_runtime_projection_repository::PgFlightRuntimeProjectionRepository;
use fms_infrastructure::repositories::pg_flight_timeline_event_repository::PgFlightTimelineEventRepository;
use serde_json::{json, Value};
use sqlx::PgPool;

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn test_database_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL")
        .ok()
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .filter(|v| !v.trim().is_empty())
}

async fn connect_pool() -> Option<PgPool> {
    let url = test_database_url()?;
    match PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(err) => {
            eprintln!("skip: cannot connect to test database: {err}");
            None
        }
    }
}

fn build_service(pool: PgPool) -> FlightBatchCellUpdateService {
    let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
    let timeline_repo = Arc::new(PgFlightTimelineEventRepository::new(pool.clone()));
    let flight_repo_dyn: Arc<dyn FlightRepository + Send + Sync> = flight_repo.clone();
    let timeline_read = timeline_repo.clone();
    FlightBatchCellUpdateService::new(flight_repo_dyn, flight_repo, timeline_repo, timeline_read, pool)
}

async fn seed_flight(pool: &PgPool, flight_id: &str, stand: &str, remarks: &str, version: i32) {
    // Keep flight_number short — some environments use VARCHAR(7).
    let flight_number = format!("B{}", &flight_id[flight_id.len().saturating_sub(5)..]);
    sqlx::query(
        r#"
        INSERT INTO flights (
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
            $1, 'CZ', $2, NULL,
            'A320', 0,
            NOW(), NOW() + INTERVAL '2 hours',
            NULL, NULL,
            NULL, NULL,
            NULL, NULL,
            'G1', $3, 'T1', NULL, NULL,
            FALSE, FALSE, TRUE,
            NOW(), NOW(), $4,
            $5, NULL, NULL, NULL
        )
        ON CONFLICT (flight_id) DO UPDATE SET
            stand = EXCLUDED.stand,
            flight_remarks = EXCLUDED.flight_remarks,
            version = EXCLUDED.version,
            updated_at = NOW()
        "#,
    )
    .bind(flight_id)
    .bind(flight_number)
    .bind(stand)
    .bind(version)
    .bind(remarks)
    .execute(pool)
    .await
    .expect("seed flight");
}

fn short_id(tag: &str) -> String {
    // Prefer short ULID-like ids to satisfy VARCHAR(26) flight_id and keep numbers tiny.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Base36 compact suffix (≤10 chars) + 1-char tag.
    let mut n = (nanos % 3_656_158_440_062_976u128) as u64; // 36^10
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = [b'0'; 10];
    for i in (0..10).rev() {
        buf[i] = alphabet[(n % 36) as usize];
        n /= 36;
    }
    format!("b{}{}", std::str::from_utf8(&buf).unwrap_or("0"), tag)
}

async fn cleanup_flight(pool: &PgPool, flight_id: &str) {
    let _ = sqlx::query("DELETE FROM flight_dispatch_timeline_events WHERE flight_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM domain_event_outbox WHERE aggregate_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM flight_legs WHERE flight_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM flights WHERE flight_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await;
}

async fn read_flight_row(pool: &PgPool, flight_id: &str) -> (Option<String>, Option<String>, i32) {
    let row: (Option<String>, Option<String>, i32) =
        sqlx::query_as("SELECT stand, flight_remarks, version FROM flights WHERE flight_id = $1")
            .bind(flight_id)
            .fetch_one(pool)
            .await
            .expect("read flight");
    row
}

async fn outbox_count(pool: &PgPool, flight_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*)::bigint FROM domain_event_outbox WHERE aggregate_id = $1")
        .bind(flight_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

async fn timeline_count(pool: &PgPool, flight_id: &str, source: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM flight_dispatch_timeline_events WHERE flight_id = $1 AND source = $2",
    )
    .bind(flight_id)
    .bind(source)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL / DATABASE_URL with migrated FMS schema"]
async fn batch_snapshot_updates_all_or_nothing_and_writes_outbox() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    let f1 = short_id("a");
    let f2 = short_id("b");

    cleanup_flight(&pool, &f1).await;
    cleanup_flight(&pool, &f2).await;
    seed_flight(&pool, &f1, "A101", "old-1", 3).await;
    seed_flight(&pool, &f2, "A102", "old-2", 5).await;

    let service = build_service(pool.clone());
    let request = FlightBatchCellUpdateRequest {
        field: FlightBatchEditableField::FlightRemarks,
        value: json!("batch-note"),
        client_action_id: Some(format!("batch-{}", unique_suffix())),
        targets: vec![
            FlightBatchCellTarget {
                flight_id: f1.clone(),
                expected_version: Some(3),
                expected_value: json!("old-1"),
            },
            FlightBatchCellTarget {
                flight_id: f2.clone(),
                expected_version: Some(5),
                expected_value: json!("old-2"),
            },
        ],
    };

    let result = service
        .execute(request, "tester", false, &["flight:manage".into()])
        .await
        .expect("batch remarks update");

    assert_eq!(result.updated_count, 2);
    assert_eq!(result.field, "flight_remarks");
    assert_eq!(result.results.len(), 2);

    let (stand1, remarks1, version1) = read_flight_row(&pool, &f1).await;
    let (stand2, remarks2, version2) = read_flight_row(&pool, &f2).await;
    assert_eq!(stand1.as_deref(), Some("A101"));
    assert_eq!(stand2.as_deref(), Some("A102"));
    assert_eq!(remarks1.as_deref(), Some("batch-note"));
    assert_eq!(remarks2.as_deref(), Some("batch-note"));
    assert_eq!(version1, 4);
    assert_eq!(version2, 6);

    assert!(outbox_count(&pool, &f1).await >= 1);
    assert!(outbox_count(&pool, &f2).await >= 1);

    cleanup_flight(&pool, &f1).await;
    cleanup_flight(&pool, &f2).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL / DATABASE_URL with migrated FMS schema"]
async fn batch_version_conflict_writes_nothing() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    let f1 = short_id("c");
    let f2 = short_id("d");

    cleanup_flight(&pool, &f1).await;
    cleanup_flight(&pool, &f2).await;
    seed_flight(&pool, &f1, "B201", "keep-1", 7).await;
    seed_flight(&pool, &f2, "B202", "keep-2", 9).await;
    let outbox_before_1 = outbox_count(&pool, &f1).await;
    let outbox_before_2 = outbox_count(&pool, &f2).await;

    let service = build_service(pool.clone());
    let request = FlightBatchCellUpdateRequest {
        field: FlightBatchEditableField::Stand,
        value: json!("Z999"),
        client_action_id: Some(format!("conflict-{}", unique_suffix())),
        targets: vec![
            FlightBatchCellTarget {
                flight_id: f1.clone(),
                expected_version: Some(7),
                expected_value: json!("B201"),
            },
            // Stale version on second flight → whole batch must fail closed.
            FlightBatchCellTarget {
                flight_id: f2.clone(),
                expected_version: Some(1),
                expected_value: json!("B202"),
            },
        ],
    };

    let err = service
        .execute(request, "admin", true, &["*".into()])
        .await
        .expect_err("conflict expected");
    assert!(matches!(err, FlightBatchCellError::Conflict { .. }));

    let (stand1, remarks1, version1) = read_flight_row(&pool, &f1).await;
    let (stand2, remarks2, version2) = read_flight_row(&pool, &f2).await;
    assert_eq!(stand1.as_deref(), Some("B201"));
    assert_eq!(stand2.as_deref(), Some("B202"));
    assert_eq!(remarks1.as_deref(), Some("keep-1"));
    assert_eq!(remarks2.as_deref(), Some("keep-2"));
    assert_eq!(version1, 7);
    assert_eq!(version2, 9);
    assert_eq!(outbox_count(&pool, &f1).await, outbox_before_1);
    assert_eq!(outbox_count(&pool, &f2).await, outbox_before_2);

    cleanup_flight(&pool, &f1).await;
    cleanup_flight(&pool, &f2).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL / DATABASE_URL with migrated FMS schema"]
async fn batch_timeline_appends_manual_batch_edit_events_with_jwt_actor() {
    let Some(pool) = connect_pool().await else {
        return;
    };
    let f1 = short_id("e");
    let f2 = short_id("f");

    cleanup_flight(&pool, &f1).await;
    cleanup_flight(&pool, &f2).await;
    seed_flight(&pool, &f1, "C301", "n/a", 2).await;
    seed_flight(&pool, &f2, "C302", "n/a", 2).await;

    let service = build_service(pool.clone());
    let occurred = Utc::now();
    let batch_id = format!("tl{}", unique_suffix());
    let request = FlightBatchCellUpdateRequest {
        field: FlightBatchEditableField::StartBoardingTime,
        value: json!(occurred.to_rfc3339()),
        client_action_id: Some(batch_id.clone()),
        targets: vec![
            FlightBatchCellTarget {
                flight_id: f1.clone(),
                expected_version: None,
                expected_value: Value::Null,
            },
            FlightBatchCellTarget {
                flight_id: f2.clone(),
                expected_version: None,
                expected_value: Value::Null,
            },
        ],
    };

    let result = service
        .execute(request, "jwt-actor-1", false, &["flight:manage".into()])
        .await
        .expect("timeline batch");

    assert_eq!(result.updated_count, 2);
    assert!(result.results.iter().all(|r| r.timeline_id.is_some()));

    assert_eq!(timeline_count(&pool, &f1, MANUAL_BATCH_EDIT_SOURCE).await, 1);
    assert_eq!(timeline_count(&pool, &f2, MANUAL_BATCH_EDIT_SOURCE).await, 1);

    let recorded_by: Option<String> = sqlx::query_scalar(
        "SELECT recorded_by FROM flight_dispatch_timeline_events WHERE flight_id = $1 AND source = $2 LIMIT 1",
    )
    .bind(&f1)
    .bind(MANUAL_BATCH_EDIT_SOURCE)
    .fetch_one(&pool)
    .await
    .expect("recorded_by");
    assert_eq!(recorded_by.as_deref(), Some("jwt-actor-1"));

    let client_action_id: Option<String> = sqlx::query_scalar(
        "SELECT client_action_id FROM flight_dispatch_timeline_events WHERE flight_id = $1 AND source = $2 LIMIT 1",
    )
    .bind(&f1)
    .bind(MANUAL_BATCH_EDIT_SOURCE)
    .fetch_one(&pool)
    .await
    .expect("client_action_id");
    assert_eq!(
        client_action_id.as_deref(),
        Some(format!("{batch_id}:start_boarding_time:{f1}").as_str())
    );

    // Idempotent re-submit with same batch id + field should not create a second event.
    let request2 = FlightBatchCellUpdateRequest {
        field: FlightBatchEditableField::StartBoardingTime,
        value: json!(occurred.to_rfc3339()),
        client_action_id: Some(batch_id.clone()),
        targets: vec![FlightBatchCellTarget {
            flight_id: f1.clone(),
            expected_version: None,
            expected_value: json!(occurred),
        }],
    };
    let retry = service
        .execute(request2, "jwt-actor-1", false, &["flight:manage".into()])
        .await
        .expect("idempotent retry");
    assert_eq!(retry.updated_count, 1);
    assert_eq!(timeline_count(&pool, &f1, MANUAL_BATCH_EDIT_SOURCE).await, 1);

    // Same batch_id but different field must still succeed (field is part of idempotency key).
    let request3 = FlightBatchCellUpdateRequest {
        field: FlightBatchEditableField::EndBoardingTime,
        value: json!((occurred + chrono::Duration::minutes(10)).to_rfc3339()),
        client_action_id: Some(batch_id.clone()),
        targets: vec![FlightBatchCellTarget {
            flight_id: f1.clone(),
            expected_version: None,
            expected_value: Value::Null,
        }],
    };
    service
        .execute(request3, "jwt-actor-1", false, &["flight:manage".into()])
        .await
        .expect("different field with same batch id");
    assert_eq!(timeline_count(&pool, &f1, MANUAL_BATCH_EDIT_SOURCE).await, 2);

    // Overwrite existing start_boarding_time to an EARLIER time (last-write-wins).
    // This is the critical "correction" case: 10:00 → 09:55 must become current.
    let earlier = occurred - chrono::Duration::minutes(5);
    let request4 = FlightBatchCellUpdateRequest {
        field: FlightBatchEditableField::StartBoardingTime,
        value: json!(earlier.to_rfc3339()),
        client_action_id: Some(format!("{batch_id}-earlier")),
        targets: vec![FlightBatchCellTarget {
            flight_id: f1.clone(),
            expected_version: None,
            expected_value: json!(occurred),
        }],
    };
    let overwrite = service
        .execute(request4, "jwt-actor-1", false, &["flight:manage".into()])
        .await
        .expect("overwrite existing timeline value with earlier time");
    assert_eq!(overwrite.updated_count, 1);
    // Response must report the earlier value that was stored.
    assert_eq!(
        overwrite.results[0].value.as_str().map(|s| s[..19].to_string()),
        Some(earlier.to_rfc3339()[..19].to_string())
    );
    // Append-only model: another event is inserted for the new value.
    assert!(timeline_count(&pool, &f1, MANUAL_BATCH_EDIT_SOURCE).await >= 3);

    // latest_snapshots (last-write-wins) must surface the earlier correction.
    let snaps = PgFlightTimelineEventRepository::new(pool.clone())
        .latest_snapshots(&[f1.clone()])
        .await
        .expect("latest snapshots");
    let current = snaps.get(&f1).and_then(|m| m.get("start_boarding_time")).copied();
    assert!(
        current.is_some_and(|dt| dt.timestamp() == earlier.timestamp()),
        "latest snapshot should be earlier correction, got {current:?}"
    );

    // Runtime projection rebuild must surface the same last-write value. The
    // runner validates the canonical notification snapshot schema up front so
    // this assertion cannot silently degrade into a skip.
    let projection_repo = PgFlightRuntimeProjectionRepository::new(pool.clone());
    projection_repo
        .rebuild_for_flight(&f1)
        .await
        .expect("rebuild projection");
    let snap_json: Value =
        sqlx::query_scalar("SELECT timeline_snapshot FROM flight_runtime_list_projection WHERE flight_id = $1")
            .bind(&f1)
            .fetch_one(&pool)
            .await
            .expect("read projection");
    let Value::Object(map) = snap_json else {
        panic!("projection timeline_snapshot must be a JSON object");
    };
    let projected = map
        .get("start_boarding_time")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    assert!(
        projected.is_some_and(|dt| dt.timestamp() == earlier.timestamp()),
        "projection should show earlier correction, got {projected:?}"
    );

    cleanup_flight(&pool, &f1).await;
    cleanup_flight(&pool, &f2).await;
}
