//! Flight history report and event journey generation.

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::business_case::FlightBusinessCase;

use super::helpers::{action_label, timestamp_from_value};
use super::types::FlightRuntimeService;

const FLIGHT_RUNTIME_MODEL: &str = "rust-flight-runtime";

impl FlightRuntimeService {
    pub async fn generate_history_report(
        &self,
        flight_id: &str,
        hours: i64,
        incident_type: Option<&str>,
    ) -> Result<Value, DomainError> {
        let flight = self
            .flight_service
            .get_flight(flight_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Flight",
                id: flight_id.to_string(),
            })?;
        let window_end = Utc::now();
        let window_start = window_end - Duration::hours(hours.clamp(1, 168));
        let history = self.history_within_window(flight_id, window_start, window_end).await?;
        let mut timeline = build_history_timeline(&history);
        if let Some(keyword) = incident_type.map(str::trim).filter(|value| !value.is_empty()) {
            let keyword_lower = keyword.to_lowercase();
            timeline.retain(|event| {
                let haystack = format!(
                    "{} {} {}",
                    event.get("title").and_then(Value::as_str).unwrap_or_default(),
                    event.get("detail").and_then(Value::as_str).unwrap_or_default(),
                    event
                )
                .to_lowercase();
                haystack.contains(&keyword_lower)
            });
        }
        let summary = build_history_summary(&timeline);
        let flight_no = flight.flight_number.clone();
        let markdown = build_history_markdown(
            flight_id,
            flight_no.as_deref(),
            window_start,
            window_end,
            &summary,
            &timeline,
        );
        let report_json = json!({
            "flight_id": flight_id,
            "flight_no": flight.flight_number,
            "window_start": window_start.to_rfc3339(),
            "window_end": window_end.to_rfc3339(),
            "timeline": timeline,
            "summary": summary,
            "incident_type": incident_type,
        });
        let _ = self.validate_report_schema("flight_history", &report_json).await;
        Ok(json!({
            "flight_id": flight_id,
            "flight_no": flight_no,
            "window_start": window_start.to_rfc3339(),
            "window_end": window_end.to_rfc3339(),
            "timeline": timeline,
            "summary": summary,
            "report_markdown": markdown,
            "report_json": report_json,
            "generated_at": Utc::now().to_rfc3339(),
            "model": FLIGHT_RUNTIME_MODEL,
        }))
    }

    pub async fn generate_event_journey(&self, flight_id: &str, hours: i64) -> Result<Value, DomainError> {
        let flight = self
            .flight_service
            .get_flight(flight_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Flight",
                id: flight_id.to_string(),
            })?;
        let window_end = Utc::now();
        let window_start = window_end - Duration::hours(hours.clamp(1, 168));
        let history = self.history_within_window(flight_id, window_start, window_end).await?;
        let flight_change_timeline = build_history_timeline(&history);
        let business_case_timeline = if let Some(service) = self.business_case_service.as_ref() {
            let cases = service.get_by_flight(flight_id).await?;
            build_business_case_timeline(&cases, window_start, window_end)
        } else {
            Vec::new()
        };
        let mut merged_timeline = business_case_timeline.clone();
        merged_timeline.extend(flight_change_timeline.clone());
        merged_timeline.sort_by(|left, right| timestamp_from_value(left).cmp(&timestamp_from_value(right)));
        let markdown = build_journey_markdown(
            flight_id,
            flight.flight_number.as_deref(),
            window_start,
            window_end,
            &merged_timeline,
        );
        let journey_json = json!({
            "flight_id": flight_id,
            "window_start": window_start.to_rfc3339(),
            "window_end": window_end.to_rfc3339(),
            "business_case_timeline": business_case_timeline,
            "flight_change_timeline": flight_change_timeline,
            "merged_timeline": merged_timeline,
        });
        let _ = self.validate_report_schema("flight_event_journey", &journey_json).await;
        Ok(json!({
            "flight_id": flight_id,
            "window_start": window_start.to_rfc3339(),
            "window_end": window_end.to_rfc3339(),
            "business_case_timeline": business_case_timeline,
            "flight_change_timeline": flight_change_timeline,
            "merged_timeline": merged_timeline,
            "journey_markdown": markdown,
            "journey_json": journey_json,
            "generated_at": Utc::now().to_rfc3339(),
            "model": FLIGHT_RUNTIME_MODEL,
        }))
    }

    async fn record_report_schema_validation(&self, report_type: &str, schema_valid: bool, error_count: usize) {
        let Some(ai_runtime_service) = self.ai_runtime_service.as_ref() else {
            return;
        };
        ai_runtime_service
            .record_report_schema_validation(schema_valid, "legacy", report_type, error_count)
            .await;
    }

    async fn validate_report_schema(&self, report_type: &str, report_json: &Value) -> (bool, usize) {
        let Some(report_map) = report_json.as_object() else {
            self.record_report_schema_validation(report_type, false, 1).await;
            return (false, 1);
        };

        let required_fields = [
            "schema_version",
            "report_type",
            "title",
            "summary",
            "time_range",
            "findings",
            "metrics",
            "actions",
            "sources",
        ];
        let error_count = required_fields
            .iter()
            .filter(|field| !report_map.contains_key(**field))
            .count();
        let schema_valid = error_count == 0;

        self.record_report_schema_validation(report_type, schema_valid, error_count)
            .await;

        (schema_valid, error_count)
    }

    async fn history_within_window(
        &self,
        flight_id: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<Value>, DomainError> {
        let history = self.get_flight_update_history(flight_id, 1, 500).await?;
        Ok(history
            .into_iter()
            .filter(|item| {
                let ts = timestamp_from_value(item);
                ts >= window_start && ts <= window_end
            })
            .collect())
    }
}

fn build_history_timeline(history: &[Value]) -> Vec<Value> {
    let mut items = history.iter().filter_map(normalize_history_record).collect::<Vec<_>>();
    items.sort_by(|left, right| timestamp_from_value(left).cmp(&timestamp_from_value(right)));
    items
}

fn normalize_history_record(record: &Value) -> Option<Value> {
    let timestamp = record.get("timestamp").and_then(Value::as_str)?.to_string();
    let changes = record.get("changes").cloned().unwrap_or_else(|| json!({}));
    let fields = changes
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let field_names = fields
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let action = record
        .get("action")
        .or_else(|| record.get("operation"))
        .and_then(Value::as_str)
        .unwrap_or("update");
    let severity = if field_names
        .iter()
        .any(|field| matches!(field.as_str(), "status" | "actual_departure" | "actual_arrival"))
    {
        "medium"
    } else {
        "low"
    };
    let title = if field_names.iter().any(|field| field == "status") {
        let next_status = changes
            .get("new")
            .and_then(Value::as_object)
            .and_then(|map| map.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN");
        format!("航班状态变更为 {next_status}")
    } else {
        format!("航班{}事件", action_label(action))
    };
    let detail = if field_names.is_empty() {
        "未提供字段变更明细".to_string()
    } else {
        format!("变更字段: {}", field_names.join(", "))
    };
    Some(json!({
        "timestamp": timestamp,
        "source": "flight_history",
        "title": title,
        "detail": detail,
        "severity": severity,
        "raw": record,
    }))
}

fn build_history_summary(timeline: &[Value]) -> Value {
    let total_events = timeline.len();
    let status_changes = timeline
        .iter()
        .filter(|item| {
            item.get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("状态变更")
        })
        .count();
    let key_field_changes = timeline
        .iter()
        .filter(|item| {
            let detail = item.get("detail").and_then(Value::as_str).unwrap_or_default();
            detail.contains("status")
                || detail.contains("estimated_departure")
                || detail.contains("actual_departure")
                || detail.contains("actual_arrival")
                || detail.contains("gate")
                || detail.contains("stand")
        })
        .count();
    json!({
        "total_events": total_events,
        "status_changes": status_changes,
        "key_field_changes": key_field_changes,
    })
}

fn build_business_case_timeline(
    cases: &[FlightBusinessCase],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Vec<Value> {
    let mut timeline = Vec::new();
    for case in cases {
        if case.created_at >= window_start && case.created_at <= window_end {
            timeline.push(json!({
                "timestamp": case.created_at.to_rfc3339(),
                "source": "business_case",
                "title": format!("事项创建：{}", case.case_type),
                "detail": case.description,
                "severity": if matches!(case.status.as_str(), "FAILED" | "DEAD_LETTER" | "CANCELLED") { "high" } else { "low" },
                "raw": case,
            }));
        }
        if let Some(finished_at) = case.finished_at {
            if finished_at >= window_start && finished_at <= window_end {
                timeline.push(json!({
                    "timestamp": finished_at.to_rfc3339(),
                    "source": "business_case",
                    "title": format!("事项完成：{}", case.case_type),
                    "detail": format!("状态: {}", case.status),
                    "severity": "low",
                    "raw": case,
                }));
            }
        }
        if let Some(cancelled_at) = case.cancelled_at {
            if cancelled_at >= window_start && cancelled_at <= window_end {
                timeline.push(json!({
                    "timestamp": cancelled_at.to_rfc3339(),
                    "source": "business_case",
                    "title": format!("事项取消：{}", case.case_type),
                    "detail": case.description,
                    "severity": "high",
                    "raw": case,
                }));
            }
        }
    }
    timeline.sort_by(|left, right| timestamp_from_value(left).cmp(&timestamp_from_value(right)));
    timeline
}

fn build_history_markdown(
    flight_id: &str,
    flight_no: Option<&str>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    summary: &Value,
    timeline: &[Value],
) -> String {
    let mut lines = vec![
        format!("# 航班动态/历史报表"),
        format!("- 航班ID: {flight_id}"),
        format!("- 航班号: {}", flight_no.unwrap_or("未知")),
        format!(
            "- 时间窗口: {} ~ {}",
            window_start.to_rfc3339(),
            window_end.to_rfc3339()
        ),
        format!(
            "- 事件总数: {}，状态变更: {}，关键字段变更: {}",
            summary.get("total_events").and_then(Value::as_u64).unwrap_or(0),
            summary.get("status_changes").and_then(Value::as_u64).unwrap_or(0),
            summary.get("key_field_changes").and_then(Value::as_u64).unwrap_or(0),
        ),
        String::new(),
        "## 关键时间线".to_string(),
    ];
    if timeline.is_empty() {
        lines.push("- 时间窗口内没有可用事件。".to_string());
    } else {
        for event in timeline.iter().take(12) {
            lines.push(format!(
                "- {} {}: {}",
                event.get("timestamp").and_then(Value::as_str).unwrap_or_default(),
                event.get("title").and_then(Value::as_str).unwrap_or_default(),
                event.get("detail").and_then(Value::as_str).unwrap_or_default(),
            ));
        }
    }
    lines.push(String::new());
    lines.push("## 风险提示".to_string());
    lines.push(
        if timeline
            .iter()
            .any(|event| event.get("severity").and_then(Value::as_str).unwrap_or_default() == "high")
        {
            "- 检测到高优先级事件，请结合运行席位与保障动作复核。".to_string()
        } else {
            "- 当前窗口内未检测到高优先级异常，但仍需关注后续资源和时刻变化。".to_string()
        },
    );
    lines.join("\n")
}

fn build_journey_markdown(
    flight_id: &str,
    flight_no: Option<&str>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    merged_timeline: &[Value],
) -> String {
    let mut lines = vec![
        "# 航班事件经过".to_string(),
        format!("- 航班ID: {flight_id}"),
        format!("- 航班号: {}", flight_no.unwrap_or("未知")),
        format!(
            "- 时间窗口: {} ~ {}",
            window_start.to_rfc3339(),
            window_end.to_rfc3339()
        ),
        String::new(),
        "## 事件主线".to_string(),
    ];
    if merged_timeline.is_empty() {
        lines.push("- 时间窗口内没有可串联的事件。".to_string());
    } else {
        for event in merged_timeline.iter().take(16) {
            lines.push(format!(
                "- [{}] {}: {}",
                event.get("source").and_then(Value::as_str).unwrap_or_default(),
                event.get("title").and_then(Value::as_str).unwrap_or_default(),
                event.get("detail").and_then(Value::as_str).unwrap_or_default(),
            ));
        }
    }
    lines.join("\n")
}
