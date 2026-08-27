//! 派工场景预览服务。

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::schemas::dispatch_schemas::DispatchScenarioPreviewRequest;
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;

pub struct DispatchScenarioService {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
}

impl DispatchScenarioService {
    pub fn new(order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>) -> Self {
        Self { order_repo }
    }

    pub async fn preview(&self, payload: &DispatchScenarioPreviewRequest) -> Result<Value, DomainError> {
        let statuses = ["pending", "assigned", "in_progress", "completed"];
        let orders = self
            .order_repo
            .find_orders_in_window(
                payload.window_start,
                payload.window_end,
                &statuses,
                None,
                None,
                None,
                false,
            )
            .await?;
        let mut states = build_states(&orders);

        let unavailable_equipment = payload
            .equipment_unavailable_ids
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<HashSet<_>>();
        let closed_stands = payload
            .closed_stand_ids
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<HashSet<_>>();
        let frozen_ids = payload
            .frozen_order_ids
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<HashSet<_>>();
        let delayed_map = payload
            .delayed_orders
            .iter()
            .filter_map(|item| {
                let order_id = item.dispatch_order_id.trim();
                if order_id.is_empty() || item.delay_minutes <= 0 {
                    None
                } else {
                    Some((order_id.to_string(), item.delay_minutes))
                }
            })
            .collect::<HashMap<_, _>>();

        let mut impacted_orders = Vec::new();
        let mut projected_conflicts = Vec::new();
        let mut recommendations = Vec::new();
        let mut changed_orders = HashSet::new();
        let mut impacted_order_ids = HashSet::new();
        let mut equipment_impacted = 0;
        let mut stand_impacted = 0;

        for (order_id, delay_minutes) in delayed_map {
            let Some(state) = states.get_mut(&order_id) else {
                continue;
            };
            state.projected_start_time += Duration::minutes(delay_minutes as i64);
            state.projected_end_time += Duration::minutes(delay_minutes as i64);
            changed_orders.insert(order_id.clone());
            impacted_order_ids.insert(order_id.clone());
            impacted_orders.push(impact_item(
                state,
                "delay",
                if delay_minutes < 20 { "medium" } else { "high" },
                &format!("订单预计顺延 {delay_minutes} 分钟"),
            ));
            recommendations.push(json!({
                "dispatch_order_id": order_id,
                "action": "shift_window",
                "reason": format!("任务预计延迟 {delay_minutes} 分钟，建议在局部窗口内重排后续任务"),
                "requires_manual_confirmation": frozen_ids.contains(&state.order_id) || delay_minutes >= 30,
            }));
        }

        for state in states.values() {
            let impacted_equipment = state
                .equipment_ids
                .intersection(&unavailable_equipment)
                .cloned()
                .collect::<Vec<_>>();
            if !impacted_equipment.is_empty() {
                equipment_impacted += 1;
                impacted_order_ids.insert(state.order_id.clone());
                projected_conflicts.push(conflict(
                    "equipment_unavailable",
                    "high",
                    Some(impacted_equipment[0].clone()),
                    None,
                    vec![state.order_id.clone()],
                    "设备在仿真场景中不可用",
                    "更换设备或改派其它资源",
                ));
                impacted_orders.push(impact_item(
                    state,
                    "equipment_unavailable",
                    "high",
                    "当前派工依赖设备在场景中停用",
                ));
                recommendations.push(json!({
                    "dispatch_order_id": state.order_id,
                    "action": "replace_equipment",
                    "reason": "涉及停机设备，建议优先尝试替换设备资源",
                    "requires_manual_confirmation": frozen_ids.contains(&state.order_id),
                }));
            }

            if !state.stand_id.is_empty() && closed_stands.contains(&state.stand_id) {
                stand_impacted += 1;
                impacted_order_ids.insert(state.order_id.clone());
                projected_conflicts.push(conflict(
                    "stand_closed",
                    "high",
                    Some(state.stand_id.clone()),
                    state.stand_code.clone(),
                    vec![state.order_id.clone()],
                    "机位在仿真场景中关闭",
                    "切换机位或发起人工确认",
                ));
                impacted_orders.push(impact_item(
                    state,
                    "stand_closed",
                    "high",
                    "当前派工所在机位在场景中关闭",
                ));
                recommendations.push(json!({
                    "dispatch_order_id": state.order_id,
                    "action": "manual_review",
                    "reason": "机位关闭通常需要联动航班与现场指挥确认",
                    "requires_manual_confirmation": true,
                }));
            }
        }

        projected_conflicts.extend(project_overlap_conflicts(&states));
        for conflict in &projected_conflicts {
            if let Some(items) = conflict.get("related_dispatch_order_ids").and_then(Value::as_array) {
                for item in items {
                    if let Some(order_id) = item.as_str() {
                        impacted_order_ids.insert(order_id.to_string());
                    }
                }
            }
        }

        for conflict in &projected_conflicts {
            let related_order_ids = conflict
                .get("related_dispatch_order_ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for order_id in related_order_ids.iter().filter_map(Value::as_str) {
                if !states.contains_key(order_id) {
                    continue;
                }
                recommendations.push(json!({
                    "dispatch_order_id": order_id,
                    "action": "local_replan",
                    "reason": conflict.get("message").and_then(Value::as_str).unwrap_or("场景扰动导致资源冲突"),
                    "requires_manual_confirmation": frozen_ids.contains(order_id) || conflict.get("severity").and_then(Value::as_str) == Some("high"),
                }));
            }
        }

        let delayed_orders_count = payload
            .delayed_orders
            .iter()
            .filter(|item| states.contains_key(item.dispatch_order_id.trim()))
            .count();
        let requires_manual_confirmation = stand_impacted > 0
            || recommendations.iter().any(|item| {
                item.get("requires_manual_confirmation")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            || impacted_order_ids.iter().any(|item| frozen_ids.contains(item));
        let risk_level = risk_level(
            projected_conflicts.len() as i32,
            impacted_order_ids.len() as i32,
            stand_impacted,
            equipment_impacted,
        );

        Ok(json!({
            "window_start": payload.window_start,
            "window_end": payload.window_end,
            "generated_at": Utc::now(),
            "impact_summary": {
                "impacted_orders": impacted_order_ids.len(),
                "projected_conflicts": projected_conflicts.len(),
                "delayed_orders": delayed_orders_count,
                "equipment_unavailable_orders": equipment_impacted,
                "stand_closed_orders": stand_impacted,
            },
            "projected_conflicts": deduplicate_conflicts(projected_conflicts),
            "impacted_orders": deduplicate_items(impacted_orders, |item| {
                format!("{}:{}", item.get("dispatch_order_id").and_then(Value::as_str).unwrap_or_default(), item.get("impact_type").and_then(Value::as_str).unwrap_or_default())
            }),
            "recommendations": deduplicate_items(recommendations, |item| {
                format!("{}:{}", item.get("dispatch_order_id").and_then(Value::as_str).unwrap_or_default(), item.get("action").and_then(Value::as_str).unwrap_or_default())
            }),
            "changed_orders": sorted_strings(changed_orders),
            "risk_level": risk_level,
            "requires_manual_confirmation": requires_manual_confirmation,
        }))
    }
}

#[derive(Clone)]
struct ScenarioOrderState {
    order_id: String,
    flight_id: Option<String>,
    team_id: String,
    team_name: Option<String>,
    individual_user_id: String,
    individual_username: Option<String>,
    stand_id: String,
    stand_code: Option<String>,
    equipment_ids: HashSet<String>,
    original_start_time: DateTime<Utc>,
    original_end_time: DateTime<Utc>,
    projected_start_time: DateTime<Utc>,
    projected_end_time: DateTime<Utc>,
}

fn build_states(orders: &[DispatchOrder]) -> HashMap<String, ScenarioOrderState> {
    let mut result = HashMap::new();
    for order in orders {
        let Some(original_start_time) = order_start(order) else {
            continue;
        };
        let original_end_time = order_end(order).unwrap_or(original_start_time + Duration::minutes(30));
        result.insert(
            order.id.clone(),
            ScenarioOrderState {
                order_id: order.id.clone(),
                flight_id: Some(order.flight_id.clone()),
                team_id: String::new(),
                team_name: None,
                individual_user_id: opt_string_or_default(&order.individual_user_id),
                individual_username: order.individual_username.clone(),
                stand_id: opt_string_or_default(&order.stand_id),
                stand_code: order.stand_code.clone(),
                equipment_ids: order
                    .equipment_list
                    .iter()
                    .map(|item| item.id.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect(),
                original_start_time,
                original_end_time,
                projected_start_time: original_start_time,
                projected_end_time: original_end_time,
            },
        );
    }
    result
}

fn opt_string_or_default(value: &Option<String>) -> String {
    value.as_deref().unwrap_or_default().to_owned()
}

fn project_overlap_conflicts(states: &HashMap<String, ScenarioOrderState>) -> Vec<Value> {
    let values = states.values().cloned().collect::<Vec<_>>();
    let mut conflicts = Vec::new();
    for (index, left) in values.iter().enumerate() {
        for right in values.iter().skip(index + 1) {
            if left.projected_start_time >= right.projected_end_time
                || right.projected_start_time >= left.projected_end_time
            {
                continue;
            }
            if !left.team_id.is_empty() && left.team_id == right.team_id {
                conflicts.push(conflict(
                    "team_overlap",
                    "high",
                    Some(left.team_id.clone()),
                    left.team_name.clone().or_else(|| right.team_name.clone()),
                    vec![left.order_id.clone(), right.order_id.clone()],
                    "场景扰动后同一班组时间窗口重叠",
                    "优先尝试更换班组或交换顺序",
                ));
            }
            if !left.individual_user_id.is_empty() && left.individual_user_id == right.individual_user_id {
                conflicts.push(conflict(
                    "individual_overlap",
                    "high",
                    Some(left.individual_user_id.clone()),
                    left.individual_username
                        .clone()
                        .or_else(|| right.individual_username.clone()),
                    vec![left.order_id.clone(), right.order_id.clone()],
                    "场景扰动后同一人员时间窗口重叠",
                    "调整执行人或改派班组",
                ));
            }
            if !left.stand_id.is_empty() && left.stand_id == right.stand_id {
                conflicts.push(conflict(
                    "stand_overlap",
                    "medium",
                    Some(left.stand_id.clone()),
                    left.stand_code.clone().or_else(|| right.stand_code.clone()),
                    vec![left.order_id.clone(), right.order_id.clone()],
                    "场景扰动后机位保障时间重叠",
                    "复核机位计划并协调窗口",
                ));
            }
            let common_equipment = left
                .equipment_ids
                .intersection(&right.equipment_ids)
                .cloned()
                .collect::<Vec<_>>();
            if !common_equipment.is_empty() {
                conflicts.push(conflict(
                    "equipment_overlap",
                    "high",
                    Some(common_equipment[0].clone()),
                    None,
                    vec![left.order_id.clone(), right.order_id.clone()],
                    "场景扰动后同一设备被重复占用",
                    "优先替换设备或错峰执行",
                ));
            }
        }
    }
    conflicts
}

fn conflict(
    conflict_type: &str,
    severity: &str,
    resource_id: Option<String>,
    resource_name: Option<String>,
    related_dispatch_order_ids: Vec<String>,
    message: &str,
    suggested_action: &str,
) -> Value {
    json!({
        "conflict_type": conflict_type,
        "severity": severity,
        "resource_id": resource_id,
        "resource_name": resource_name,
        "related_dispatch_order_ids": related_dispatch_order_ids,
        "message": message,
        "suggested_action": suggested_action,
        "context": {},
    })
}

fn impact_item(state: &ScenarioOrderState, impact_type: &str, severity: &str, message: &str) -> Value {
    json!({
        "dispatch_order_id": state.order_id,
        "flight_id": state.flight_id,
        "impact_type": impact_type,
        "severity": severity,
        "message": message,
        "original_start_time": state.original_start_time,
        "original_end_time": state.original_end_time,
        "projected_start_time": state.projected_start_time,
        "projected_end_time": state.projected_end_time,
    })
}

fn deduplicate_conflicts(items: Vec<Value>) -> Vec<Value> {
    let mut unique: HashMap<String, Value> = HashMap::new();
    for item in items {
        let key = format!(
            "{}:{}:{}",
            item.get("conflict_type").and_then(Value::as_str).unwrap_or_default(),
            item.get("resource_id").and_then(Value::as_str).unwrap_or_default(),
            item.get("related_dispatch_order_ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(",")
        );
        let replace = unique
            .get(&key)
            .and_then(|current| current.get("severity").and_then(Value::as_str))
            != Some("high")
            && item.get("severity").and_then(Value::as_str) == Some("high");
        if !unique.contains_key(&key) || replace {
            unique.insert(key, item);
        }
    }
    unique.into_values().collect()
}

fn deduplicate_items<F>(items: Vec<Value>, key_fn: F) -> Vec<Value>
where
    F: Fn(&Value) -> String,
{
    let mut unique = HashMap::new();
    for item in items {
        unique.insert(key_fn(&item), item);
    }
    unique.into_values().collect()
}

fn sorted_strings(values: HashSet<String>) -> Vec<String> {
    let mut items = values.into_iter().collect::<Vec<_>>();
    items.sort();
    items
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

fn risk_level(conflict_count: i32, impacted_count: i32, stand_impacted: i32, equipment_impacted: i32) -> &'static str {
    if stand_impacted > 0 || conflict_count >= 5 || impacted_count >= 6 {
        "critical"
    } else if equipment_impacted > 0 || conflict_count >= 3 || impacted_count >= 4 {
        "high"
    } else if conflict_count > 0 || impacted_count > 0 {
        "medium"
    } else {
        "low"
    }
}
