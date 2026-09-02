//! 航班风险摘要微模型实现
//!
//! 基于 `flight_risk_service.rs` 的确定性评分逻辑，
//! 扩展为带证据追踪、置信度评估和动作建议生成的完整微模型。

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use std::time::Instant;
use ulid::Ulid;

use fms_domain::models::ai_proposal::RiskLevel;
use fms_domain::models::micro_model::{MicroModelExecutionResult, MicroModelExecutionStatus};

use crate::schemas::flight_schemas::{FlightResponse, FlightRiskReason};
use crate::schemas::micro_model_schemas::{
    ConfidenceLevel, EvidenceType, FlightRiskEvidence, FlightRiskInput, FlightRiskOutput, FlightRiskProposal,
    MicroModelConfidence,
};

const STALE_AFTER_MINUTES: i64 = 30;
const MODEL_ID: &str = "flight_risk_v1";
const MODEL_VERSION: &str = "1.0.0";

pub struct FlightRiskMicroModel;

impl FlightRiskMicroModel {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(&self, input: &FlightRiskInput) -> FlightRiskOutput {
        let start = Instant::now();
        let flight_id = input.flight_id.clone();
        let now = Utc::now();

        let mut output = FlightRiskOutput::new(&flight_id);
        output.model_id = MODEL_ID.to_string();
        output.model_version = MODEL_VERSION.to_string();
        output.input_snapshot = serde_json::to_value(input).unwrap_or(Value::Null);

        let evidence: Vec<FlightRiskEvidence> = Vec::new();
        let _risk_score: i32 = 0;
        let mut confidence_reasons = Vec::new();

        if let Some(ceiling) = input.risk_ceiling_level() {
            confidence_reasons.push(format!("risk ceiling set to {:?}", ceiling));
        }

        let data_completeness = estimate_data_completeness(&input, &now);
        if data_completeness < 0.5 {
            confidence_reasons.push("flight data may be incomplete".to_string());
        }

        let data_freshness_score = estimate_data_freshness(&input, &now);
        if data_freshness_score < 0.7 {
            confidence_reasons.push("flight data may be stale".to_string());
        }

        if !input.include_weather {
            confidence_reasons.push("weather context not included".to_string());
        }
        if !input.include_manual_context {
            confidence_reasons.push("manual context not included".to_string());
        }

        let confidence_score = calculate_confidence(
            data_completeness,
            data_freshness_score,
            evidence.len(),
            confidence_reasons.len(),
        );

        output.confidence = MicroModelConfidence {
            score: confidence_score,
            level: ConfidenceLevel::from_score(confidence_score),
            reasons: confidence_reasons,
        };

        if input.include_manual_context {
            output.add_limitation("manual context is provided by users and may not be verified");
        }

        output.execution_time_ms = start.elapsed().as_millis() as u64;
        output
    }

    pub fn execute_from_flight_data(&self, input: &FlightRiskInput, flight: &FlightResponse) -> FlightRiskOutput {
        let start = Instant::now();
        let flight_id = input.flight_id.clone();
        let now = Utc::now();

        let mut output = FlightRiskOutput::new(&flight_id);
        output.model_id = MODEL_ID.to_string();
        output.model_version = MODEL_VERSION.to_string();
        output.input_snapshot = serde_json::to_value(input).unwrap_or(Value::Null);

        let mut evidence = Vec::new();
        let mut risk_score = 0;
        let mut confidence_reasons = Vec::new();

        if let Some(ceiling) = input.risk_ceiling_level() {
            confidence_reasons.push(format!("risk ceiling set to {:?}", ceiling));
            output.add_limitation(format!("risk score may be capped at {:?} ceiling", ceiling));
        }

        let has_anomaly = flight.anomaly_summary.has_open_anomaly || flight.anomaly_summary.open_count > 0;
        if has_anomaly {
            let severity = if flight.anomaly_summary.open_count >= 2 {
                "critical"
            } else {
                "high"
            };
            let weight = if severity == "critical" { 50.0 } else { 40.0 };
            risk_score += weight as i32;
            evidence.push(
                FlightRiskEvidence::new(
                    "open_anomaly",
                    format!("{} open anomaly item(s)", flight.anomaly_summary.open_count.max(1)),
                    severity,
                    EvidenceType::ObjectReference,
                )
                .with_object(flight_id.clone())
                .with_weight(weight)
                .with_raw_value(serde_json::json!({
                    "open_count": flight.anomaly_summary.open_count,
                    "acknowledged_count": flight.anomaly_summary.acknowledged_count,
                })),
            );
        }

        let has_delay = has_departure_delay(flight);
        if has_delay {
            risk_score += 25;
            evidence.push(
                FlightRiskEvidence::new(
                    "departure_delay",
                    "Estimated or current status indicates departure delay",
                    "high",
                    EvidenceType::DataView,
                )
                .with_data_view("flight_timeline")
                .with_weight(25.0)
                .with_raw_value(serde_json::json!({
                    "scheduled_departure": flight.scheduled_departure,
                    "estimated_departure": flight.estimated_departure,
                    "status": flight.status,
                })),
            );
        }

        let is_vip = is_vip_flight(flight);
        if is_vip {
            risk_score += 15;
            evidence.push(
                FlightRiskEvidence::new(
                    "vip_flight",
                    "VIP flight requires elevated operational attention",
                    "medium",
                    EvidenceType::ObjectReference,
                )
                .with_object(flight_id.clone())
                .with_weight(15.0),
            );
        }

        let has_business_case = has_open_business_case(flight);
        if has_business_case {
            risk_score += 10;
            evidence.push(
                FlightRiskEvidence::new(
                    "open_business_case",
                    "Flight has unfinished business case work",
                    "medium",
                    EvidenceType::ObjectReference,
                )
                .with_object(flight_id.clone())
                .with_weight(10.0)
                .with_raw_value(serde_json::json!({
                    "business_case_count": flight.business_cases.len(),
                })),
            );
        }

        let freshness = calculate_data_freshness(flight, now);
        if freshness.stale {
            risk_score += 10;
            evidence.push(
                FlightRiskEvidence::new(
                    "stale_data",
                    "Flight data is stale or missing an update timestamp",
                    "medium",
                    EvidenceType::DataView,
                )
                .with_data_view("flight_latest")
                .with_weight(10.0)
                .with_raw_value(serde_json::json!({
                    "updated_at": freshness.updated_at,
                    "stale": freshness.stale,
                })),
            );
        }

        let risk_ceiling = input.risk_ceiling_level();
        let capped_score = apply_risk_ceiling(risk_score, risk_ceiling);

        let risk_level = match capped_score {
            0..=24 => "low",
            25..=49 => "medium",
            50..=79 => "high",
            _ => "critical",
        }
        .to_string();

        output.risk_score = capped_score;
        output.risk_level = risk_level;
        output.evidence = evidence.clone();

        let mut confidence_reasons_internal = Vec::new();

        let data_completeness = estimate_data_completeness(input, &now);
        if data_completeness < 0.5 {
            confidence_reasons_internal.push("flight data may be incomplete".to_string());
        }

        let data_freshness_score = freshness_score_value(&freshness);
        if data_freshness_score < 0.7 {
            confidence_reasons_internal.push("flight data may be stale based on update timestamp".to_string());
        }

        if !input.include_weather {
            confidence_reasons_internal.push("weather context not included".to_string());
        }
        if !input.include_manual_context {
            confidence_reasons_internal.push("manual context not included in this analysis".to_string());
        }

        let confidence_score = calculate_confidence(
            data_completeness,
            data_freshness_score,
            evidence.len(),
            confidence_reasons_internal.len(),
        );

        output.confidence = MicroModelConfidence {
            score: confidence_score,
            level: ConfidenceLevel::from_score(confidence_score),
            reasons: confidence_reasons_internal,
        };

        let proposals = self.generate_proposals(flight, &output, &input);
        output.proposals = proposals;

        if !input.include_weather {
            output.add_limitation("weather context not included — delay risk may be underestimated");
        }
        if !input.include_manual_context {
            output.add_limitation("manual context not included — operational notes not considered");
        }
        output
            .add_limitation("model evaluates deterministic signals only — external factors may influence actual risk");

        output.execution_time_ms = start.elapsed().as_millis() as u64;
        output
    }

    fn generate_proposals(
        &self,
        flight: &FlightResponse,
        output: &FlightRiskOutput,
        input: &FlightRiskInput,
    ) -> Vec<FlightRiskProposal> {
        let mut proposals = Vec::new();
        let flight_id = &input.flight_id;

        if output.has_critical_evidence() && output.risk_level == "critical" {
            if let Some(stand) = &flight.stand {
                proposals.push(
                    FlightRiskProposal::new(
                        "review_stand_assignment",
                        "Flight",
                        "suggest_stand_adjustment",
                        format!(
                            "Critical risk flight {} at stand {} requires stand capacity review",
                            flight.flight_number.as_deref().unwrap_or(&flight_id),
                            stand
                        ),
                    )
                    .with_object_id(flight_id.clone())
                    .with_priority(1)
                    .with_risk_if_not_acted("potential stand congestion or operational delay"),
                );
            }

            proposals.push(
                FlightRiskProposal::new(
                    "escalate_supervisor",
                    "Anomaly",
                    "escalate",
                    format!(
                        "Escalate critical risk flight {} to supervisor attention",
                        flight.flight_number.as_deref().unwrap_or(&flight_id)
                    ),
                )
                .with_object_id(flight_id.clone())
                .with_priority(2)
                .with_risk_if_not_acted("supervisor may not be aware of critical risk"),
            );
        }

        let anomaly_count = flight.anomaly_summary.open_count;
        if anomaly_count > 0 {
            proposals.push(
                FlightRiskProposal::new(
                    "review_anomalies",
                    "Anomaly",
                    "acknowledge",
                    format!(
                        "Flight {} has {} open anomaly(s) requiring review",
                        flight.flight_number.as_deref().unwrap_or(&flight_id),
                        anomaly_count
                    ),
                )
                .with_object_id(flight_id.clone())
                .with_priority(if output.risk_level == "critical" { 1 } else { 3 }),
            );
        }

        if has_departure_delay(flight) {
            proposals.push(
                FlightRiskProposal::new(
                    "notify_delay",
                    "Flight",
                    "add_note",
                    format!(
                        "Inform stakeholders about departure delay for flight {}",
                        flight.flight_number.as_deref().unwrap_or(&flight_id)
                    ),
                )
                .with_object_id(flight_id.clone())
                .with_priority(4),
            );
        }

        proposals.sort_by_key(|p| p.priority);
        proposals.truncate(5);
        proposals
    }

    pub fn to_execution_result(
        &self,
        result: FlightRiskOutput,
        job_id: &str,
        run_id: &str,
    ) -> MicroModelExecutionResult {
        MicroModelExecutionResult {
            model_id: MODEL_ID.to_string(),
            model_version: MODEL_VERSION.to_string(),
            execution_id: format!("exec_{}", Ulid::new()),
            job_id: job_id.to_string(),
            run_id: run_id.to_string(),
            input: result.input_snapshot.clone(),
            output: serde_json::to_value(&result).unwrap_or(Value::Null),
            execution_time_ms: result.execution_time_ms,
            status: MicroModelExecutionStatus::Success,
            error_message: None,
            created_at: Utc::now(),
        }
    }
}

impl Default for FlightRiskMicroModel {
    fn default() -> Self {
        Self::new()
    }
}

fn has_departure_delay(flight: &FlightResponse) -> bool {
    let delayed_status = flight
        .status
        .as_deref()
        .map(|status| {
            let normalized = status.trim().to_ascii_lowercase();
            normalized.contains("delay") || normalized.contains("delayed")
        })
        .unwrap_or(false);
    if delayed_status {
        return true;
    }

    match (flight.scheduled_departure, flight.estimated_departure) {
        (Some(scheduled), Some(estimated)) => estimated.signed_duration_since(scheduled) >= Duration::minutes(15),
        _ => false,
    }
}

fn is_vip_flight(flight: &FlightResponse) -> bool {
    match flight.direction.as_deref() {
        Some("inbound") => flight.inbound_leg.as_ref().map(|leg| leg.is_vip).unwrap_or(false),
        Some("outbound") => flight.outbound_leg.as_ref().map(|leg| leg.is_vip).unwrap_or(false),
        _ => flight.inbound_leg.as_ref().map(|leg| leg.is_vip).unwrap_or(false)
            || flight.outbound_leg.as_ref().map(|leg| leg.is_vip).unwrap_or(false),
    }
}

fn has_open_business_case(flight: &FlightResponse) -> bool {
    flight.business_cases.iter().any(|case| {
        let status = string_field(case, "status")
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase();
        !matches!(
            status.as_str(),
            "SUCCESS" | "RESOLVED" | "FINISHED" | "CANCELLED" | "CANCELED"
        )
    })
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn calculate_data_freshness(
    flight: &FlightResponse,
    now: DateTime<Utc>,
) -> crate::schemas::flight_schemas::FlightDataFreshness {
    let updated_at = flight.updated_at;
    let source = "flight_response".to_string();
    let stale = updated_at
        .map(|ts| now.signed_duration_since(ts) > Duration::minutes(STALE_AFTER_MINUTES))
        .unwrap_or(true);

    crate::schemas::flight_schemas::FlightDataFreshness {
        source,
        updated_at,
        stale,
    }
}

fn freshness_score_value(freshness: &crate::schemas::flight_schemas::FlightDataFreshness) -> f64 {
    if !freshness.stale {
        1.0
    } else {
        0.4
    }
}

fn estimate_data_completeness(input: &FlightRiskInput, _now: &DateTime<Utc>) -> f64 {
    let mut score: f64 = 0.8;
    if input.include_weather {
        score += 0.1;
    }
    if input.include_manual_context {
        score += 0.1;
    }
    score.min(1.0)
}

fn estimate_data_freshness(_input: &FlightRiskInput, _now: &DateTime<Utc>) -> f64 {
    0.85
}

fn calculate_confidence(
    data_completeness: f64,
    data_freshness: f64,
    evidence_count: usize,
    limitation_count: usize,
) -> f64 {
    let mut score = (data_completeness + data_freshness) / 2.0;

    if evidence_count >= 3 {
        score += 0.1;
    } else if evidence_count == 0 {
        score -= 0.2;
    }

    if limitation_count > 2 {
        score -= 0.15 * (limitation_count as f64 - 2.0);
    }

    score.clamp(0.0, 1.0)
}

fn apply_risk_ceiling(score: i32, ceiling: Option<RiskLevel>) -> i32 {
    match ceiling {
        Some(RiskLevel::Low) => score.min(24),
        Some(RiskLevel::Medium) => score.min(49),
        Some(RiskLevel::High) => score.min(79),
        Some(RiskLevel::Critical) => score,
        None => score,
    }
}

pub fn apply_flight_risk_ex(
    flight: &mut FlightResponse,
    input: &FlightRiskInput,
    _now: DateTime<Utc>,
) -> FlightRiskOutput {
    let model = FlightRiskMicroModel::new();
    let output = model.execute_from_flight_data(input, flight);

    flight.risk_score = Some(output.risk_score);
    flight.risk_level = Some(output.risk_level.clone());
    flight.risk_reasons = Some(
        output
            .evidence
            .iter()
            .map(|e| FlightRiskReason {
                code: e.signal_code.clone(),
                label: e.signal_label.clone(),
                severity: e.severity.clone(),
            })
            .collect(),
    );

    output
}
