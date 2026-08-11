use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::anomaly::*;
use fms_domain::models::dispatch::*;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    pub async fn build_workbench(&self, user_id: &str, max_orders: i64) -> Result<serde_json::Value, DomainError> {
        let orders = self.order.order_repo.find_by_user(user_id, None).await?;

        let mut pending = 0i64;
        let mut assigned = 0i64;
        let mut in_progress = 0i64;
        let mut completed = 0i64;
        let mut cancelled = 0i64;

        let mut order_items: Vec<serde_json::Value> = Vec::new();
        for o in &orders {
            let status = o.status.as_ref();
            match status {
                "pending" => pending += 1,
                "assigned" => assigned += 1,
                "inprogress" | "in_progress" => in_progress += 1,
                "completed" => completed += 1,
                "cancelled" => cancelled += 1,
                _ => {}
            }
            order_items.push(serde_json::json!({
                "order_id": o.id,
                "flight_id": o.flight_id,
                "task_type": o.task_type,
                "status": status,
                "terminal": o.terminal,
                "stand_id": o.stand_id,
                "gate": o.gate,
                "planned_start_time": o.planned_start_time,
                "planned_end_time": o.planned_end_time,
                "actual_start_time": o.actual_start_time,
                "assignment_deadline": o.assignment_deadline,
                "supervisor_notified": o.supervisor_notified,
                "created_at": o.created_at,
            }));
        }

        let total = order_items.len() as i64;
        order_items.truncate(max_orders.max(1) as usize);

        Ok(serde_json::json!({
            "user_id": user_id,
            "generated_at": Utc::now().to_rfc3339(),
            "my_orders": order_items,
            "order_counts": {
                "pending": pending,
                "assigned": assigned,
                "in_progress": in_progress,
                "completed": completed,
                "cancelled": cancelled,
                "total": total,
            },
            "notification_unread_count": 0,
            "chat_unread_total": 0,
            "pending_shift_handover_count": 0,
            "pending_sync_action_count": 0,
            "channel_recommendation": {
                "push": false,
                "sse": true,
                "in_app": true,
            },
        }))
    }

    /// 移动端运营事件流 (operations/events)
    pub async fn build_event_feed(&self, user_id: &str, limit: i64) -> Result<serde_json::Value, DomainError> {
        let orders = self.order.order_repo.find_by_user(user_id, None).await?;

        let mut events: Vec<serde_json::Value> = Vec::new();
        for o in &orders {
            let status = o.status.as_ref();
            let severity = match status {
                "pending" | "assigned" => "critical",
                "inprogress" | "in_progress" => "warning",
                _ => "info",
            };
            let occurred_at = o
                .actual_start_time
                .or(o.planned_start_time)
                .or(o.created_at)
                .unwrap_or_else(Utc::now);

            events.push(serde_json::json!({
                "event_id": format!("dispatch:{}", o.id),
                "event_type": "dispatch_order",
                "severity": severity,
                "status": status,
                "title": format!("工单 {}", o.task_type),
                "flight_id": o.flight_id,
                "occurred_at": occurred_at.to_rfc3339(),
                "source": "dispatch_orders",
                "payload": {
                    "order_id": o.id,
                    "task_type": o.task_type,
                    "planned_start_time": o.planned_start_time,
                    "planned_end_time": o.planned_end_time,
                    "assignment_deadline": o.assignment_deadline,
                },
            }));
        }

        events.sort_by(|a, b| {
            let ta = a.get("occurred_at").and_then(|v| v.as_str()).unwrap_or("");
            let tb = b.get("occurred_at").and_then(|v| v.as_str()).unwrap_or("");
            tb.cmp(ta)
        });
        events.truncate(limit.max(1) as usize);

        let total = events.len() as i64;
        let mut event_type_counts = std::collections::HashMap::<String, i64>::new();
        let mut severity_counts = std::collections::HashMap::<String, i64>::new();
        for e in &events {
            let et = e.get("event_type").and_then(|v| v.as_str()).unwrap_or("unknown");
            let sv = e.get("severity").and_then(|v| v.as_str()).unwrap_or("info");
            *event_type_counts.entry(et.to_string()).or_insert(0) += 1;
            *severity_counts.entry(sv.to_string()).or_insert(0) += 1;
        }

        Ok(serde_json::json!({
            "user_id": user_id,
            "generated_at": Utc::now().to_rfc3339(),
            "total": total,
            "event_type_counts": event_type_counts,
            "severity_counts": severity_counts,
            "events": events,
        }))
    }
}
