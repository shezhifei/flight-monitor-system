//! 资源利用率应用服务。
//!
//! 对齐 Python `resource_utilization_service.py`。

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;

pub struct ResourceUtilizationService {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
}

impl ResourceUtilizationService {
    pub fn new(order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>) -> Self {
        Self { order_repo }
    }

    pub async fn get_summary(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Value, DomainError> {
        let (window_start, window_end) = resolve_window(window_start, window_end);
        let window_minutes = window_minutes(window_start, window_end);
        let orders = self.get_active_orders(window_start, window_end).await?;

        let stand_items = self.compute_stand_utilization(&orders, window_start, window_end, window_minutes);
        let team_items = self.compute_team_workload(&orders, window_start, window_end, window_minutes);
        let equipment_items = self.compute_equipment_utilization(&orders, window_start, window_end, window_minutes);

        Ok(json!({
            "window_start": window_start.to_rfc3339(),
            "window_end": window_end.to_rfc3339(),
            "stand_utilization_rate": average_rate(&stand_items),
            "team_utilization_rate": average_rate(&team_items),
            "equipment_utilization_rate": average_rate(&equipment_items),
            "stand_count": stand_items.len(),
            "team_count": team_items.len(),
            "equipment_count": equipment_items.len(),
        }))
    }

    pub async fn get_stand_utilization(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<Value>, DomainError> {
        let (window_start, window_end) = resolve_window(window_start, window_end);
        let window_minutes = window_minutes(window_start, window_end);
        let orders = self.get_active_orders(window_start, window_end).await?;
        Ok(self.compute_stand_utilization(&orders, window_start, window_end, window_minutes))
    }

    pub async fn get_team_workload(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<Value>, DomainError> {
        let (window_start, window_end) = resolve_window(window_start, window_end);
        let window_minutes = window_minutes(window_start, window_end);
        let orders = self.get_active_orders(window_start, window_end).await?;
        Ok(self.compute_team_workload(&orders, window_start, window_end, window_minutes))
    }

    pub async fn get_equipment_utilization(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<Value>, DomainError> {
        let (window_start, window_end) = resolve_window(window_start, window_end);
        let window_minutes = window_minutes(window_start, window_end);
        let orders = self.get_active_orders(window_start, window_end).await?;
        Ok(self.compute_equipment_utilization(&orders, window_start, window_end, window_minutes))
    }

    async fn get_active_orders(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        self.order_repo
            .find_orders_in_window(window_start, window_end, &[], None, None, None, false)
            .await
    }

    fn compute_stand_utilization(
        &self,
        orders: &[DispatchOrder],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        window_minutes: f64,
    ) -> Vec<Value> {
        let mut stand_minutes: HashMap<String, f64> = HashMap::new();
        let mut stand_names: HashMap<String, String> = HashMap::new();

        for order in orders {
            let Some(stand_id) = order.stand_id.as_deref() else {
                continue;
            };
            stand_names
                .entry(stand_id.to_string())
                .or_insert_with(|| order.stand_code.clone().unwrap_or_else(|| stand_id.to_string()));
            let occupied_minutes = overlap_minutes(order, window_start, window_end);
            *stand_minutes.entry(stand_id.to_string()).or_insert(0.0) += occupied_minutes;
        }

        let mut results = stand_minutes
            .into_iter()
            .map(|(stand_id, occupied_minutes)| {
                json!({
                    "stand_id": stand_id,
                    "stand_code": stand_names.get(&stand_id).cloned().unwrap_or(stand_id.clone()),
                    "occupied_minutes": round_one_decimal(occupied_minutes),
                    "utilization_rate": round_four((occupied_minutes / window_minutes).min(1.0)),
                })
            })
            .collect::<Vec<_>>();

        sort_desc_by_rate(&mut results);
        results
    }

    fn compute_team_workload(
        &self,
        orders: &[DispatchOrder],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        window_minutes: f64,
    ) -> Vec<Value> {
        let mut team_minutes: HashMap<String, f64> = HashMap::new();
        let mut team_order_count: HashMap<String, i64> = HashMap::new();

        // 工单不再挂班组：按成员的班组名册来源（source_team_id）聚合
        for order in orders {
            let occupied_minutes = overlap_minutes(order, window_start, window_end);
            let mut seen_in_order = std::collections::HashSet::new();
            for member in order.members.iter().filter(|member| member.is_active) {
                let Some(team_id) = member
                    .source_team_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                if seen_in_order.insert(team_id.to_string()) {
                    *team_minutes.entry(team_id.to_string()).or_insert(0.0) += occupied_minutes;
                    *team_order_count.entry(team_id.to_string()).or_insert(0) += 1;
                }
            }
        }

        let mut results = team_minutes
            .into_iter()
            .map(|(team_id, occupied_minutes)| {
                json!({
                    "team_id": team_id,
                    "order_count": team_order_count.get(&team_id).copied().unwrap_or(0),
                    "occupied_minutes": round_one_decimal(occupied_minutes),
                    "utilization_rate": round_four((occupied_minutes / window_minutes).min(1.0)),
                })
            })
            .collect::<Vec<_>>();

        sort_desc_by_rate(&mut results);
        results
    }

    fn compute_equipment_utilization(
        &self,
        orders: &[DispatchOrder],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        window_minutes: f64,
    ) -> Vec<Value> {
        let mut equipment_minutes: HashMap<String, f64> = HashMap::new();
        let mut equipment_names: HashMap<String, String> = HashMap::new();

        for order in orders {
            let occupied_minutes = overlap_minutes(order, window_start, window_end);
            for equipment in &order.equipment_list {
                let equipment_id = equipment.id.clone();
                equipment_names
                    .entry(equipment_id.clone())
                    .or_insert_with(|| equipment.name.clone().unwrap_or_else(|| equipment.code.clone()));
                *equipment_minutes.entry(equipment_id).or_insert(0.0) += occupied_minutes;
            }
        }

        let mut results = equipment_minutes
            .into_iter()
            .map(|(equipment_id, occupied_minutes)| {
                json!({
                    "equipment_id": equipment_id,
                    "equipment_name": equipment_names
                        .get(&equipment_id)
                        .cloned()
                        .unwrap_or_else(|| equipment_id.clone()),
                    "occupied_minutes": round_one_decimal(occupied_minutes),
                    "utilization_rate": round_four((occupied_minutes / window_minutes).min(1.0)),
                })
            })
            .collect::<Vec<_>>();

        sort_desc_by_rate(&mut results);
        results
    }
}

fn resolve_window(
    window_start: Option<DateTime<Utc>>,
    window_end: Option<DateTime<Utc>>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let window_end = window_end.unwrap_or(now);
    let window_start = window_start.unwrap_or_else(|| now - Duration::hours(12));
    (window_start, window_end)
}

fn effective_start(order: &DispatchOrder) -> Option<DateTime<Utc>> {
    order.planned_start_time.or(order.actual_start_time)
}

fn effective_end(order: &DispatchOrder, start: DateTime<Utc>) -> DateTime<Utc> {
    order
        .planned_end_time
        .or(order.actual_end_time)
        .unwrap_or(start + Duration::minutes(30))
}

fn overlap_minutes(order: &DispatchOrder, window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> f64 {
    let Some(start) = effective_start(order) else {
        return 0.0;
    };
    let end = effective_end(order, start);
    let overlap_start = start.max(window_start);
    let overlap_end = end.min(window_end);
    if overlap_start >= overlap_end {
        return 0.0;
    }
    (overlap_end - overlap_start).num_seconds() as f64 / 60.0
}

fn window_minutes(window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> f64 {
    ((window_end - window_start).num_seconds().max(60) as f64) / 60.0
}

fn average_rate(items: &[Value]) -> f64 {
    if items.is_empty() {
        return 0.0;
    }
    let total = items
        .iter()
        .filter_map(|item| item.get("utilization_rate").and_then(Value::as_f64))
        .sum::<f64>();
    round_four(total / items.len() as f64)
}

fn sort_desc_by_rate(items: &mut [Value]) {
    items.sort_by(|left, right| {
        let left_rate = left.get("utilization_rate").and_then(Value::as_f64).unwrap_or(0.0);
        let right_rate = right.get("utilization_rate").and_then(Value::as_f64).unwrap_or(0.0);
        right_rate.partial_cmp(&left_rate).unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round_four(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}
