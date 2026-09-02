use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::models::anomaly::AnomalyStatus;
use fms_domain::models::value_objects::FlightStatus;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::flight_repository::FlightRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_datetime, arg_str, evidence, BRIEFING_UPCOMING_TASKS_MAX};

pub struct BriefingService {
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
}

impl BriefingService {
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_repo,
            dispatch_repo,
            anomaly_repo,
        }
    }

    pub async fn generate(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let now = Utc::now();
        let shift_start = arg_datetime(args, "shift_start")?.unwrap_or(now);
        let shift_end = arg_datetime(args, "shift_end")?.unwrap_or(shift_start + chrono::Duration::hours(8));
        if shift_end <= shift_start {
            return Err(OntologyActionError::InvalidArguments(
                "`shift_end` must be after `shift_start`".to_string(),
            ));
        }
        let scope = arg_str(args, "scope").unwrap_or("all");
        if !matches!(scope, "all" | "inbound" | "outbound") {
            return Err(OntologyActionError::InvalidArguments(
                "`scope` must be one of all|inbound|outbound".to_string(),
            ));
        }
        let department_id = arg_str(args, "department_id");

        let mut flights = Vec::new();
        let mut day = shift_start.date_naive();
        let last_day = shift_end.date_naive();
        let mut days_scanned = 0i32;
        while day <= last_day && days_scanned < 2 {
            let day_flights = self.flight_repo.find_by_date(day).await.map_err(repo_err)?;
            flights.extend(day_flights);
            day = day
                .succ_opt()
                .ok_or_else(|| OntologyActionError::Internal("date overflow".to_string()))?;
            days_scanned += 1;
        }
        flights.sort_by(|a, b| a.flight_id.as_str().cmp(b.flight_id.as_str()));
        flights.dedup_by(|a, b| a.flight_id == b.flight_id);
        flights.retain(|flight| {
            let in_window = [
                flight.scheduled_departure,
                flight.scheduled_arrival,
                flight.estimated_departure,
                flight.estimated_arrival,
            ]
            .into_iter()
            .flatten()
            .any(|moment| moment >= shift_start && moment <= shift_end);
            let in_scope = match scope {
                "inbound" => flight.is_arrival_flight(),
                "outbound" => flight.is_departure_flight(),
                _ => true,
            };
            in_window && in_scope
        });

        let arrivals = flights.iter().filter(|flight| flight.is_arrival_flight()).count();
        let departures = flights.iter().filter(|flight| flight.is_departure_flight()).count();
        let cancelled = flights
            .iter()
            .filter(|flight| flight.status == FlightStatus::Cancelled)
            .count();
        let delayed = flights
            .iter()
            .filter(|flight| flight.status == FlightStatus::Delayed)
            .count();

        let orders = self
            .dispatch_repo
            .find_orders_in_window(shift_start, shift_end, &[], None, department_id, None, true)
            .await
            .map_err(repo_err)?;
        let pending = orders.iter().filter(|order| order.status.as_ref() == "pending").count();
        let in_progress = orders
            .iter()
            .filter(|order| order.status.as_ref() == "in_progress")
            .count();
        let completed = orders
            .iter()
            .filter(|order| order.status.as_ref() == "completed")
            .count();

        let mut upcoming: Vec<Value> = orders
            .iter()
            .filter(|order| matches!(order.status.as_ref(), "pending" | "assigned"))
            .filter(|order| order.planned_start_time.is_some_and(|moment| moment >= now))
            .map(|order| {
                json!({
                    "dispatch_order_id": order.id,
                    "task_type": order.task_type,
                    "flight_id": order.flight_id,
                    "planned_time": order.planned_start_time,
                })
            })
            .collect();
        upcoming.sort_by(|a, b| a["planned_time"].to_string().cmp(&b["planned_time"].to_string()));
        upcoming.truncate(BRIEFING_UPCOMING_TASKS_MAX);

        let open_anomalies = self
            .anomaly_repo
            .find_by_status(AnomalyStatus::Open)
            .await
            .map_err(repo_err)?;
        let critical_open = open_anomalies
            .iter()
            .filter(|anomaly| anomaly.severity.as_ref() == "critical")
            .count();

        let mut limitations = vec![
            "flights_summary 基于按日检索 + 时间窗过滤，跨 2 天以上的班次不完全覆盖",
            "dispatch_summary 未过滤部门以外的组织维度",
        ];
        if flights.is_empty() {
            limitations.push("窗口内未检索到航班，汇总可能反映数据缺口而非零运行量");
        }
        let confidence = if flights.is_empty() { 0.5 } else { 0.9 };

        Ok(json!({
            "briefing": {
                "title": format!("Operations briefing {} ~ {}", shift_start, shift_end),
                "generated_at": now,
                "flights_summary": {
                    "total": flights.len(),
                    "arrivals": arrivals,
                    "departures": departures,
                    "delayed": delayed,
                    "cancelled": cancelled,
                },
                "dispatch_summary": {
                    "total": orders.len(),
                    "pending": pending,
                    "in_progress": in_progress,
                    "completed": completed,
                },
                "anomaly_summary": {
                    "open": open_anomalies.len(),
                    "critical": critical_open,
                },
                "upcoming_tasks": upcoming,
                "checklist": [
                    "确认窗口内 critical 异常已认领",
                    "核对即将开始的派工单资源到位情况",
                    "复核延误/取消航班的机位与登机口占用",
                ],
            },
            "confidence": confidence,
            "limitations": limitations,
            "evidence": evidence(None),
        }))
    }
}
