//! 派工查询：订单列表、详情、时间线、冲突检测、级联延误预览。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::models::dispatch_collaboration::NotificationReceiptSummary;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use serde_json::{json, Value};

use super::helpers::{
    build_conflict, deduplicate_conflicts, effective_interval, intersecting_member_user_ids, round_to_2,
};
use super::serialization::{dispatch_order_to_value_with_summary, is_workflow_pending_query};

use chrono::Duration;

pub struct DispatchQueryService {
    pub(crate) order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    pub(crate) collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
}

impl DispatchQueryService {
    const ACTIVE_CONFLICT_STATUSES: [&'static str; 3] = ["pending", "assigned", "in_progress"];
    const ANALYTICS_CONFLICT_STATUSES: [&'static str; 4] = ["pending", "assigned", "in_progress", "completed"];

    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    ) -> Self {
        Self {
            order_repo,
            collaboration_repo,
        }
    }

    pub async fn list_orders(
        &self,
        flight_id: Option<&str>,
        team_id: Option<&str>,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let limit = if is_workflow_pending_query(status, source) {
            page_size.clamp(1, 200)
        } else {
            page_size.clamp(1, 100)
        };
        let offset = (page.max(1) - 1) * limit;

        if let Some(flight_id) = flight_id {
            return self
                .order_repo
                .find_by_flight_with_filters(flight_id, status, source, department, limit, offset)
                .await;
        }

        if let Some(team_id) = team_id {
            return self
                .order_repo
                .find_by_team_filtered(team_id, status, source, department, limit, offset)
                .await;
        }

        self.order_repo
            .find_all_filtered(status, source, department, limit, offset)
            .await
    }

    pub async fn get_order(
        &self,
        order_id: &str,
        load_members: bool,
        department: Option<&str>,
    ) -> Result<Option<DispatchOrder>, DomainError> {
        self.order_repo.find_by_id(order_id, load_members, department).await
    }

    pub async fn list_my_orders(&self, user_id: &str, status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
        self.order_repo.find_by_user(user_id, status).await
    }

    pub async fn list_order_records(
        &self,
        flight_id: Option<&str>,
        team_id: Option<&str>,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Value>, DomainError> {
        let orders = self
            .list_orders(flight_id, team_id, status, source, department, page, page_size)
            .await?;
        self.serialize_orders_with_receipt_summaries(&orders).await
    }

    pub async fn get_order_record(
        &self,
        order_id: &str,
        load_members: bool,
        department: Option<&str>,
    ) -> Result<Option<Value>, DomainError> {
        let Some(order) = self.get_order(order_id, load_members, department).await? else {
            return Ok(None);
        };
        let summary = self.collaboration_repo.summarize_receipts_for_order(order_id).await?;
        Ok(Some(dispatch_order_to_value_with_summary(&order, Some(&summary))))
    }

    pub async fn list_my_order_records(&self, user_id: &str, status: Option<&str>) -> Result<Vec<Value>, DomainError> {
        let orders = self.list_my_orders(user_id, status).await?;
        self.serialize_orders_with_receipt_summaries(&orders).await
    }

    async fn serialize_orders_with_receipt_summaries(
        &self,
        orders: &[DispatchOrder],
    ) -> Result<Vec<Value>, DomainError> {
        let mut payload = Vec::with_capacity(orders.len());
        for order in orders {
            let summary = self.collaboration_repo.summarize_receipts_for_order(&order.id).await?;
            payload.push(dispatch_order_to_value_with_summary(order, Some(&summary)));
        }
        Ok(payload)
    }

    pub async fn get_order_timeline(&self, order_id: &str, limit: i64) -> Result<Option<Value>, DomainError> {
        let order = self.order_repo.find_by_id(order_id, false, None).await?;
        if order.is_none() {
            return Ok(None);
        }

        let logs = self.order_repo.list_logs(order_id, limit.max(1)).await?;
        let items = logs
            .into_iter()
            .map(|entry| {
                json!({
                    "id": entry.get("id").cloned().unwrap_or_else(|| json!("")),
                    "action": entry.get("action").cloned().unwrap_or_else(|| json!("unknown")),
                    "actor_id": entry.get("actor_id").unwrap_or(&Value::Null),
                    "actor_username": entry.get("actor_username").unwrap_or(&Value::Null),
                    "details": entry.get("details").cloned().unwrap_or_else(|| json!({})),
                    "created_at": entry.get("created_at").unwrap_or(&Value::Null),
                })
            })
            .collect::<Vec<_>>();

        Ok(Some(json!({
            "dispatch_order_id": order_id,
            "items": items,
            "total": items.len(),
        })))
    }

    pub async fn list_conflicts(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Value>, DomainError> {
        self.list_conflicts_with_statuses(window_start, window_end, limit, &Self::ACTIVE_CONFLICT_STATUSES)
            .await
    }

    pub async fn list_conflicts_for_analytics(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<Value>, DomainError> {
        self.list_conflicts_with_statuses(window_start, window_end, limit, &Self::ANALYTICS_CONFLICT_STATUSES)
            .await
    }

    async fn list_conflicts_with_statuses(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        limit: i64,
        statuses: &[&str],
    ) -> Result<Vec<Value>, DomainError> {
        let orders = self
            .order_repo
            .find_orders_in_window(window_start, window_end, statuses, None, None, None, false)
            .await?;

        let mut conflicts = Vec::new();
        for (index, left) in orders.iter().enumerate() {
            let (left_start, left_end) = effective_interval(left, window_start);
            for right in orders.iter().skip(index + 1) {
                let (right_start, right_end) = effective_interval(right, window_start);
                if left_end < right_start || right_end < left_start {
                    continue;
                }

                if left.team_id.is_some() && left.team_id == right.team_id {
                    conflicts.push(build_conflict(
                        "team_overlap",
                        "high",
                        left.team_id.clone(),
                        left.team_name.clone().or_else(|| right.team_name.clone()),
                        vec![left.id.clone(), right.id.clone()],
                        "班组在同一时间段被重复分配",
                        Some("优先调整后创建工单的开始时间".to_string()),
                        json!({}),
                    ));
                }

                if left.individual_user_id.is_some() && left.individual_user_id == right.individual_user_id {
                    conflicts.push(build_conflict(
                        "individual_overlap",
                        "high",
                        left.individual_user_id.clone(),
                        left.individual_username
                            .clone()
                            .or_else(|| right.individual_username.clone()),
                        vec![left.id.clone(), right.id.clone()],
                        "同一人员在同一时间段被重复分配",
                        Some("更换执行人或错峰执行".to_string()),
                        json!({}),
                    ));
                }

                let overlapping_user_ids = intersecting_member_user_ids(left, right);
                if !overlapping_user_ids.is_empty()
                    && !(left.individual_user_id.is_some() && left.individual_user_id == right.individual_user_id)
                {
                    conflicts.push(build_conflict(
                        "person_time_overlap",
                        "high",
                        Some(overlapping_user_ids[0].clone()),
                        None,
                        vec![left.id.clone(), right.id.clone()],
                        "同一成员在同一时间段参与了多个任务编组",
                        Some("更换成员、重组编组或错峰执行".to_string()),
                        json!({ "matched_user_ids": overlapping_user_ids }),
                    ));
                }

                if left.stand_id.is_some() && left.stand_id == right.stand_id {
                    conflicts.push(build_conflict(
                        "stand_overlap",
                        "medium",
                        left.stand_id.clone(),
                        left.stand_code.clone().or_else(|| right.stand_code.clone()),
                        vec![left.id.clone(), right.id.clone()],
                        "机位时间窗口重叠",
                        Some("核对作业类型依赖关系后再调整窗口".to_string()),
                        json!({}),
                    ));
                }

                if conflicts.len() as i64 >= limit.max(1) {
                    return Ok(deduplicate_conflicts(conflicts, limit.max(1) as usize));
                }
            }
        }

        Ok(deduplicate_conflicts(conflicts, limit.max(1) as usize))
    }

    pub async fn cascade_delay_preview(
        &self,
        flight_id: &str,
        task_type: &str,
        delay_minutes: f64,
        scheduled_departure: Option<DateTime<Utc>>,
    ) -> Result<Value, DomainError> {
        let mut orders = self.order_repo.find_by_flight(flight_id).await?;
        if orders.is_empty() {
            return Ok(json!({
                "delayed_task_type": task_type,
                "delay_minutes": delay_minutes,
                "cascaded_task_types": [],
                "departure_impact_minutes": 0.0,
            }));
        }

        orders.sort_by_key(|order| order.planned_start_time.or(order.created_at).unwrap_or_else(Utc::now));

        let Some(anchor_index) = orders.iter().position(|item| item.task_type == task_type) else {
            return Ok(json!({
                "delayed_task_type": task_type,
                "delay_minutes": delay_minutes,
                "cascaded_task_types": [],
                "departure_impact_minutes": 0.0,
            }));
        };

        let delay_delta = Duration::minutes(delay_minutes.round() as i64);
        let fallback_now = Utc::now();
        let mut cascaded_task_types = Vec::new();

        let anchor = &orders[anchor_index];
        let (anchor_start, anchor_end) = effective_interval(anchor, fallback_now);
        let projected_anchor_end = anchor_end + delay_delta;
        cascaded_task_types.push(json!({
            "task_type": anchor.task_type,
            "task_type_name": anchor.task_type_name.clone().unwrap_or_else(|| anchor.task_type.clone()),
            "original_start": anchor_start.to_rfc3339(),
            "original_end": anchor_end.to_rfc3339(),
            "projected_start": anchor_start.to_rfc3339(),
            "projected_end": projected_anchor_end.to_rfc3339(),
            "shift_minutes": round_to_2(delay_minutes),
        }));

        let mut previous_projected_end = projected_anchor_end;
        for order in orders.iter().skip(anchor_index + 1) {
            let (start, end) = effective_interval(order, fallback_now);
            let duration = end - start;
            let (projected_start, projected_end, shift_minutes) = if start < previous_projected_end {
                let shift = previous_projected_end - start;
                let projected_start = previous_projected_end;
                let projected_end = projected_start + duration;
                (
                    projected_start,
                    projected_end,
                    round_to_2(shift.num_seconds() as f64 / 60.0),
                )
            } else {
                (start, end, 0.0)
            };

            cascaded_task_types.push(json!({
                "task_type": order.task_type,
                "task_type_name": order.task_type_name.clone().unwrap_or_else(|| order.task_type.clone()),
                "original_start": start.to_rfc3339(),
                "original_end": end.to_rfc3339(),
                "projected_start": projected_start.to_rfc3339(),
                "projected_end": projected_end.to_rfc3339(),
                "shift_minutes": shift_minutes,
            }));

            previous_projected_end = if shift_minutes > 0.0 { projected_end } else { end };
        }

        let departure_impact_minutes = scheduled_departure
            .filter(|scheduled_departure| previous_projected_end > *scheduled_departure)
            .map(|scheduled_departure| {
                round_to_2((previous_projected_end - scheduled_departure).num_seconds() as f64 / 60.0)
            })
            .unwrap_or(0.0);

        Ok(json!({
            "delayed_task_type": task_type,
            "delay_minutes": delay_minutes,
            "cascaded_task_types": cascaded_task_types,
            "departure_impact_minutes": departure_impact_minutes,
        }))
    }
}
