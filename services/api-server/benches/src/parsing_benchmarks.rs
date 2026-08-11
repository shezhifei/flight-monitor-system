//! Criterion benchmarks for (a) a protobuf encode/decode path and
//! (b) a JSON (serde) serialize/deserialize path.
//!
//! Both benchmarks run entirely in-memory with no external services. The
//! protobuf path uses `prost` with a representative flight record struct; the
//! JSON path round-trips a real domain model (`fms_domain::models::anomaly::Anomaly`).

use chrono::{DateTime, Utc};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prost::Message;

/// Representative flight record mirroring the `ProtoFlight` shape produced by
/// `crates/api/src/routes/flights/proto.rs`. Used to exercise the prost
/// encode/decode (protobuf) path.
#[derive(Clone, PartialEq, Message)]
struct BenchFlight {
    #[prost(string, tag = "1")]
    flight_id: String,
    #[prost(string, tag = "2")]
    flight_number: String,
    #[prost(string, tag = "15")]
    status: String,
    #[prost(string, tag = "60")]
    created_by: String,
    #[prost(bool, tag = "28")]
    has_boarding_restriction: bool,
    #[prost(int32, tag = "37")]
    version: i32,
}

fn sample_flight() -> BenchFlight {
    BenchFlight {
        flight_id: "flight-2002".to_string(),
        flight_number: "CA1234".to_string(),
        status: "scheduled".to_string(),
        created_by: "dispatcher-1".to_string(),
        has_boarding_restriction: false,
        version: 3,
    }
}

fn bench_protobuf_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("protobuf");

    let flight = sample_flight();

    group.bench_function("encode_flight", |b| {
        b.iter(|| {
            let bytes = black_box(&flight).encode_to_vec();
            black_box(bytes)
        })
    });

    let encoded = flight.encode_to_vec();

    group.bench_function("decode_flight", |b| {
        b.iter(|| {
            let decoded = BenchFlight::decode(black_box(&encoded[..])).expect("decode");
            black_box(decoded)
        })
    });

    group.finish();
}

fn sample_anomaly() -> fms_domain::models::anomaly::Anomaly {
    use fms_domain::models::anomaly::{Anomaly, AnomalySeverity, AnomalyStatus, AnomalyType};
    use std::collections::HashMap;

    let now: DateTime<Utc> = Utc::now();
    Anomaly {
        anomaly_id: "anomaly-1001".to_string(),
        flight_id: "flight-2002".to_string(),
        anomaly_type: AnomalyType::GateStandConflict,
        severity: AnomalySeverity::High,
        title: "Gate/stand conflict detected".to_string(),
        description: Some("Stand S12 assigned to two flights".to_string()),
        status: AnomalyStatus::Open,
        detected_at: now,
        resolved_at: None,
        escalation_level: 0,
        last_escalated_at: None,
        linked_todo_id: None,
        rule_id: Some("rule-7".to_string()),
        context_data: {
            let mut map = HashMap::new();
            map.insert("gate".to_string(), serde_json::json!("A1"));
            map.insert("stand".to_string(), serde_json::json!("S12"));
            map
        },
        created_at: now,
        updated_at: now,
    }
}

fn bench_json_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_serde");

    let anomaly = sample_anomaly();

    group.bench_function("serialize_anomaly", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&anomaly)).expect("serialize");
            black_box(json)
        })
    });

    let json = serde_json::to_string(&anomaly).expect("serialize");

    group.bench_function("deserialize_anomaly", |b| {
        b.iter(|| {
            let value =
                serde_json::from_str::<fms_domain::models::anomaly::Anomaly>(black_box(&json)).expect("deserialize");
            black_box(value)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_protobuf_roundtrip, bench_json_roundtrip);
criterion_main!(benches);
