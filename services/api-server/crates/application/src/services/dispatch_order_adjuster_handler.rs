//! 工单调整器事件处理器 (基于规则配置)
//!
//! 从数据库加载规则并执行动作。

use chrono::{Duration, Utc};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{DispatchOrder, DispatchOrderStatus, ScheduleSource};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info};

use crate::repositories::event_rule_repository::{
    AdjustmentRuleRecord, EventRuleRepository, GenerationRuleRecord, ListAdjustmentRulesParams,
    ListGenerationRulesParams,
};
use crate::schemas::dispatch_schemas::GenerationRuleConfig;
use crate::services::domain_event_subscriber_service::DomainEventEnvelope;

#[derive(Debug, Clone)]
pub struct AdjustmentResult {
    pub modified: bool,
    pub reason: String,
    pub modified_fields: Vec<String>,
    pub rule_id: String,
    pub rule_name: String,
}

impl AdjustmentResult {
    pub fn unchanged(rule_id: &str, rule_name: &str, reason: &str) -> Self {
        Self {
            modified: false,
            reason: reason.to_string(),
            modified_fields: Vec::new(),
            rule_id: rule_id.to_string(),
            rule_name: rule_name.to_string(),
        }
    }

    pub fn modified(rule_id: &str, rule_name: &str, reason: &str, fields: Vec<String>) -> Self {
        Self {
            modified: true,
            reason: reason.to_string(),
            modified_fields: fields,
            rule_id: rule_id.to_string(),
            rule_name: rule_name.to_string(),
        }
    }
}

pub struct GeneratedOrder {
    pub order: DispatchOrder,
    pub rule_id: String,
    pub rule_name: String,
}

pub struct CancellationResult {
    pub cancelled: bool,
    pub reason: String,
    pub rule_id: String,
    pub rule_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdjustmentPreviewAffectedOrder {
    pub order_id: String,
    pub task_type: String,
    pub modified_fields: Vec<String>,
    pub reason: String,
}

pub trait EventRuleOrderGateway: Send + Sync {
    fn find_adjustable_orders_for_event<'a>(
        &'a self,
        flight_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<DispatchOrder>, DomainError>> + Send + 'a>>;

    fn save_event_adjusted_order<'a>(
        &'a self,
        order: &'a DispatchOrder,
        details: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DomainError>> + Send + 'a>>;
}

pub struct DispatchOrderAdjusterHandler {
    repo: Arc<dyn EventRuleRepository + Send + Sync>,
    order_gateway: Option<Arc<dyn EventRuleOrderGateway + Send + Sync>>,
}

impl DispatchOrderAdjusterHandler {
    pub fn new(repo: Arc<dyn EventRuleRepository + Send + Sync>) -> Self {
        Self {
            repo,
            order_gateway: None,
        }
    }
}

impl DispatchOrderAdjusterHandler {
    pub fn with_order_gateway(mut self, order_gateway: Arc<dyn EventRuleOrderGateway + Send + Sync>) -> Self {
        self.order_gateway = Some(order_gateway);
        self
    }

    pub async fn process_event(
        &self,
        event: &DomainEventEnvelope,
    ) -> Result<(Vec<GeneratedOrder>, Vec<AdjustmentResult>), DomainError> {
        let mut generated_orders = Vec::new();
        let mut adjustment_results = Vec::new();

        let adjustment_params = ListAdjustmentRulesParams {
            page: None,
            page_size: None,
            is_enabled: Some(true),
            department_id: None,
        };

        let generation_params = ListGenerationRulesParams {
            page: None,
            page_size: None,
            is_enabled: Some(true),
            department_id: None,
        };

        let adjustment_rules = self.repo.list_adjustment_rules(&adjustment_params).await?;
        let generation_rules = self.repo.list_generation_rules(&generation_params).await?;

        let matching_adjustment_rules: Vec<&AdjustmentRuleRecord> = adjustment_rules
            .iter()
            .filter(|rule| {
                rule.event_patterns.contains(&event.event_type)
                    && self.evaluate_conditions(&rule.conditions, &event.payload)
            })
            .collect();

        if let Some(order_gateway) = self.order_gateway.as_ref() {
            if !matching_adjustment_rules.is_empty() {
                let mut orders = order_gateway
                    .find_adjustable_orders_for_event(&event.aggregate_id)
                    .await?;

                for order in &mut orders {
                    for rule in &matching_adjustment_rules {
                        if !Self::rule_applies_to_order(rule, order) {
                            continue;
                        }

                        let result = Self::apply_adjustment(rule, order)?;
                        if result.modified {
                            order.updated_at = Some(Utc::now());
                            let details = Self::adjustment_log_details(event, &result);
                            order_gateway.save_event_adjusted_order(order, details).await?;
                        }
                        adjustment_results.push(result);
                    }
                }
            }
        }

        for rule in &generation_rules {
            if rule.event_patterns.contains(&event.event_type) {
                if self.evaluate_conditions(&rule.conditions, &event.payload) {
                    if let Some(order) = Self::apply_generation_rule(rule, event)? {
                        generated_orders.push(order);
                    }
                }
            }
        }

        Ok((generated_orders, adjustment_results))
    }

    fn rule_applies_to_order(rule: &AdjustmentRuleRecord, order: &DispatchOrder) -> bool {
        match rule.department_id.as_deref() {
            Some(rule_department_id) => order.department_id.as_deref() == Some(rule_department_id),
            None => true,
        }
    }

    fn adjustment_log_details(event: &DomainEventEnvelope, result: &AdjustmentResult) -> Value {
        json!({
            "event_id": event.event_id,
            "event_type": event.event_type,
            "aggregate_id": event.aggregate_id,
            "source_change_id": event.source_change_id,
            "rule_id": result.rule_id,
            "rule_name": result.rule_name,
            "reason": result.reason,
            "modified_fields": result.modified_fields,
        })
    }

    pub fn evaluate_conditions(&self, conditions: &Option<Value>, payload: &Value) -> bool {
        let Some(conditions) = conditions else {
            return true;
        };

        if let Some(operator) = conditions.get("operator").and_then(|v| v.as_str()) {
            let children = match conditions.get("children") {
                Some(v) => v.as_array().map(|a| a.to_vec()).unwrap_or_default(),
                None => return true,
            };

            match operator {
                "AND" => {
                    for child in &children {
                        if !self.evaluate_single_condition(child, payload) {
                            return false;
                        }
                    }
                    true
                }
                "OR" => {
                    for child in &children {
                        if self.evaluate_single_condition(child, payload) {
                            return true;
                        }
                    }
                    false
                }
                _ => true,
            }
        } else {
            self.evaluate_single_condition(conditions, payload)
        }
    }

    fn evaluate_single_condition(&self, condition: &Value, payload: &Value) -> bool {
        let field = match condition.get("field").and_then(|v| v.as_str()) {
            Some(f) => f,
            None => return true,
        };

        let op = match condition.get("op").and_then(|v| v.as_str()) {
            Some(o) => o,
            None => return true,
        };

        let cond_value = match condition.get("value") {
            Some(v) => v,
            None => return true,
        };

        let payload_value = payload
            .get(field)
            .or_else(|| payload.get("data").and_then(|d| d.get(field)));

        let Some(payload_val) = payload_value else {
            return false;
        };

        match op {
            "eq" => payload_val == cond_value,
            "neq" => payload_val != cond_value,
            "gt" => {
                let p = payload_val.as_f64().unwrap_or(0.0);
                let c = cond_value.as_f64().unwrap_or(0.0);
                p > c
            }
            "gte" => {
                let p = payload_val.as_f64().unwrap_or(0.0);
                let c = cond_value.as_f64().unwrap_or(0.0);
                p >= c
            }
            "lt" => {
                let p = payload_val.as_f64().unwrap_or(0.0);
                let c = cond_value.as_f64().unwrap_or(0.0);
                p < c
            }
            "lte" => {
                let p = payload_val.as_f64().unwrap_or(0.0);
                let c = cond_value.as_f64().unwrap_or(0.0);
                p <= c
            }
            "in" => {
                if let Some(arr) = cond_value.as_array() {
                    arr.contains(payload_val)
                } else {
                    false
                }
            }
            "nin" => {
                if let Some(arr) = cond_value.as_array() {
                    !arr.contains(payload_val)
                } else {
                    true
                }
            }
            "contains" => {
                if let Some(s) = payload_val.as_str() {
                    if let Some(sub) = cond_value.as_str() {
                        s.contains(sub)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => true,
        }
    }

    pub fn preview_affected_order_ids(
        rule: &AdjustmentRuleRecord,
        orders: &[DispatchOrder],
    ) -> Result<Vec<String>, DomainError> {
        Ok(Self::preview_affected_orders(rule, orders)?
            .into_iter()
            .map(|affected_order| affected_order.order_id)
            .collect())
    }

    pub fn preview_affected_orders(
        rule: &AdjustmentRuleRecord,
        orders: &[DispatchOrder],
    ) -> Result<Vec<AdjustmentPreviewAffectedOrder>, DomainError> {
        let mut affected_orders = Vec::new();

        for order in orders {
            if !Self::rule_applies_to_order(rule, order) {
                continue;
            }

            let mut preview_order = order.clone();
            let result = Self::apply_adjustment(rule, &mut preview_order)?;
            if result.modified {
                affected_orders.push(AdjustmentPreviewAffectedOrder {
                    order_id: order.id.clone(),
                    task_type: order.task_type.clone(),
                    modified_fields: result.modified_fields,
                    reason: result.reason,
                });
            }
        }

        Ok(affected_orders)
    }

    pub fn apply_adjustment(
        rule: &AdjustmentRuleRecord,
        order: &mut DispatchOrder,
    ) -> Result<AdjustmentResult, DomainError> {
        let action_type = rule
            .config
            .get("action_type")
            .and_then(Value::as_str)
            .unwrap_or(rule.adjuster_type.as_str());
        let config = Self::adjustment_action_config(&rule.config);

        match action_type {
            "add_crew_slot" => {
                let slot_code = config.get("slot_code").and_then(|v| v.as_str()).unwrap_or("");
                let qualification_code = config.get("qualification_code").and_then(|v| v.as_str()).unwrap_or("");
                let required_count = config.get("required_count").and_then(|v| v.as_i64()).unwrap_or(1) as i32;

                order.crew_requirement_snapshot.push(serde_json::json!({
                    "slot_code": slot_code,
                    "qualification_code": qualification_code,
                    "required_count": required_count,
                    "rule_id": rule.id,
                    "auto_added": true,
                }));

                Ok(AdjustmentResult::modified(
                    &rule.id,
                    &rule.name,
                    &format!("Added crew slot: {}", slot_code),
                    vec![format!("crew_slot:{}", slot_code)],
                ))
            }
            "increase_crew_count" => {
                let slot_code = config.get("slot_code").and_then(|v| v.as_str()).unwrap_or("");
                let delta = config.get("delta").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

                let mut found = false;
                for slot in &mut order.crew_requirement_snapshot {
                    if slot
                        .get("slot_code")
                        .and_then(|v| v.as_str())
                        .map(|s| s == slot_code)
                        .unwrap_or(false)
                    {
                        let current = slot.get("required_count").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        slot["required_count"] = serde_json::json!(current + delta);
                        found = true;
                        break;
                    }
                }

                if found {
                    Ok(AdjustmentResult::modified(
                        &rule.id,
                        &rule.name,
                        &format!("Increased {} by {}", slot_code, delta),
                        vec![format!("crew_count:{}", slot_code)],
                    ))
                } else {
                    Ok(AdjustmentResult::unchanged(&rule.id, &rule.name, "Slot not found"))
                }
            }
            "extend_duration" => {
                let delta = config.get("delta_minutes").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

                if let Some(current_end) = order.planned_end_time {
                    order.planned_end_time = Some(current_end + Duration::minutes(delta as i64));
                    Ok(AdjustmentResult::modified(
                        &rule.id,
                        &rule.name,
                        &format!("Extended duration by {} minutes", delta),
                        vec!["planned_end_time".to_string()],
                    ))
                } else {
                    Ok(AdjustmentResult::unchanged(
                        &rule.id,
                        &rule.name,
                        "No end time to extend",
                    ))
                }
            }
            "advance_publish" => {
                let delta = config.get("delta_minutes").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

                if let Some(current_publish) = order.publish_at {
                    order.publish_at = Some(current_publish - Duration::minutes(delta as i64));
                    Ok(AdjustmentResult::modified(
                        &rule.id,
                        &rule.name,
                        &format!("Advanced publish by {} minutes", delta),
                        vec!["publish_at".to_string()],
                    ))
                } else {
                    Ok(AdjustmentResult::unchanged(
                        &rule.id,
                        &rule.name,
                        "No publish time to advance",
                    ))
                }
            }
            _ => {
                debug!(action_type = %action_type, "Unknown adjustment action type");
                Ok(AdjustmentResult::unchanged(
                    &rule.id,
                    &rule.name,
                    &format!("Unknown action type: {}", action_type),
                ))
            }
        }
    }

    fn adjustment_action_config(config: &Value) -> &Value {
        config.get("config").filter(|value| value.is_object()).unwrap_or(config)
    }

    pub fn apply_generation_rule(
        rule: &GenerationRuleRecord,
        event: &DomainEventEnvelope,
    ) -> Result<Option<GeneratedOrder>, DomainError> {
        let config: GenerationRuleConfig =
            serde_json::from_value(rule.config.clone()).unwrap_or_else(|_| GenerationRuleConfig {
                task_type: rule.name.clone(),
                duration_minutes_from: None,
                fixed_duration_minutes: None,
                crew_requirements: vec![],
                equipment_requirements: vec![],
            });

        let duration = if let Some(duration_from) = &config.duration_minutes_from {
            event.payload.get(duration_from).and_then(|v| v.as_i64()).unwrap_or(0) as i32
        } else {
            config.fixed_duration_minutes.unwrap_or(30)
        };

        let now = Utc::now();
        let start_time = event
            .payload
            .get("scheduled_time")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);

        let mut crew_snapshot: Vec<Value> = Vec::new();
        for req in &config.crew_requirements {
            crew_snapshot.push(serde_json::json!({
                "slot_code": req.slot_code,
                "qualification_code": req.qualification_code,
                "required_count": req.required_count,
                "min_level_code": req.min_level_code,
                "rule_id": rule.id,
            }));
        }

        let mut workflow_context = serde_json::Map::new();
        workflow_context.insert("trigger_event".to_string(), serde_json::json!(event.event_type));
        workflow_context.insert("rule_name".to_string(), serde_json::json!(rule.name));
        workflow_context.insert("generated_at".to_string(), serde_json::json!(now.to_rfc3339()));

        let order = DispatchOrder {
            id: ulid::Ulid::new().to_string(),
            flight_id: event.aggregate_id.clone(),
            task_type: config.task_type.clone(),
            stand_id: event.payload.get("stand_id").and_then(|v| v.as_str()).map(String::from),
            task_type_name: None,
            stand_code: None,
            terminal: event.payload.get("terminal").and_then(|v| v.as_str()).map(String::from),
            assignee_type: fms_domain::models::dispatch::AssigneeType::Team,
            team_id: None,
            team_name: None,
            department: None,
            individual_user_id: None,
            individual_username: None,
            driver_type: None,
            driver_team_id: None,
            driver_user_id: None,
            planned_start_time: Some(start_time),
            planned_end_time: Some(start_time + Duration::minutes(duration as i64)),
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: DispatchOrderStatus::Pending,
            dispatch_type: fms_domain::models::dispatch::DispatchType::Auto,
            dispatched_at: None,
            dispatched_by: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context: serde_json::Value::Object(workflow_context),
            workflow_status: "pending".to_string(),
            source: "event_rule".to_string(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: fms_domain::models::dispatch::DispatchLockLevel::Active,
            publication_state: "prepublished".to_string(),
            source_type: "event_generated".to_string(),
            department_id: rule.department_id.clone(),
            leg_scope: "none".to_string(),
            generation_rule_id: Some(rule.id.clone()),
            generation_rule_version: Some(1),
            generation_anchor_type: None,
            generation_anchor_time: None,
            completion_time_mode: None,
            completion_anchor_type: None,
            completion_anchor_time: None,
            completion_offset_minutes: None,
            completion_warning_lead_minutes: None,
            publish_trigger_mode: None,
            publish_at: None,
            turnaround_pair_key: None,
            turnaround_constraint_mode: None,
            department_rule_version: None,
            crew_requirement_snapshot: crew_snapshot,
            equipment_requirement_snapshot: vec![],
            task_crew: serde_json::Value::Object(Default::default()),
            equipment_assignment: vec![],
            qualification_gap: vec![],
            equipment_gap: vec![],
            availability_reason: None,
            score_breakdown: serde_json::Value::Object(Default::default()),
            conflict_reason: None,
            recommended_assignees: vec![],
            recommendation_score: None,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: None,
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            members: vec![],
            equipment_list: vec![],
        };

        info!(
            rule_id = %rule.id,
            rule_name = %rule.name,
            flight_id = %event.aggregate_id,
            task_type = %config.task_type,
            "Generated order from rule"
        );

        Ok(Some(GeneratedOrder {
            order,
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
        }))
    }

    pub fn get_matching_rules(&self, event_type: &str, event_patterns: &[String]) -> bool {
        event_patterns.contains(&event_type.to_string())
    }
}

impl Default for DispatchOrderAdjusterHandler {
    fn default() -> Self {
        panic!("DispatchOrderAdjusterHandler requires a repository");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::ports::event_rule_repository::{
        DispatchOrderAdjustmentRuleCreate, DispatchOrderAdjustmentRuleUpdate, EventDrivenGenerationRuleCreate,
        EventDrivenGenerationRuleUpdate,
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeEventRuleRepo {
        adjustment_rules: Vec<AdjustmentRuleRecord>,
        generation_rules: Vec<GenerationRuleRecord>,
    }

    #[async_trait::async_trait]
    impl EventRuleRepository for FakeEventRuleRepo {
        async fn list_adjustment_rules(
            &self,
            _params: &ListAdjustmentRulesParams,
        ) -> Result<Vec<AdjustmentRuleRecord>, DomainError> {
            Ok(self.adjustment_rules.clone())
        }

        async fn count_adjustment_rules(&self, _params: &ListAdjustmentRulesParams) -> Result<i64, DomainError> {
            unimplemented!()
        }

        async fn get_adjustment_rule(&self, _id: &str) -> Result<Option<AdjustmentRuleRecord>, DomainError> {
            unimplemented!()
        }

        async fn create_adjustment_rule(
            &self,
            _payload: DispatchOrderAdjustmentRuleCreate,
            _created_by: Option<&str>,
        ) -> Result<AdjustmentRuleRecord, DomainError> {
            unimplemented!()
        }

        async fn update_adjustment_rule(
            &self,
            _id: &str,
            _payload: DispatchOrderAdjustmentRuleUpdate,
        ) -> Result<AdjustmentRuleRecord, DomainError> {
            unimplemented!()
        }

        async fn delete_adjustment_rule(&self, _id: &str) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn set_adjustment_rule_enabled(
            &self,
            _id: &str,
            _enabled: bool,
        ) -> Result<AdjustmentRuleRecord, DomainError> {
            unimplemented!()
        }

        async fn list_generation_rules(
            &self,
            _params: &ListGenerationRulesParams,
        ) -> Result<Vec<GenerationRuleRecord>, DomainError> {
            Ok(self.generation_rules.clone())
        }

        async fn count_generation_rules(&self, _params: &ListGenerationRulesParams) -> Result<i64, DomainError> {
            unimplemented!()
        }

        async fn get_generation_rule(&self, _id: &str) -> Result<Option<GenerationRuleRecord>, DomainError> {
            unimplemented!()
        }

        async fn create_generation_rule(
            &self,
            _payload: EventDrivenGenerationRuleCreate,
            _created_by: Option<&str>,
        ) -> Result<GenerationRuleRecord, DomainError> {
            unimplemented!()
        }

        async fn update_generation_rule(
            &self,
            _id: &str,
            _payload: EventDrivenGenerationRuleUpdate,
        ) -> Result<GenerationRuleRecord, DomainError> {
            unimplemented!()
        }

        async fn delete_generation_rule(&self, _id: &str) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn set_generation_rule_enabled(
            &self,
            _id: &str,
            _enabled: bool,
        ) -> Result<GenerationRuleRecord, DomainError> {
            unimplemented!()
        }
    }

    #[derive(Default)]
    struct FakeOrderGateway {
        orders: Vec<DispatchOrder>,
        saved_orders: Mutex<Vec<DispatchOrder>>,
        logs: Mutex<Vec<serde_json::Value>>,
    }

    impl EventRuleOrderGateway for FakeOrderGateway {
        fn find_adjustable_orders_for_event<'a>(
            &'a self,
            flight_id: &'a str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<DispatchOrder>, DomainError>> + Send + 'a>>
        {
            Box::pin(async move {
                Ok(self
                    .orders
                    .iter()
                    .filter(|order| order.flight_id == flight_id)
                    .cloned()
                    .collect())
            })
        }

        fn save_event_adjusted_order<'a>(
            &'a self,
            order: &'a DispatchOrder,
            details: Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DomainError>> + Send + 'a>> {
            Box::pin(async move {
                self.saved_orders.lock().unwrap().push(order.clone());
                self.logs.lock().unwrap().push(details);
                Ok(())
            })
        }
    }

    fn adjustment_rule() -> AdjustmentRuleRecord {
        let now = Utc::now();
        AdjustmentRuleRecord {
            id: "rule-adjust-1".to_string(),
            adjuster_type: "increase_crew_count".to_string(),
            name: "Delay extra loader".to_string(),
            description: None,
            event_patterns: vec!["flight.status_updated_v2".to_string()],
            priority: 10,
            conditions: Some(json!({
                "operator": "AND",
                "children": [{
                    "field": "delay_minutes",
                    "op": "gte",
                    "value": 30
                }]
            })),
            config: json!({
                "action_type": "increase_crew_count",
                "config": {
                    "slot_code": "loader",
                    "delta": 1
                }
            }),
            is_enabled: true,
            department_id: Some("dept-ground".to_string()),
            department_name: None,
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    fn pending_order() -> DispatchOrder {
        let now = Utc::now();
        DispatchOrder {
            id: "order-1".to_string(),
            flight_id: "flight-1".to_string(),
            task_type: "loading".to_string(),
            stand_id: Some("stand-1".to_string()),
            task_type_name: None,
            stand_code: None,
            terminal: Some("T1".to_string()),
            assignee_type: fms_domain::models::dispatch::AssigneeType::Team,
            team_id: None,
            team_name: None,
            department: None,
            individual_user_id: None,
            individual_username: None,
            driver_type: None,
            driver_team_id: None,
            driver_user_id: None,
            planned_start_time: Some(now),
            planned_end_time: Some(now + Duration::minutes(30)),
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: DispatchOrderStatus::Pending,
            dispatch_type: fms_domain::models::dispatch::DispatchType::Auto,
            dispatched_at: None,
            dispatched_by: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context: serde_json::Value::Object(Default::default()),
            workflow_status: "pending".to_string(),
            source: "generation_rule".to_string(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: fms_domain::models::dispatch::DispatchLockLevel::Active,
            publication_state: "prepublished".to_string(),
            source_type: "scheduled".to_string(),
            department_id: Some("dept-ground".to_string()),
            leg_scope: "none".to_string(),
            generation_rule_id: None,
            generation_rule_version: None,
            generation_anchor_type: None,
            generation_anchor_time: None,
            completion_time_mode: None,
            completion_anchor_type: None,
            completion_anchor_time: None,
            completion_offset_minutes: None,
            completion_warning_lead_minutes: None,
            publish_trigger_mode: None,
            publish_at: None,
            turnaround_pair_key: None,
            turnaround_constraint_mode: None,
            department_rule_version: None,
            crew_requirement_snapshot: vec![json!({
                "slot_code": "loader",
                "qualification_code": "LOAD",
                "required_count": 2
            })],
            equipment_requirement_snapshot: vec![],
            task_crew: serde_json::Value::Object(Default::default()),
            equipment_assignment: vec![],
            qualification_gap: vec![],
            equipment_gap: vec![],
            availability_reason: None,
            score_breakdown: serde_json::Value::Object(Default::default()),
            conflict_reason: None,
            recommended_assignees: vec![],
            recommendation_score: None,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: None,
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: Some(now),
            updated_at: Some(now),
            members: vec![],
            equipment_list: vec![],
        }
    }

    #[tokio::test]
    async fn process_event_applies_matching_adjustment_to_existing_orders() {
        let rule_repo = Arc::new(FakeEventRuleRepo {
            adjustment_rules: vec![adjustment_rule()],
            generation_rules: vec![],
        });
        let order_gateway = Arc::new(FakeOrderGateway {
            orders: vec![pending_order()],
            ..Default::default()
        });
        let handler = DispatchOrderAdjusterHandler::new(rule_repo).with_order_gateway(order_gateway.clone());
        let event = DomainEventEnvelope {
            event_id: "evt-1".to_string(),
            source_change_id: Some("chg-1".to_string()),
            aggregate_type: "flight".to_string(),
            aggregate_id: "flight-1".to_string(),
            event_type: "flight.status_updated_v2".to_string(),
            payload: json!({
                "delay_minutes": 45
            }),
            stream_message_id: "1-0".to_string(),
        };

        let (_generated_orders, adjustment_results) = handler.process_event(&event).await.expect("process event");

        assert_eq!(adjustment_results.len(), 1);
        assert!(adjustment_results[0].modified);

        let saved_orders = order_gateway.saved_orders.lock().unwrap();
        assert_eq!(saved_orders.len(), 1);
        assert_eq!(saved_orders[0].crew_requirement_snapshot[0]["required_count"], json!(3));

        let logs = order_gateway.logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["event_id"], json!("evt-1"));
        assert_eq!(logs[0]["rule_id"], json!("rule-adjust-1"));
        assert_eq!(logs[0]["modified_fields"], json!(["crew_count:loader"]));
    }

    #[test]
    fn preview_adjustment_affected_orders_returns_structured_modified_orders() {
        let rule = adjustment_rule();
        let mut matching_order = pending_order();
        matching_order.id = "order-matching".to_string();

        let mut other_department_order = pending_order();
        other_department_order.id = "order-other-dept".to_string();
        other_department_order.department_id = Some("dept-other".to_string());

        let mut unchanged_order = pending_order();
        unchanged_order.id = "order-unchanged".to_string();
        unchanged_order.crew_requirement_snapshot = vec![json!({
            "slot_code": "supervisor",
            "qualification_code": "SUP",
            "required_count": 1
        })];

        let affected_orders = DispatchOrderAdjusterHandler::preview_affected_orders(
            &rule,
            &[matching_order, other_department_order, unchanged_order],
        )
        .expect("preview affected orders");

        assert_eq!(affected_orders.len(), 1);
        assert_eq!(affected_orders[0].order_id, "order-matching");
        assert_eq!(affected_orders[0].task_type, "loading");
        assert_eq!(
            affected_orders[0].modified_fields,
            vec!["crew_count:loader".to_string()]
        );
        assert_eq!(affected_orders[0].reason, "Increased loader by 1");
    }
}
