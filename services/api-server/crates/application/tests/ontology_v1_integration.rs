//! PostgreSQL integration tests for Ontology V1 service.
//!
//! Requires a migrated FMS test database including `migrations/119_ontology_v1_core.sql`.
//!
//! Run:
//! ```powershell
//! $env:TEST_DATABASE_URL = "postgres://USER:PASS@localhost:5432/flight_monitor_test"
//! cargo test -p fms-application --test ontology_v1_integration -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Duration, Utc};
use fms_application::schemas::ontology_schemas::{
    AdjustGateRequest, AdjustStandRequest, AllocateGateRequest, AllocateStandRequest, AutoLinkScanRequest,
    BreakTurnaroundLinkRequest, ConfirmDraftFlightsRequest, CreateSuggestionRequest, CreateTurnaroundLinkRequest,
    ReassignAircraftChange, ReassignAircraftRequest, ReleaseResourceRequest, SuggestionAcceptRequest,
    SuggestionRejectRequest,
};
use fms_application::services::ontology_service::{OntologyService, OntologyTransactions, OntologyWriter};
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_repository::{FlightRepository, FlightTransactionalRepository};
use fms_domain::ports::ontology_repository::{
    AircraftRepository, CarouselAssignmentRepository, GateAssignmentRepository, OntologyTransactionalRepository,
    ResourceAdjustmentSuggestionRepository, StandOccupationRepository, TurnaroundLinkRepository,
};
use fms_infrastructure::db::transaction::PgUnitOfWork;
use fms_infrastructure::repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;
use fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository;
use fms_infrastructure::repositories::pg_ontology_repository::{
    PgAircraftRepository, PgCarouselAssignmentRepository, PgGateAssignmentRepository,
    PgResourceAdjustmentSuggestionRepository, PgStandOccupationRepository, PgTurnaroundLinkRepository,
};
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

async fn connect_pool() -> PgPool {
    let url = test_database_url().expect(
        "TEST_DATABASE_URL (or DATABASE_URL) must be set; refusing to silently skip ontology integration tests",
    );
    PgPool::connect(&url)
        .await
        .unwrap_or_else(|err| panic!("cannot connect to ontology test database: {err}"))
}

async fn ontology_tables_ready(pool: &PgPool) -> bool {
    let ok: Result<(bool,), sqlx::Error> = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'aircraft'
         )",
    )
    .fetch_one(pool)
    .await;
    matches!(ok, Ok((true,)))
}

fn build_service(pool: PgPool) -> OntologyService {
    let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
    let aircraft = Arc::new(PgAircraftRepository::new(pool.clone()));
    let occupations = Arc::new(PgStandOccupationRepository::new(pool.clone()));
    let assignments = Arc::new(PgGateAssignmentRepository::new(pool.clone()));
    let links = Arc::new(PgTurnaroundLinkRepository::new(pool.clone()));
    let suggestions = Arc::new(PgResourceAdjustmentSuggestionRepository::new(pool.clone()));
    let carousels = Arc::new(PgCarouselAssignmentRepository::new(pool.clone()));

    let flight_port: Arc<dyn FlightRepository + Send + Sync> = flight_repo.clone();
    let aircraft_port: Arc<dyn AircraftRepository + Send + Sync> = aircraft.clone();
    let occupation_port: Arc<dyn StandOccupationRepository + Send + Sync> = occupations;
    let assignment_port: Arc<dyn GateAssignmentRepository + Send + Sync> = assignments;
    let link_port: Arc<dyn TurnaroundLinkRepository + Send + Sync> = links.clone();
    let suggestion_port: Arc<dyn ResourceAdjustmentSuggestionRepository + Send + Sync> = suggestions;
    let carousel_port: Arc<dyn CarouselAssignmentRepository + Send + Sync> = carousels.clone();
    let ontology_tx: Arc<
        dyn OntologyTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync,
    > = aircraft.clone();
    let flight_tx: Arc<dyn FlightTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync> =
        flight_repo.clone();
    let outbox_tx: Arc<
        dyn DomainEventOutboxTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync,
    > = Arc::new(PgDomainEventOutboxRepository::new(pool.clone()));

    let writer = Arc::new(OntologyWriter::new(
        flight_port.clone(),
        link_port.clone(),
        ontology_tx,
        flight_tx,
        outbox_tx,
        Arc::new(PgUnitOfWork::new(pool.clone())),
    ));

    OntologyService::new(
        flight_port,
        aircraft_port,
        occupation_port,
        assignment_port,
        link_port,
        suggestion_port,
        carousel_port,
        writer as Arc<dyn OntologyTransactions>,
    )
}

async fn seed_flight(pool: &PgPool, flight_id: &str, registration: Option<&str>, with_outbound: bool, is_draft: bool) {
    let flight_number = format!("O{}", &flight_id[flight_id.len().saturating_sub(5)..]);

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
            aircraft_maintenance_remarks, aircraft_check_remarks,
            is_draft, flight_kind, divert
        ) VALUES (
            $1, 'CZ', $2, $3,
            'A320', 0,
            NOW() + INTERVAL '3 hours', NOW() + INTERVAL '1 hours',
            NULL, NULL,
            NULL, CASE WHEN $3 IS NOT NULL THEN NOW() - INTERVAL '30 minutes' ELSE NULL END,
            NULL, NULL,
            NULL, NULL, 'T1', NULL, NULL,
            FALSE, FALSE, TRUE,
            NOW(), NOW(), 1,
            NULL, NULL, NULL, NULL,
            $4, 'passenger', FALSE
        )
        ON CONFLICT (flight_id) DO UPDATE SET
            registration = EXCLUDED.registration,
            is_draft = EXCLUDED.is_draft,
            actual_arrival = EXCLUDED.actual_arrival,
            version = flights.version + 1,
            updated_at = NOW()
        "#,
    )
    .bind(flight_id)
    .bind(&flight_number)
    .bind(registration)
    .bind(is_draft)
    .execute(pool)
    .await
    .expect("seed flight");

    // Legs live in flight_legs (not JSON columns on flights).
    let _ = sqlx::query("DELETE FROM flight_legs WHERE flight_id = $1")
        .bind(flight_id)
        .execute(pool)
        .await;

    // flight_legs uses origin_stations / destination_stations JSONB arrays (not origin_code).
    sqlx::query(
        r#"
        INSERT INTO flight_legs (
            leg_id, flight_id, leg_type, flight_no, flight_type,
            origin_stations, destination_stations, is_vip, scheduled_time, created_at, updated_at
        ) VALUES (
            $1, $2, 'inbound', $3, 'domestic',
            '[{"code":"PEK"}]'::jsonb, '[{"code":"SZX"}]'::jsonb,
            FALSE, NOW() + INTERVAL '1 hours', NOW(), NOW()
        )
        ON CONFLICT (flight_id, leg_type) DO UPDATE SET flight_no = EXCLUDED.flight_no
        "#,
    )
    .bind(format!("{flight_id}I"))
    .bind(flight_id)
    .bind(&flight_number)
    .execute(pool)
    .await
    .expect("seed inbound leg");

    if with_outbound {
        sqlx::query(
            r#"
            INSERT INTO flight_legs (
                leg_id, flight_id, leg_type, flight_no, flight_type,
                origin_stations, destination_stations, is_vip, scheduled_time, created_at, updated_at
            ) VALUES (
                $1, $2, 'outbound', $3, 'domestic',
                '[{"code":"SZX"}]'::jsonb, '[{"code":"PEK"}]'::jsonb,
                FALSE, NOW() + INTERVAL '3 hours', NOW(), NOW()
            )
            ON CONFLICT (flight_id, leg_type) DO UPDATE SET flight_no = EXCLUDED.flight_no
            "#,
        )
        .bind(format!("{flight_id}O"))
        .bind(flight_id)
        .bind(&flight_number)
        .execute(pool)
        .await
        .expect("seed outbound leg");
    }
}

async fn cleanup(pool: &PgPool, flight_ids: &[&str], regs: &[&str]) {
    for id in flight_ids {
        let _ = sqlx::query("DELETE FROM resource_adjustment_suggestions WHERE flight_id = $1")
            .bind(id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM turnaround_links WHERE inbound_flight_id = $1 OR outbound_flight_id = $1")
            .bind(id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM stand_occupations WHERE flight_id = $1")
            .bind(id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM gate_assignments WHERE flight_id = $1")
            .bind(id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM flight_legs WHERE flight_id = $1")
            .bind(id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM flights WHERE flight_id = $1")
            .bind(id)
            .execute(pool)
            .await;
    }
    for reg in regs {
        let _ = sqlx::query("DELETE FROM stand_occupations WHERE registration = $1")
            .bind(reg)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM gate_assignments WHERE registration = $1")
            .bind(reg)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM aircraft WHERE registration = $1")
            .bind(reg)
            .execute(pool)
            .await;
    }
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with migration 119 applied"]
async fn reassign_aircraft_updates_registration() {
    let pool = connect_pool().await;
    assert!(
        ontology_tables_ready(&pool).await,
        "ontology tables missing; apply migration 119"
    );

    let suffix = unique_suffix();
    let flight_id = format!("OTF{suffix}");
    let old_reg = format!("B-O{suffix}");
    let new_reg = format!("B-N{suffix}");
    cleanup(&pool, &[&flight_id], &[&old_reg, &new_reg]).await;
    seed_flight(&pool, &flight_id, Some(&old_reg), true, false).await;

    let svc = build_service(pool.clone());
    let result = svc
        .reassign_aircraft(
            ReassignAircraftRequest {
                changes: vec![ReassignAircraftChange {
                    flight_id: flight_id.clone(),
                    new_registration: new_reg.clone(),
                }],
                correlation_id: None,
            },
            "tester",
            &["ontology.aircraft.reassign".into()],
            false,
        )
        .await
        .expect("reassign");

    assert_eq!(result.applied.len(), 1);
    assert_eq!(result.applied[0].new_registration, new_reg);

    let row: (Option<String>,) = sqlx::query_as("SELECT registration FROM flights WHERE flight_id = $1")
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .expect("read flight");
    assert_eq!(row.0.as_deref(), Some(new_reg.as_str()));

    cleanup(&pool, &[&flight_id], &[&old_reg, &new_reg]).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with migration 119 applied"]
async fn allocate_stand_and_accept_suggestion() {
    let pool = connect_pool().await;
    assert!(
        ontology_tables_ready(&pool).await,
        "ontology tables missing; apply migration 119"
    );

    let suffix = unique_suffix();
    let flight_id = format!("OTS{suffix}");
    let reg = format!("B-S{suffix}");
    cleanup(&pool, &[&flight_id], &[&reg]).await;
    seed_flight(&pool, &flight_id, Some(&reg), true, false).await;

    let svc = build_service(pool.clone());
    let now = Utc::now();
    let stand = svc
        .allocate_stand(
            AllocateStandRequest {
                registration: reg.clone(),
                stand_code: "201".into(),
                starts_at: now,
                ends_at: now + Duration::hours(2),
                kind: "normal".into(),
                moving_to_stand: None,
                flight_id: Some(flight_id.clone()),
                sync_flight_plan: true,
            },
            "aoc_user",
            &["ontology.stand.manage".into()],
            false,
        )
        .await
        .expect("allocate stand");
    assert!(stand.occupation.get("id").is_some());

    let suggestion = svc
        .create_suggestion(
            CreateSuggestionRequest {
                flight_id: flight_id.clone(),
                kind: "stand".into(),
                suggested_value: "202".into(),
                current_value: Some("201".into()),
                reason: Some("test".into()),
                payload: Some(serde_json::json!({
                    "starts_at": (now + Duration::hours(3)).to_rfc3339(),
                    "ends_at": (now + Duration::hours(5)).to_rfc3339()
                })),
                expires_at: Some(now + Duration::hours(1)),
                created_by: Some("aoc_user".into()),
            },
            "aoc_user",
            &["ontology.stand.manage".into()],
            false,
        )
        .await
        .expect("create suggestion");

    let accepted = svc
        .accept_suggestion(
            &suggestion.id,
            SuggestionAcceptRequest {
                accepted_by: "aoc_user".into(),
                actor_permissions: vec!["ontology.suggestion.accept_stand".into()],
            },
        )
        .await
        .expect("accept suggestion");
    assert_eq!(accepted.status.as_str_status(), "accepted_executed");

    let stand_code: (Option<String>,) = sqlx::query_as("SELECT stand FROM flights WHERE flight_id = $1")
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .expect("read stand");
    assert_eq!(stand_code.0.as_deref(), Some("202"));

    let occ_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM stand_occupations WHERE flight_id = $1 AND stand_code = '202' AND status = 'active'",
    )
    .bind(&flight_id)
    .fetch_one(&pool)
    .await
    .expect("count occupations");
    assert!(occ_count.0 >= 1);

    cleanup(&pool, &[&flight_id], &[&reg]).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with migration 119 applied"]
async fn auto_link_scan_creates_link_for_same_registration() {
    let pool = connect_pool().await;
    assert!(
        ontology_tables_ready(&pool).await,
        "ontology tables missing; apply migration 119"
    );

    let suffix = unique_suffix();
    let inbound_id = format!("OTI{suffix}");
    let outbound_id = format!("OTO{suffix}");
    let reg = format!("B-L{suffix}");
    cleanup(&pool, &[&inbound_id, &outbound_id], &[&reg]).await;

    // inbound: arrived, same reg
    seed_flight(&pool, &inbound_id, Some(&reg), false, false).await;
    sqlx::query("UPDATE flights SET status = 2, actual_arrival = NOW() - INTERVAL '20 minutes' WHERE flight_id = $1")
        .bind(&inbound_id)
        .execute(&pool)
        .await
        .expect("set inbound arrived");

    // outbound: scheduled with departure soon
    seed_flight(&pool, &outbound_id, Some(&reg), true, false).await;
    sqlx::query(
        "UPDATE flights SET scheduled_departure = NOW() + INTERVAL '40 minutes', status = 0 WHERE flight_id = $1",
    )
    .bind(&outbound_id)
    .execute(&pool)
    .await
    .expect("set outbound sched");

    let svc = build_service(pool.clone());
    let scan = svc
        .auto_link_scan(AutoLinkScanRequest {
            window_minutes: Some(360),
            limit: Some(50),
        })
        .await
        .expect("scan");

    assert!(scan.evaluated > 0, "scan should evaluate candidates: {scan:?}");

    let link_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM turnaround_links WHERE inbound_flight_id = $1 AND outbound_flight_id = $2 AND status = 'active'",
    )
    .bind(&inbound_id)
    .bind(&outbound_id)
    .fetch_one(&pool)
    .await
    .expect("count links");

    assert_eq!(link_count.0, 1, "auto_link_scan must create the expected active link");

    cleanup(&pool, &[&inbound_id, &outbound_id], &[&reg]).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with migration 119 applied"]
async fn stand_and_gate_adjust_and_release() {
    let pool = connect_pool().await;
    assert!(
        ontology_tables_ready(&pool).await,
        "ontology tables missing; apply migration 119"
    );

    let suffix = unique_suffix();
    let flight_id = format!("OTA{suffix}");
    let reg = format!("B-A{suffix}");
    cleanup(&pool, &[&flight_id], &[&reg]).await;
    seed_flight(&pool, &flight_id, Some(&reg), true, false).await;

    let svc = build_service(pool.clone());
    let now = Utc::now();
    let stand_perms = ["ontology.stand.manage".into()];
    let gate_perms = ["ontology.gate.manage".into()];

    let stand = svc
        .allocate_stand(
            AllocateStandRequest {
                registration: reg.clone(),
                stand_code: "301".into(),
                starts_at: now,
                ends_at: now + Duration::hours(2),
                kind: "normal".into(),
                moving_to_stand: None,
                flight_id: Some(flight_id.clone()),
                sync_flight_plan: true,
            },
            "aoc",
            &stand_perms,
            false,
        )
        .await
        .expect("allocate stand");
    let occ_id = stand
        .occupation
        .get("id")
        .and_then(|v| v.as_str())
        .expect("occupation id")
        .to_string();

    let adjusted = svc
        .adjust_stand(
            &occ_id,
            AdjustStandRequest {
                stand_code: Some("302".into()),
                starts_at: None,
                ends_at: None,
                kind: None,
                moving_to_stand: None,
                sync_flight_plan: true,
            },
            "aoc",
            &stand_perms,
            false,
        )
        .await
        .expect("adjust stand");
    assert_eq!(
        adjusted
            .occupation
            .get("stand_code")
            .and_then(|v| v.as_str())
            .or_else(|| {
                adjusted
                    .occupation
                    .get("stand_code")
                    .and_then(|v| v.get("0"))
                    .and_then(|v| v.as_str())
            }),
        Some("302")
    );

    let stand_code: (Option<String>,) = sqlx::query_as("SELECT stand FROM flights WHERE flight_id = $1")
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .expect("read stand");
    assert_eq!(stand_code.0.as_deref(), Some("302"));

    let released = svc
        .release_stand(
            &occ_id,
            ReleaseResourceRequest {
                released_by: Some("aoc".into()),
            },
            "aoc",
            &stand_perms,
            false,
        )
        .await
        .expect("release stand");
    assert_eq!(
        format!("{:?}", released.status).to_lowercase().contains("released"),
        true
    );

    let gate = svc
        .allocate_gate(
            AllocateGateRequest {
                registration: reg.clone(),
                gate_code: "G1".into(),
                starts_at: now,
                ends_at: now + Duration::hours(2),
                flight_id: Some(flight_id.clone()),
                sync_flight_plan: true,
            },
            "toc",
            &gate_perms,
            false,
        )
        .await
        .expect("allocate gate");
    let asn_id = gate
        .assignment
        .get("id")
        .and_then(|v| v.as_str())
        .expect("assignment id")
        .to_string();

    svc.adjust_gate(
        &asn_id,
        AdjustGateRequest {
            gate_code: Some("G2".into()),
            starts_at: None,
            ends_at: None,
            sync_flight_plan: true,
        },
        "toc",
        &gate_perms,
        false,
    )
    .await
    .expect("adjust gate");

    let gate_code: (Option<String>,) = sqlx::query_as("SELECT gate FROM flights WHERE flight_id = $1")
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .expect("read gate");
    assert_eq!(gate_code.0.as_deref(), Some("G2"));

    let released_gate = svc
        .release_gate(
            &asn_id,
            ReleaseResourceRequest {
                released_by: Some("toc".into()),
            },
            "toc",
            &gate_perms,
            false,
        )
        .await
        .expect("release gate");
    assert!(format!("{:?}", released_gate.status)
        .to_lowercase()
        .contains("released"));

    cleanup(&pool, &[&flight_id], &[&reg]).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with migration 119 applied"]
async fn confirm_draft_and_reject_suggestion() {
    let pool = connect_pool().await;
    assert!(
        ontology_tables_ready(&pool).await,
        "ontology tables missing; apply migration 119"
    );

    let suffix = unique_suffix();
    let flight_id = format!("OTD{suffix}");
    let reg = format!("B-D{suffix}");
    cleanup(&pool, &[&flight_id], &[&reg]).await;
    seed_flight(&pool, &flight_id, Some(&reg), true, true).await;

    let svc = build_service(pool.clone());
    let confirmed = svc
        .confirm_draft_flights(ConfirmDraftFlightsRequest {
            flight_ids: vec![flight_id.clone()],
            confirmed_by: "aoc".into(),
        })
        .await
        .expect("confirm drafts");
    assert!(confirmed.confirmed.iter().any(|id| id == &flight_id));

    let is_draft: (bool,) = sqlx::query_as("SELECT is_draft FROM flights WHERE flight_id = $1")
        .bind(&flight_id)
        .fetch_one(&pool)
        .await
        .expect("read is_draft");
    assert!(!is_draft.0);

    let suggestion = svc
        .create_suggestion(
            CreateSuggestionRequest {
                flight_id: flight_id.clone(),
                kind: "gate".into(),
                suggested_value: "B9".into(),
                current_value: None,
                reason: Some("reject me".into()),
                payload: None,
                expires_at: Some(Utc::now() + Duration::hours(1)),
                created_by: Some("toc".into()),
            },
            "toc",
            &["ontology.gate.manage".into()],
            false,
        )
        .await
        .expect("create suggestion");

    let rejected = svc
        .reject_suggestion(
            &suggestion.id,
            SuggestionRejectRequest {
                rejected_by: "toc".into(),
                reason: Some("not needed".into()),
            },
        )
        .await
        .expect("reject suggestion");
    assert_eq!(rejected.status.as_str_status(), "rejected");

    cleanup(&pool, &[&flight_id], &[&reg]).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL with migration 119 applied"]
async fn manual_turnaround_link_create_and_break() {
    let pool = connect_pool().await;
    assert!(
        ontology_tables_ready(&pool).await,
        "ontology tables missing; apply migration 119"
    );

    let suffix = unique_suffix();
    // keep ids within varchar(26)
    let inbound_id = format!("MI{suffix}");
    let outbound_id = format!("MO{suffix}");
    let reg = format!("B-M{suffix}");
    cleanup(&pool, &[&inbound_id, &outbound_id], &[&reg]).await;
    seed_flight(&pool, &inbound_id, Some(&reg), false, false).await;
    seed_flight(&pool, &outbound_id, Some(&reg), true, false).await;

    let svc = build_service(pool.clone());
    let link = svc
        .create_turnaround_link(
            CreateTurnaroundLinkRequest {
                inbound_flight_id: inbound_id.clone(),
                outbound_flight_id: outbound_id.clone(),
                source: "manual".into(),
                created_by: Some("aoc".into()),
            },
            "aoc",
            &["ontology.plan.confirm".into()],
            false,
        )
        .await
        .expect("create link");
    assert!(!link.id.is_empty());

    let broken = svc
        .break_turnaround_link(
            &link.id,
            BreakTurnaroundLinkRequest {
                reason: Some("test break".into()),
                broken_by: Some("aoc".into()),
            },
            "aoc",
            &["ontology.plan.confirm".into()],
            false,
        )
        .await
        .expect("break link");
    assert!(format!("{:?}", broken.status).to_lowercase().contains("broken"));

    cleanup(&pool, &[&inbound_id, &outbound_id], &[&reg]).await;
}

// helper for status string in test — SuggestionStatus may not have as_str_status
trait SuggestionStatusStr {
    fn as_str_status(&self) -> &'static str;
}

impl SuggestionStatusStr for fms_domain::models::ontology_v1::SuggestionStatus {
    fn as_str_status(&self) -> &'static str {
        match self {
            fms_domain::models::ontology_v1::SuggestionStatus::Pending => "pending",
            fms_domain::models::ontology_v1::SuggestionStatus::AcceptedExecuted => "accepted_executed",
            fms_domain::models::ontology_v1::SuggestionStatus::Rejected => "rejected",
            fms_domain::models::ontology_v1::SuggestionStatus::Expired => "expired",
        }
    }
}
