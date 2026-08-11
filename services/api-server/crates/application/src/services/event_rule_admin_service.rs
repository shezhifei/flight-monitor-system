use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrder;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::event_rule_repository::{
    AdjustmentRuleRecord, DispatchOrderAdjustmentRuleCreate, DispatchOrderAdjustmentRuleUpdate,
    EventDrivenGenerationRuleCreate, EventDrivenGenerationRuleUpdate, EventRuleRepository, GenerationRuleRecord,
    ListAdjustmentRulesParams, ListGenerationRulesParams,
};

pub struct EventRuleAdminService<R, O>
where
    R: EventRuleRepository + Send + Sync + 'static + ?Sized,
    O: DispatchOrderRepository + Send + Sync + 'static + ?Sized,
{
    rule_repo: Arc<R>,
    dispatch_order_repo: Arc<O>,
}

impl<R, O> EventRuleAdminService<R, O>
where
    R: EventRuleRepository + Send + Sync + 'static + ?Sized,
    O: DispatchOrderRepository + Send + Sync + 'static + ?Sized,
{
    pub fn new(rule_repo: Arc<R>, dispatch_order_repo: Arc<O>) -> Self {
        Self {
            rule_repo,
            dispatch_order_repo,
        }
    }

    pub async fn list_adjustment_rules(
        &self,
        params: ListAdjustmentRulesParams,
    ) -> Result<(Vec<AdjustmentRuleRecord>, i64), DomainError> {
        let records = self.rule_repo.list_adjustment_rules(&params).await?;
        let total = self.rule_repo.count_adjustment_rules(&params).await?;
        Ok((records, total))
    }

    pub async fn get_adjustment_rule(&self, id: &str) -> Result<AdjustmentRuleRecord, DomainError> {
        self.rule_repo
            .get_adjustment_rule(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "adjustment_rule",
                id: id.to_string(),
            })
    }

    pub async fn create_adjustment_rule(
        &self,
        payload: DispatchOrderAdjustmentRuleCreate,
        created_by: Option<&str>,
    ) -> Result<AdjustmentRuleRecord, DomainError> {
        self.rule_repo.create_adjustment_rule(payload, created_by).await
    }

    pub async fn update_adjustment_rule(
        &self,
        id: &str,
        payload: DispatchOrderAdjustmentRuleUpdate,
    ) -> Result<AdjustmentRuleRecord, DomainError> {
        self.rule_repo.update_adjustment_rule(id, payload).await
    }

    pub async fn delete_adjustment_rule(&self, id: &str) -> Result<(), DomainError> {
        self.rule_repo.delete_adjustment_rule(id).await
    }

    pub async fn enable_adjustment_rule(&self, id: &str) -> Result<AdjustmentRuleRecord, DomainError> {
        self.rule_repo.set_adjustment_rule_enabled(id, true).await
    }

    pub async fn disable_adjustment_rule(&self, id: &str) -> Result<AdjustmentRuleRecord, DomainError> {
        self.rule_repo.set_adjustment_rule_enabled(id, false).await
    }

    pub async fn list_generation_rules(
        &self,
        params: ListGenerationRulesParams,
    ) -> Result<(Vec<GenerationRuleRecord>, i64), DomainError> {
        let records = self.rule_repo.list_generation_rules(&params).await?;
        let total = self.rule_repo.count_generation_rules(&params).await?;
        Ok((records, total))
    }

    pub async fn get_generation_rule(&self, id: &str) -> Result<GenerationRuleRecord, DomainError> {
        self.rule_repo
            .get_generation_rule(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "generation_rule",
                id: id.to_string(),
            })
    }

    pub async fn create_generation_rule(
        &self,
        payload: EventDrivenGenerationRuleCreate,
        created_by: Option<&str>,
    ) -> Result<GenerationRuleRecord, DomainError> {
        self.rule_repo.create_generation_rule(payload, created_by).await
    }

    pub async fn update_generation_rule(
        &self,
        id: &str,
        payload: EventDrivenGenerationRuleUpdate,
    ) -> Result<GenerationRuleRecord, DomainError> {
        self.rule_repo.update_generation_rule(id, payload).await
    }

    pub async fn delete_generation_rule(&self, id: &str) -> Result<(), DomainError> {
        self.rule_repo.delete_generation_rule(id).await
    }

    pub async fn enable_generation_rule(&self, id: &str) -> Result<GenerationRuleRecord, DomainError> {
        self.rule_repo.set_generation_rule_enabled(id, true).await
    }

    pub async fn disable_generation_rule(&self, id: &str) -> Result<GenerationRuleRecord, DomainError> {
        self.rule_repo.set_generation_rule_enabled(id, false).await
    }

    pub async fn preview_inputs(&self, flight_id: &str) -> Result<RulePreviewInputs, DomainError> {
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

        let adjustment_rules = self.rule_repo.list_adjustment_rules(&adjustment_params).await?;
        let generation_rules = self.rule_repo.list_generation_rules(&generation_params).await?;
        let pending_orders = self.dispatch_order_repo.find_pending_for_flight(flight_id).await?;

        Ok(RulePreviewInputs {
            adjustment_rules,
            generation_rules,
            pending_orders,
        })
    }
}

pub struct RulePreviewInputs {
    pub adjustment_rules: Vec<AdjustmentRuleRecord>,
    pub generation_rules: Vec<GenerationRuleRecord>,
    pub pending_orders: Vec<DispatchOrder>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use fms_domain::ports::dispatch_repository::CreateDispatchOrderCommand;
    use fms_domain::ports::event_rule_repository::{
        AdjustmentActionType, EventDrivenGenerationRuleCreate, GenerationRuleConfig,
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRuleRepo {
        adjustment_rules: Mutex<Vec<AdjustmentRuleRecord>>,
        generation_rules: Mutex<Vec<GenerationRuleRecord>>,
        last_adjustment_params: Mutex<Option<ListAdjustmentRulesParams>>,
        last_generation_params: Mutex<Option<ListGenerationRulesParams>>,
    }

    #[async_trait]
    impl EventRuleRepository for FakeRuleRepo {
        async fn list_adjustment_rules(
            &self,
            params: &ListAdjustmentRulesParams,
        ) -> Result<Vec<AdjustmentRuleRecord>, DomainError> {
            *self.last_adjustment_params.lock().unwrap() = Some(params.clone());
            Ok(self.adjustment_rules.lock().unwrap().clone())
        }

        async fn count_adjustment_rules(&self, _params: &ListAdjustmentRulesParams) -> Result<i64, DomainError> {
            Ok(self.adjustment_rules.lock().unwrap().len() as i64)
        }

        async fn get_adjustment_rule(&self, id: &str) -> Result<Option<AdjustmentRuleRecord>, DomainError> {
            Ok(self
                .adjustment_rules
                .lock()
                .unwrap()
                .iter()
                .find(|rule| rule.id == id)
                .cloned())
        }

        async fn create_adjustment_rule(
            &self,
            payload: DispatchOrderAdjustmentRuleCreate,
            created_by: Option<&str>,
        ) -> Result<AdjustmentRuleRecord, DomainError> {
            let mut record = adjustment_rule_record("created", &payload.name);
            record.created_by = created_by.map(str::to_string);
            self.adjustment_rules.lock().unwrap().push(record.clone());
            Ok(record)
        }

        async fn update_adjustment_rule(
            &self,
            id: &str,
            _payload: DispatchOrderAdjustmentRuleUpdate,
        ) -> Result<AdjustmentRuleRecord, DomainError> {
            self.get_adjustment_rule(id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "adjustment_rule",
                    id: id.to_string(),
                })
        }

        async fn delete_adjustment_rule(&self, _id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn set_adjustment_rule_enabled(
            &self,
            id: &str,
            enabled: bool,
        ) -> Result<AdjustmentRuleRecord, DomainError> {
            let mut record = self
                .get_adjustment_rule(id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "adjustment_rule",
                    id: id.to_string(),
                })?;
            record.is_enabled = enabled;
            Ok(record)
        }

        async fn list_generation_rules(
            &self,
            params: &ListGenerationRulesParams,
        ) -> Result<Vec<GenerationRuleRecord>, DomainError> {
            *self.last_generation_params.lock().unwrap() = Some(params.clone());
            Ok(self.generation_rules.lock().unwrap().clone())
        }

        async fn count_generation_rules(&self, _params: &ListGenerationRulesParams) -> Result<i64, DomainError> {
            Ok(self.generation_rules.lock().unwrap().len() as i64)
        }

        async fn get_generation_rule(&self, id: &str) -> Result<Option<GenerationRuleRecord>, DomainError> {
            Ok(self
                .generation_rules
                .lock()
                .unwrap()
                .iter()
                .find(|rule| rule.id == id)
                .cloned())
        }

        async fn create_generation_rule(
            &self,
            payload: EventDrivenGenerationRuleCreate,
            created_by: Option<&str>,
        ) -> Result<GenerationRuleRecord, DomainError> {
            let mut record = generation_rule_record("created", &payload.name);
            record.created_by = created_by.map(str::to_string);
            self.generation_rules.lock().unwrap().push(record.clone());
            Ok(record)
        }

        async fn update_generation_rule(
            &self,
            id: &str,
            _payload: EventDrivenGenerationRuleUpdate,
        ) -> Result<GenerationRuleRecord, DomainError> {
            self.get_generation_rule(id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "generation_rule",
                    id: id.to_string(),
                })
        }

        async fn delete_generation_rule(&self, _id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn set_generation_rule_enabled(
            &self,
            id: &str,
            enabled: bool,
        ) -> Result<GenerationRuleRecord, DomainError> {
            let mut record = self
                .get_generation_rule(id)
                .await?
                .ok_or_else(|| DomainError::NotFound {
                    entity_type: "generation_rule",
                    id: id.to_string(),
                })?;
            record.is_enabled = enabled;
            Ok(record)
        }
    }

    #[derive(Default)]
    struct FakeOrderRepo {
        pending_orders: Mutex<Vec<DispatchOrder>>,
        last_pending_flight_id: Mutex<Option<String>>,
    }

    #[async_trait]
    impl DispatchOrderRepository for FakeOrderRepo {
        async fn save(&self, _order: &DispatchOrder) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn create_order_atomic(&self, _command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn save_orders_atomic(&self, _commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn find_by_id(
            &self,
            _id: &str,
            _load_members: bool,
            _department: Option<&str>,
        ) -> Result<Option<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_by_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_by_flight_with_filters(
            &self,
            _flight_id: &str,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_by_team(
            &self,
            _team_id: &str,
            _status: Option<&str>,
            _start_date: Option<chrono::DateTime<Utc>>,
            _end_date: Option<chrono::DateTime<Utc>>,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_by_team_filtered(
            &self,
            _team_id: &str,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_by_user(&self, _user_id: &str, _status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_all(
            &self,
            _status: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_all_filtered(
            &self,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_orders_in_window(
            &self,
            _window_start: chrono::DateTime<Utc>,
            _window_end: chrono::DateTime<Utc>,
            _statuses: &[&str],
            _source: Option<&str>,
            _department: Option<&str>,
            _terminal: Option<&str>,
            _include_cancelled: bool,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_overlapping_orders(
            &self,
            _window_start: chrono::DateTime<Utc>,
            _window_end: chrono::DateTime<Utc>,
            _team_id: Option<&str>,
            _individual_user_id: Option<&str>,
            _stand_id: Option<&str>,
            _exclude_order_id: Option<&str>,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn find_equipment_conflicts(
            &self,
            _equipment_ids: &[String],
            _window_start: chrono::DateTime<Utc>,
            _window_end: chrono::DateTime<Utc>,
            _exclude_order_id: Option<&str>,
        ) -> Result<Vec<serde_json::Value>, DomainError> {
            unimplemented!()
        }

        async fn list_logs(
            &self,
            _dispatch_order_id: &str,
            _limit: i64,
        ) -> Result<Vec<serde_json::Value>, DomainError> {
            unimplemented!()
        }

        async fn find_pending_for_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
            *self.last_pending_flight_id.lock().unwrap() = Some(flight_id.to_string());
            Ok(self.pending_orders.lock().unwrap().clone())
        }

        async fn find_publishable_orders(
            &self,
            _as_of: chrono::DateTime<Utc>,
            _limit: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!()
        }

        async fn update_status(
            &self,
            _id: &str,
            _status: &str,
            _actor_id: Option<&str>,
            _enforce_actor_assignment: bool,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn start_order(
            &self,
            _id: &str,
            _actual_start: chrono::DateTime<Utc>,
            _actor_id: &str,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn complete_order(
            &self,
            _id: &str,
            _actual_end: chrono::DateTime<Utc>,
            _actor_id: &str,
            _notes: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn append_log(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _details: Option<serde_json::Value>,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }

        async fn append_log_once(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _details: serde_json::Value,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn has_logged_action(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _client_action_id: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn report_estimated_completion(
            &self,
            _id: &str,
            _estimated_time: chrono::DateTime<Utc>,
            _actor_id: &str,
            _note: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn update_planned_times(
            &self,
            _id: &str,
            _planned_start: chrono::DateTime<Utc>,
            _planned_end: chrono::DateTime<Utc>,
        ) -> Result<bool, DomainError> {
            unimplemented!()
        }

        async fn replace_order_equipment_assignments(
            &self,
            _id: &str,
            _equipment_ids: &[String],
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    fn service(
        rule_repo: Arc<FakeRuleRepo>,
        order_repo: Arc<FakeOrderRepo>,
    ) -> EventRuleAdminService<FakeRuleRepo, FakeOrderRepo> {
        EventRuleAdminService::new(rule_repo, order_repo)
    }

    fn adjustment_rule_record(id: &str, name: &str) -> AdjustmentRuleRecord {
        let now = Utc::now();
        AdjustmentRuleRecord {
            id: id.to_string(),
            adjuster_type: "add_crew_slot".to_string(),
            name: name.to_string(),
            description: None,
            event_patterns: vec!["flight.updated".to_string()],
            priority: 100,
            conditions: None,
            config: json!({"slot_code": "driver", "required_count": 1}),
            is_enabled: true,
            department_id: None,
            department_name: None,
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    fn generation_rule_record(id: &str, name: &str) -> GenerationRuleRecord {
        let now = Utc::now();
        GenerationRuleRecord {
            id: id.to_string(),
            generator_type: "event_generated".to_string(),
            name: name.to_string(),
            description: None,
            event_patterns: vec!["flight.updated".to_string()],
            priority: 100,
            conditions: None,
            config: json!({
                "task_type": "cleaning",
                "duration_minutes_from": null,
                "fixed_duration_minutes": 30,
                "crew_requirements": [],
                "equipment_requirements": []
            }),
            is_enabled: true,
            department_id: None,
            department_name: None,
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    fn dispatch_order(id: &str, flight_id: &str) -> DispatchOrder {
        serde_json::from_value(json!({
            "id": id,
            "flight_id": flight_id,
            "task_type": "cleaning"
        }))
        .expect("valid dispatch order fixture")
    }

    #[tokio::test]
    async fn list_adjustment_rules_returns_records_and_total_from_repository() {
        let rule_repo = Arc::new(FakeRuleRepo::default());
        rule_repo
            .adjustment_rules
            .lock()
            .unwrap()
            .push(adjustment_rule_record("rule-1", "Rule 1"));
        let svc = service(rule_repo.clone(), Arc::new(FakeOrderRepo::default()));

        let (records, total) = svc
            .list_adjustment_rules(ListAdjustmentRulesParams {
                page: Some(2),
                page_size: Some(10),
                is_enabled: Some(true),
                department_id: Some("dept-1".to_string()),
            })
            .await
            .expect("list adjustment rules");

        assert_eq!(records.len(), 1);
        assert_eq!(total, 1);
        let params = rule_repo.last_adjustment_params.lock().unwrap().clone().unwrap();
        assert_eq!(params.page, Some(2));
        assert_eq!(params.page_size, Some(10));
        assert_eq!(params.is_enabled, Some(true));
        assert_eq!(params.department_id.as_deref(), Some("dept-1"));
    }

    #[tokio::test]
    async fn get_adjustment_rule_maps_missing_record_to_domain_not_found() {
        let svc = service(Arc::new(FakeRuleRepo::default()), Arc::new(FakeOrderRepo::default()));

        let err = svc.get_adjustment_rule("missing").await.expect_err("missing rule");

        assert!(matches!(
            err,
            DomainError::NotFound {
                entity_type: "adjustment_rule",
                id
            } if id == "missing"
        ));
    }

    #[tokio::test]
    async fn get_generation_rule_maps_missing_record_to_domain_not_found() {
        let svc = service(Arc::new(FakeRuleRepo::default()), Arc::new(FakeOrderRepo::default()));

        let err = svc.get_generation_rule("missing").await.expect_err("missing rule");

        assert!(matches!(
            err,
            DomainError::NotFound {
                entity_type: "generation_rule",
                id
            } if id == "missing"
        ));
    }

    #[tokio::test]
    async fn create_adjustment_rule_delegates_payload_and_created_by() {
        let svc = service(Arc::new(FakeRuleRepo::default()), Arc::new(FakeOrderRepo::default()));

        let record = svc
            .create_adjustment_rule(
                DispatchOrderAdjustmentRuleCreate {
                    adjuster_type: AdjustmentActionType::AddCrewSlot,
                    name: "New rule".to_string(),
                    description: None,
                    event_patterns: vec!["flight.updated".to_string()],
                    priority: 100,
                    conditions: None,
                    config: json!({"slot_code": "driver", "required_count": 1}),
                    is_enabled: true,
                    department_id: None,
                },
                Some("tester"),
            )
            .await
            .expect("create adjustment rule");

        assert_eq!(record.name, "New rule");
        assert_eq!(record.created_by.as_deref(), Some("tester"));
    }

    #[tokio::test]
    async fn create_generation_rule_delegates_payload_and_created_by() {
        let svc = service(Arc::new(FakeRuleRepo::default()), Arc::new(FakeOrderRepo::default()));

        let record = svc
            .create_generation_rule(
                EventDrivenGenerationRuleCreate {
                    generator_type: "event_generated".to_string(),
                    name: "New generation".to_string(),
                    description: None,
                    event_patterns: vec!["flight.updated".to_string()],
                    priority: 100,
                    conditions: None,
                    config: GenerationRuleConfig {
                        task_type: "cleaning".to_string(),
                        duration_minutes_from: None,
                        fixed_duration_minutes: Some(30),
                        crew_requirements: vec![],
                        equipment_requirements: vec![],
                    },
                    is_enabled: true,
                    department_id: None,
                },
                Some("tester"),
            )
            .await
            .expect("create generation rule");

        assert_eq!(record.name, "New generation");
        assert_eq!(record.created_by.as_deref(), Some("tester"));
    }

    #[tokio::test]
    async fn preview_inputs_loads_enabled_rules_and_pending_orders_for_flight() {
        let rule_repo = Arc::new(FakeRuleRepo::default());
        rule_repo
            .adjustment_rules
            .lock()
            .unwrap()
            .push(adjustment_rule_record("adjust-1", "Adjust"));
        rule_repo
            .generation_rules
            .lock()
            .unwrap()
            .push(generation_rule_record("generate-1", "Generate"));
        let order_repo = Arc::new(FakeOrderRepo::default());
        order_repo
            .pending_orders
            .lock()
            .unwrap()
            .push(dispatch_order("order-1", "flight-1"));
        let svc = service(rule_repo.clone(), order_repo.clone());

        let inputs = svc.preview_inputs("flight-1").await.expect("preview inputs");

        assert_eq!(inputs.adjustment_rules.len(), 1);
        assert_eq!(inputs.generation_rules.len(), 1);
        assert_eq!(inputs.pending_orders.len(), 1);
        assert_eq!(
            order_repo.last_pending_flight_id.lock().unwrap().as_deref(),
            Some("flight-1")
        );
        assert_eq!(
            rule_repo
                .last_adjustment_params
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|params| params.is_enabled),
            Some(true)
        );
        assert_eq!(
            rule_repo
                .last_generation_params
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|params| params.is_enabled),
            Some(true)
        );
    }
}
