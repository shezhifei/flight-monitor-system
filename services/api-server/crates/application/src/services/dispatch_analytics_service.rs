//! 派工运营分析服务。

use chrono::{DateTime, Duration, Timelike, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::schemas::dispatch_schemas::{
    DispatchAnalyticsBreakdownItem, DispatchAnalyticsSummaryResponse, DispatchAnalyticsTrendItem,
};
use crate::types::{ConcreteDispatchQueryService, ConcreteResourceUtilizationService};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;

pub struct DispatchAnalyticsService {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    query_service: Arc<ConcreteDispatchQueryService>,
    resource_utilization_service: Arc<ConcreteResourceUtilizationService>,
}

impl DispatchAnalyticsService {
    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        query_service: Arc<ConcreteDispatchQueryService>,
        resource_utilization_service: Arc<ConcreteResourceUtilizationService>,
    ) -> Self {
        Self {
            order_repo,
            query_service,
            resource_utilization_service,
        }
    }

    pub async fn get_operations_summary(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<DispatchAnalyticsSummaryResponse, DomainError> {
        let (ws, we) = resolve_window(window_start, window_end);
        let orders = self.get_orders(ws, we).await?;
        let conflict_items = self.get_conflicts(ws, we, orders.len()).await?;
        let conflict_order_ids = conflict_order_ids(&conflict_items);
        let replanned_order_ids = self.get_replanned_order_ids(&orders).await?;
        let response_minutes = orders.iter().filter_map(response_minutes_for_order).collect::<Vec<_>>();
        let team_loads = team_occupied_minutes(&orders, ws, we);
        let equipment_utilization = self
            .resource_utilization_service
            .get_equipment_utilization(Some(ws), Some(we))
            .await?;
        let key_stats = key_order_stats(&orders);
        let assigned_orders = orders.iter().filter(|order| is_assigned(order)).count() as i32;
        let completed_orders = orders.iter().filter(|order| status_value(order) == "completed").count() as i32;

        let equipment_idle_rate = if equipment_utilization.is_empty() {
            0.0
        } else {
            let avg_rate = equipment_utilization
                .iter()
                .filter_map(|item| item.get("utilization_rate").and_then(Value::as_f64))
                .sum::<f64>()
                / equipment_utilization.len() as f64;
            round_to_4((1.0_f64 - avg_rate).max(0.0_f64))
        };
        let denominator = assigned_orders.max(1) as f64;

        Ok(DispatchAnalyticsSummaryResponse {
            window_start: ws,
            window_end: we,
            assigned_order_count: assigned_orders,
            completed_order_count: completed_orders,
            conflict_count: conflict_items.len() as i32,
            conflict_order_count: conflict_order_ids.len() as i32,
            conflict_rate: round_to_4(conflict_order_ids.len() as f64 / denominator),
            replanned_order_count: replanned_order_ids.len() as i32,
            replan_rate: round_to_4(replanned_order_ids.len() as f64 / denominator),
            avg_dispatch_response_minutes: average(&response_minutes),
            team_load_balance_score: load_balance_score(team_loads.values().copied().collect()),
            equipment_idle_rate,
            key_order_count: key_stats.0,
            key_order_ontime_rate: key_stats.1,
        })
    }

    pub async fn get_breakdown(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        group_by: &str,
    ) -> Result<Vec<DispatchAnalyticsBreakdownItem>, DomainError> {
        let (ws, we) = resolve_window(window_start, window_end);
        let orders = self.get_orders(ws, we).await?;
        let conflict_items = self.get_conflicts(ws, we, orders.len()).await?;
        let conflict_order_ids = conflict_order_ids(&conflict_items);
        let replanned_order_ids = self.get_replanned_order_ids(&orders).await?;

        #[derive(Default)]
        struct GroupStats {
            group_label: String,
            order_count: i32,
            assigned_order_count: i32,
            completed_order_count: i32,
            occupied_minutes: f64,
            conflict_order_count: i32,
            replanned_order_count: i32,
            response_minutes: Vec<f64>,
        }

        let mut groups: HashMap<String, GroupStats> = HashMap::new();
        for order in &orders {
            let (key, label) = group_key(order, group_by);
            let entry = groups.entry(key.clone()).or_default();
            entry.group_label = label;
            entry.order_count += 1;
            if is_assigned(order) {
                entry.assigned_order_count += 1;
            }
            if status_value(order) == "completed" {
                entry.completed_order_count += 1;
            }
            entry.occupied_minutes += occupied_minutes(order, ws, we);
            if conflict_order_ids.contains(&order.id) {
                entry.conflict_order_count += 1;
            }
            if replanned_order_ids.contains(&order.id) {
                entry.replanned_order_count += 1;
            }
            if let Some(value) = response_minutes_for_order(order) {
                entry.response_minutes.push(value);
            }
        }

        let mut items = groups
            .into_iter()
            .map(|(group_key, stats)| {
                let assigned_count = stats.assigned_order_count.max(1) as f64;
                DispatchAnalyticsBreakdownItem {
                    group_key,
                    group_label: stats.group_label,
                    order_count: stats.order_count,
                    assigned_order_count: stats.assigned_order_count,
                    completed_order_count: stats.completed_order_count,
                    occupied_minutes: round_to_2(stats.occupied_minutes),
                    conflict_order_count: stats.conflict_order_count,
                    conflict_rate: round_to_4(stats.conflict_order_count as f64 / assigned_count),
                    replanned_order_count: stats.replanned_order_count,
                    replan_rate: round_to_4(stats.replanned_order_count as f64 / assigned_count),
                    avg_dispatch_response_minutes: average(&stats.response_minutes),
                }
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .order_count
                .cmp(&left.order_count)
                .then_with(|| left.group_key.cmp(&right.group_key))
        });
        Ok(items)
    }

    pub async fn get_performance_trend(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        bucket: &str,
    ) -> Result<Vec<DispatchAnalyticsTrendItem>, DomainError> {
        if bucket != "hour" {
            return Err(DomainError::ValidationError("仅支持按小时聚合趋势".to_string()));
        }
        let (ws, we) = resolve_window(window_start, window_end);
        let orders = self.get_orders(ws, we).await?;
        let conflict_items = self.get_conflicts(ws, we, orders.len()).await?;
        let conflict_order_ids = conflict_order_ids(&conflict_items);
        let replanned_order_ids = self.get_replanned_order_ids(&orders).await?;

        #[derive(Default)]
        struct BucketStats {
            order_count: i32,
            conflict_order_count: i32,
            replanned_order_count: i32,
            response_minutes: Vec<f64>,
        }

        let mut buckets: HashMap<DateTime<Utc>, BucketStats> = HashMap::new();
        let mut cursor = ws
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .unwrap_or(ws);
        while cursor < we {
            buckets.insert(cursor, BucketStats::default());
            cursor += Duration::hours(1);
        }

        for order in &orders {
            let Some(start_time) = order_start(order) else {
                continue;
            };
            let Some(bucket_start) = start_time
                .with_minute(0)
                .and_then(|value| value.with_second(0))
                .and_then(|value| value.with_nanosecond(0))
            else {
                continue;
            };
            let Some(bucket_stats) = buckets.get_mut(&bucket_start) else {
                continue;
            };
            bucket_stats.order_count += 1;
            if conflict_order_ids.contains(&order.id) {
                bucket_stats.conflict_order_count += 1;
            }
            if replanned_order_ids.contains(&order.id) {
                bucket_stats.replanned_order_count += 1;
            }
            if let Some(value) = response_minutes_for_order(order) {
                bucket_stats.response_minutes.push(value);
            }
        }

        let mut items = buckets.into_iter().collect::<Vec<_>>();
        items.sort_by_key(|(bucket_start, _)| *bucket_start);
        Ok(items
            .into_iter()
            .map(|(bucket_start, stats)| DispatchAnalyticsTrendItem {
                bucket_start,
                bucket_end: bucket_start + Duration::hours(1),
                order_count: stats.order_count,
                conflict_order_count: stats.conflict_order_count,
                replanned_order_count: stats.replanned_order_count,
                avg_dispatch_response_minutes: average(&stats.response_minutes),
            })
            .collect())
    }

    async fn get_orders(&self, ws: DateTime<Utc>, we: DateTime<Utc>) -> Result<Vec<DispatchOrder>, DomainError> {
        self.order_repo
            .find_orders_in_window(ws, we, &[], None, None, None, false)
            .await
    }

    async fn get_conflicts(
        &self,
        ws: DateTime<Utc>,
        we: DateTime<Utc>,
        order_count: usize,
    ) -> Result<Vec<Value>, DomainError> {
        let limit = (order_count.max(1) * 8).max(200) as i64;
        self.query_service.list_conflicts_for_analytics(ws, we, limit).await
    }

    async fn get_replanned_order_ids(&self, orders: &[DispatchOrder]) -> Result<HashSet<String>, DomainError> {
        let mut result = HashSet::new();
        for order in orders {
            let logs = self.order_repo.list_logs(&order.id, 200).await?;
            if logs.iter().any(|item| {
                item.get("action")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .starts_with("replanned")
            }) {
                result.insert(order.id.clone());
            }
        }
        Ok(result)
    }
}

fn resolve_window(
    window_start: Option<DateTime<Utc>>,
    window_end: Option<DateTime<Utc>>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let end = window_end.unwrap_or_else(Utc::now);
    let start = window_start.unwrap_or_else(|| end - Duration::hours(12));
    (start, end)
}

fn conflict_order_ids(items: &[Value]) -> HashSet<String> {
    let mut result = HashSet::new();
    for item in items {
        if let Some(order_ids) = item.get("related_dispatch_order_ids").and_then(Value::as_array) {
            for order_id in order_ids {
                if let Some(value) = order_id.as_str() {
                    result.insert(value.to_string());
                }
            }
        }
    }
    result
}

fn status_value(order: &DispatchOrder) -> &'static str {
    match order.status {
        fms_domain::models::dispatch::DispatchOrderStatus::Pending => "pending",
        fms_domain::models::dispatch::DispatchOrderStatus::Assigned => "assigned",
        fms_domain::models::dispatch::DispatchOrderStatus::InProgress => "in_progress",
        fms_domain::models::dispatch::DispatchOrderStatus::Completed => "completed",
        fms_domain::models::dispatch::DispatchOrderStatus::Cancelled => "cancelled",
    }
}

fn is_assigned(order: &DispatchOrder) -> bool {
    order.individual_user_id.is_some() || !order.members.is_empty()
}

fn group_key(order: &DispatchOrder, group_by: &str) -> (String, String) {
    match group_by {
        "terminal" => {
            let value = order.terminal.clone().unwrap_or_else(|| "unknown".to_string());
            (value.clone(), value)
        }
        "step" => {
            let key = order.task_type.clone();
            let label = order.task_type_name.clone().unwrap_or_else(|| key.clone());
            (key, label)
        }
        // 班组不再是指派对象：默认按科室分组
        _ => {
            let key = order
                .department_id
                .clone()
                .or_else(|| order.department.clone())
                .unwrap_or_else(|| "unassigned".to_string());
            (key.clone(), key)
        }
    }
}

fn team_occupied_minutes(orders: &[DispatchOrder], ws: DateTime<Utc>, we: DateTime<Utc>) -> HashMap<String, f64> {
    // 工单不再挂班组：按科室聚合占用时长
    let mut result = HashMap::new();
    for order in orders {
        let Some(department_id) = order.department_id.as_ref() else {
            continue;
        };
        *result.entry(department_id.clone()).or_insert(0.0) += occupied_minutes(order, ws, we);
    }
    result
}

fn response_minutes_for_order(order: &DispatchOrder) -> Option<f64> {
    let created_at = order.created_at?;
    let dispatched_at = order.dispatched_at?;
    let diff = (dispatched_at - created_at).num_seconds() as f64 / 60.0;
    if diff < 0.0 {
        None
    } else {
        Some(round_to_2(diff))
    }
}

fn occupied_minutes(order: &DispatchOrder, ws: DateTime<Utc>, we: DateTime<Utc>) -> f64 {
    let Some(start) = order_start(order) else {
        return 0.0;
    };
    let end = order_end(order).unwrap_or(start + Duration::minutes(30));
    let overlap_start = start.max(ws);
    let overlap_end = end.min(we);
    if overlap_end <= overlap_start {
        return 0.0;
    }
    (overlap_end - overlap_start).num_seconds() as f64 / 60.0
}

fn order_start(order: &DispatchOrder) -> Option<DateTime<Utc>> {
    order
        .actual_start_time
        .or(order.planned_start_time)
        .or(order.assignment_deadline)
        .or(order.created_at)
}

fn order_end(order: &DispatchOrder) -> Option<DateTime<Utc>> {
    order
        .actual_end_time
        .or(order.estimated_completion_time)
        .or(order.planned_end_time)
        .or(order.actual_start_time)
        .or(order.planned_start_time)
}

fn key_order_stats(orders: &[DispatchOrder]) -> (i32, f64) {
    let key_orders = orders.iter().filter(|order| is_key_order(order)).collect::<Vec<_>>();
    if key_orders.is_empty() {
        return (0, 0.0);
    }
    let on_time_count = key_orders
        .iter()
        .filter(|order| {
            let actual_end = order_end(order);
            let deadline = order.assignment_deadline.or(order.planned_end_time);
            matches!((actual_end, deadline), (Some(actual_end), Some(deadline)) if actual_end <= deadline)
        })
        .count() as i32;
    (
        key_orders.len() as i32,
        round_to_4(on_time_count as f64 / key_orders.len() as f64),
    )
}

fn is_key_order(order: &DispatchOrder) -> bool {
    if order
        .workflow_context
        .get("is_key_flight")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let priority = order
        .workflow_context
        .get("priority")
        .or_else(|| order.workflow_context.get("flight_priority"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(priority.as_str(), "high" | "critical" | "urgent")
}

fn load_balance_score(values: Vec<f64>) -> f64 {
    if values.len() <= 1 {
        return 1.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean <= 0.0 {
        return 1.0;
    }
    let variance = values.iter().map(|value| (*value - mean).powi(2)).sum::<f64>() / values.len() as f64;
    let coefficient = variance.sqrt() / mean;
    round_to_4((1.0 - coefficient).clamp(0.0, 1.0))
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        round_to_2(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round_to_4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
