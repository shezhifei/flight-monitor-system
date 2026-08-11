use chrono::Utc;
use fms_domain::ports::kpi_port::KpiPort;
use fms_infrastructure::repositories::pg_kpi_repository::PgKpiRepository;
use sqlx::PgPool;

#[sqlx::test(migrations = "tests/migrations_kpi")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn fetch_equipment_utilization_rate_with_one_equipment_and_no_dispatch(pool: PgPool) {
    sqlx::query("INSERT INTO equipment (id) VALUES ($1)")
        .bind("EQ-1")
        .execute(&pool)
        .await
        .unwrap();

    let repo = PgKpiRepository::new(pool);
    let rate = repo.fetch_equipment_utilization_rate().await.unwrap();
    assert_eq!(rate, Some(0.0));
}

#[sqlx::test(migrations = "tests/migrations_kpi")]
#[ignore = "requires DATABASE_URL with PostgreSQL"]
async fn fetch_hourly_flight_volume_with_one_flight(pool: PgPool) {
    let today = Utc::now().date_naive();
    // Shanghai is UTC+8; 01:00 UTC corresponds to 09:00 Asia/Shanghai.
    let scheduled = today.and_hms_opt(1, 0, 0).unwrap().and_utc();
    sqlx::query("INSERT INTO flights (flight_id, scheduled_departure) VALUES ($1, $2)")
        .bind("FL-1")
        .bind(scheduled)
        .execute(&pool)
        .await
        .unwrap();

    let repo = PgKpiRepository::new(pool);
    let rows = repo.fetch_hourly_flight_volume(today, today).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].hour, 9);
    assert_eq!(rows[0].count, 1);
}
