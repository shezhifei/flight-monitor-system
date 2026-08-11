//! Criterion benchmarks for key PostgreSQL repository query paths.
//!
//! These benches are designed to compile and run WITHOUT a live database.
//! The query construction path (`sqlx::query(...).bind(...)`) is pure and is
//! always benchmarked. The actual DB-touching path (`.fetch_optional`) is only
//! executed when `DATABASE_URL` is present in the environment, so running the
//! suite without a database does not fail.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_anomaly_repository_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("anomaly_repository");

    // Pure query-construction benchmark (always runs, no DB required). This
    // mirrors the `find_by_id` query in
    // `crates/infrastructure/src/repositories/pg_anomaly_repository.rs`.
    group.bench_function("find_by_id_build_query", |b| {
        b.iter(|| {
            let q = sqlx::query("SELECT * FROM anomalies WHERE anomaly_id = $1").bind(black_box("anomaly-1001"));
            black_box(q)
        })
    });

    // Pure query-construction benchmark for the ordered list lookup
    // (`find_by_flight`).
    group.bench_function("find_by_flight_build_query", |b| {
        b.iter(|| {
            let q = sqlx::query("SELECT * FROM anomalies WHERE flight_id = $1 ORDER BY detected_at DESC")
                .bind(black_box("flight-2002"));
            black_box(q)
        })
    });

    // Real DB path — only executed when DATABASE_URL points at a reachable PG.
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let pool = rt
            .block_on(sqlx::PgPool::connect(&database_url))
            .expect("connect to DATABASE_URL");

        group.bench_function("find_by_id_fetch_optional", |b| {
            b.iter(|| {
                rt.block_on(
                    sqlx::query("SELECT anomaly_id FROM anomalies WHERE anomaly_id = $1")
                        .bind("anomaly-1001")
                        .fetch_optional(&pool),
                )
            })
        });

        group.bench_function("find_by_flight_fetch_all", |b| {
            b.iter(|| {
                rt.block_on(
                    sqlx::query("SELECT anomaly_id FROM anomalies WHERE flight_id = $1 ORDER BY detected_at DESC")
                        .bind("flight-2002")
                        .fetch_all(&pool),
                )
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_anomaly_repository_queries);
criterion_main!(benches);
