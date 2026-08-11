use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

use crate::schemas::flight_schemas::{
    FlightDataFreshness, FlightNextAction, FlightResponse, FlightRiskAssessment, FlightRiskReason,
};

const STALE_AFTER_MINUTES: i64 = 30;

/// Deterministic flight risk model for read models.
///
/// Signals are intentionally simple and explainable:
/// - open anomalies dominate the score because they represent known operational issues;
/// - departure delay/status drift adds urgency for coordination;
/// - VIP flights add priority but do not create critical risk alone;
/// - open business cases indicate work that still needs closure;
/// - stale or missing flight update timestamps lower confidence and require refresh.
pub fn score_flight_risk(flight: &FlightResponse, now: DateTime<Utc>) -> FlightRiskAssessment {
    let mut score = 0;
    let mut reasons = Vec::new();

    if flight.anomaly_summary.has_open_anomaly || flight.anomaly_summary.open_count > 0 {
        let severity = if flight.anomaly_summary.open_count >= 2 {
            "critical"
        } else {
            "high"
        };
        score += if severity == "critical" { 50 } else { 40 };
        reasons.push(reason(
            "open_anomaly",
            format!("{} open anomaly item(s)", flight.anomaly_summary.open_count.max(1)),
            severity,
        ));
    }

    if has_departure_delay(flight) {
        score += 25;
        reasons.push(reason(
            "departure_delay",
            "Estimated or current status indicates departure delay",
            "high",
        ));
    }

    if is_vip_flight(flight) {
        score += 15;
        reasons.push(reason(
            "vip_flight",
            "VIP flight requires elevated operational attention",
            "medium",
        ));
    }

    if has_open_business_case(flight) {
        score += 10;
        reasons.push(reason(
            "open_business_case",
            "Flight has unfinished business case work",
            "medium",
        ));
    }

    let freshness = data_freshness(flight, now);
    if freshness.stale {
        score += 10;
        reasons.push(reason(
            "stale_data",
            "Flight data is stale or missing an update timestamp",
            "medium",
        ));
    }

    let risk_score = score.clamp(0, 100);
    let risk_level = match risk_score {
        0..=24 => "low",
        25..=49 => "medium",
        50..=79 => "high",
        _ => "critical",
    }
    .to_string();

    FlightRiskAssessment {
        risk_score,
        risk_level,
        risk_reasons: reasons,
        next_primary_action: next_primary_action(flight),
        data_freshness: freshness,
    }
}

pub fn apply_flight_risk(flight: &mut FlightResponse, now: DateTime<Utc>) {
    let assessment = score_flight_risk(flight, now);
    flight.risk_score = Some(assessment.risk_score);
    flight.risk_level = Some(assessment.risk_level);
    flight.risk_reasons = Some(assessment.risk_reasons);
    flight.next_primary_action = assessment.next_primary_action;
    flight.data_freshness = Some(assessment.data_freshness);
}

fn reason(code: &str, label: impl Into<String>, severity: &str) -> FlightRiskReason {
    FlightRiskReason {
        code: code.to_string(),
        label: label.into(),
        severity: severity.to_string(),
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
    flight.inbound_leg.as_ref().map(|leg| leg.is_vip).unwrap_or(false)
        || flight.outbound_leg.as_ref().map(|leg| leg.is_vip).unwrap_or(false)
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

fn data_freshness(flight: &FlightResponse, now: DateTime<Utc>) -> FlightDataFreshness {
    let stale = flight
        .updated_at
        .map(|updated_at| now.signed_duration_since(updated_at) > Duration::minutes(STALE_AFTER_MINUTES))
        .unwrap_or(true);

    FlightDataFreshness {
        source: "flight.updated_at".to_string(),
        updated_at: flight.updated_at,
        stale,
    }
}

fn next_primary_action(flight: &FlightResponse) -> Option<FlightNextAction> {
    let flight_id = flight.flight_id.as_deref().unwrap_or_default();
    if flight.anomaly_summary.has_open_anomaly || flight.anomaly_summary.open_count > 0 {
        return Some(FlightNextAction {
            code: "resolve_anomaly".to_string(),
            label: "Resolve open anomaly".to_string(),
            target: format!("/api/v2/anomalies?flight_id={flight_id}&status=open"),
            reason: "Open anomalies have the highest operational priority".to_string(),
        });
    }

    if has_open_business_case(flight) {
        return Some(FlightNextAction {
            code: "review_business_case".to_string(),
            label: "Review business case".to_string(),
            target: format!("/api/v2/business-cases?flight_id={flight_id}"),
            reason: "Unfinished business case work needs closure".to_string(),
        });
    }

    if has_departure_delay(flight) {
        return Some(FlightNextAction {
            code: "verify_departure_plan".to_string(),
            label: "Verify departure plan".to_string(),
            target: format!("/api/v2/flights/{flight_id}"),
            reason: "Departure timing indicates delay risk".to_string(),
        });
    }

    if is_vip_flight(flight) {
        return Some(FlightNextAction {
            code: "monitor_vip_service".to_string(),
            label: "Monitor VIP service".to_string(),
            target: format!("/api/v2/flights/{flight_id}"),
            reason: "VIP flights require proactive monitoring".to_string(),
        });
    }

    None
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| match item {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serde_json::json;

    use crate::schemas::flight_schemas::{FlightAnomalySummary, FlightLegPayload, FlightResponse};
    use crate::services::flight_risk_service::score_flight_risk;

    fn base_flight() -> FlightResponse {
        let now = Utc::now();
        FlightResponse {
            flight_id: Some("flight-1".to_string()),
            flight_number: Some("CA1234".to_string()),
            airline_code: None,
            registration: None,
            aircraft_type_detail: None,
            status: Some("DELAYED".to_string()),
            scheduled_departure: Some(now - Duration::minutes(30)),
            scheduled_arrival: None,
            estimated_departure: Some(now + Duration::minutes(40)),
            estimated_arrival: None,
            actual_departure: None,
            actual_arrival: None,
            cobt_time: None,
            codt: None,
            on_blocks_time: None,
            cabin_door_open_time: None,
            deboarding_complete_time: None,
            cleaning_start_time: None,
            cleaning_end_time: None,
            boarding_allowed_time: None,
            start_boarding_time: None,
            passenger_ready_time: None,
            end_boarding_time: None,
            cabin_door_close_time: None,
            cargo_door_close_time: None,
            loading_complete_time: None,
            off_blocks_time: None,
            stand: Some("S1".to_string()),
            gate: Some("G1".to_string()),
            terminal: None,
            position: None,
            baggage_carousel: None,
            has_boarding_restriction: false,
            is_quick_turnaround: false,
            is_commercial_signed: true,
            inbound_leg: None,
            outbound_leg: Some(FlightLegPayload {
                leg_type: "outbound".to_string(),
                flight_no: "CA1234".to_string(),
                flight_type: "domestic".to_string(),
                mission: None,
                origin_stations: Vec::new(),
                destination_stations: Vec::new(),
                origin_code: None,
                destination_code: None,
                origin_name: None,
                destination_name: None,
                is_vip: true,
                stand_type: None,
                scheduled_time: Some(now - Duration::minutes(30)),
            }),
            anomaly_summary: FlightAnomalySummary {
                has_open_anomaly: true,
                open_count: 2,
                acknowledged_count: 1,
            },
            business_cases: vec![json!({
                "case_id": "case-1",
                "case_type": "gate_check",
                "status": "PENDING",
                "description": "Need gate confirmation"
            })],
            created_at: Some(now - Duration::hours(2)),
            updated_at: Some(now - Duration::minutes(45)),
            version: 3,
            labels: Vec::new(),
            flight_remarks: None,
            load_planning_remarks: None,
            aircraft_maintenance_remarks: None,
            aircraft_check_remarks: None,
            created_by: None,
            updated_by: None,
            risk_score: None,
            risk_level: None,
            risk_reasons: None,
            next_primary_action: None,
            data_freshness: None,
        }
    }

    #[test]
    fn risk_scoring_explains_operational_reasons_and_next_action() {
        let risk = score_flight_risk(&base_flight(), Utc::now());

        assert_eq!(risk.risk_level, "critical");
        assert!(risk.risk_score >= 90);
        assert!(risk
            .risk_reasons
            .iter()
            .any(|reason| reason.code == "open_anomaly" && reason.severity == "critical"));
        assert!(risk
            .risk_reasons
            .iter()
            .any(|reason| reason.code == "departure_delay" && reason.severity == "high"));
        assert!(risk
            .risk_reasons
            .iter()
            .any(|reason| reason.code == "vip_flight" && reason.severity == "medium"));
        assert_eq!(
            risk.next_primary_action.as_ref().map(|action| action.code.as_str()),
            Some("resolve_anomaly")
        );
        assert!(risk.data_freshness.stale);
        assert_eq!(risk.data_freshness.source, "flight.updated_at");
    }
}
