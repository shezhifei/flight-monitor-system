//! SIMD JSON vs serde_json comparison on an AI-copilot-sized payload.
//!
//! Run: cargo bench -p fms-benches --bench simd_json_benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCopilotResponse {
    pub copilot_id: String,
    pub business_case_id: Option<String>,
    pub status: String,
    pub actions: Vec<ActionProposal>,
    pub metadata: ActionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionProposal {
    pub proposal_id: String,
    pub action_type: String,
    pub payload: serde_json::Value,
    pub risk_level: u8,
    pub confidence_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMetadata {
    pub created_at: String,
    pub version: String,
    pub model_version: Option<String>,
    pub tokens_used: u32,
}

fn generate_large_response() -> AiCopilotResponse {
    AiCopilotResponse {
        copilot_id: "test-copilot-123".to_string(),
        business_case_id: Some("business-case-456".to_string()),
        status: "committed".to_string(),
        actions: (0..32)
            .map(|i| ActionProposal {
                proposal_id: format!("action-{i}"),
                action_type: "flight_adjustment".to_string(),
                payload: serde_json::json!({
                    "flight_number": "CA1234",
                    "index": i,
                    "reason": "Weather delay",
                    "notes": "x".repeat(128)
                }),
                risk_level: 1,
                confidence_score: 0.95,
            })
            .collect(),
        metadata: ActionMetadata {
            created_at: "2026-08-22T10:00:00Z".to_string(),
            version: "v2.1".to_string(),
            model_version: Some("ai-copilot-v3.2".to_string()),
            tokens_used: 15000,
        },
    }
}

fn bench_serde_to_string(c: &mut Criterion) {
    let data = generate_large_response();
    c.bench_function("serde_json_to_string", |b| {
        b.iter(|| {
            let result = serde_json::to_string(black_box(&data)).unwrap();
            black_box(result);
        })
    });
}

fn bench_simd_to_string(c: &mut Criterion) {
    let data = generate_large_response();
    c.bench_function("simd_json_to_string", |b| {
        b.iter(|| {
            let result = simd_json::serde::to_string(black_box(&data)).unwrap();
            black_box(result);
        })
    });
}

fn bench_serde_from_string(c: &mut Criterion) {
    let data = generate_large_response();
    let json = serde_json::to_string(&data).unwrap();
    c.bench_function("serde_json_from_string", |b| {
        b.iter(|| {
            let result: AiCopilotResponse = serde_json::from_str(black_box(&json)).unwrap();
            black_box(result);
        })
    });
}

fn bench_simd_from_string(c: &mut Criterion) {
    let data = generate_large_response();
    let json = serde_json::to_string(&data).unwrap();
    c.bench_function("simd_json_from_string", |b| {
        b.iter(|| {
            let mut bytes = json.clone().into_bytes();
            let result: AiCopilotResponse = simd_json::serde::from_slice(black_box(&mut bytes)).unwrap();
            black_box(result);
        })
    });
}

criterion_group!(
    benches,
    bench_serde_to_string,
    bench_simd_to_string,
    bench_serde_from_string,
    bench_simd_from_string
);
criterion_main!(benches);
