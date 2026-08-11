//! 事件规则处理器
//!
//! 处理航班事件并执行事件驱动的派工规则。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::ports::event_rule_repository::EventRuleRepository;
use serde_json::json;
use tracing::{debug, info, warn};

use crate::services::dispatch_order_adjuster_handler::DispatchOrderAdjusterHandler;
use crate::services::dispatch_service::DispatchService;
use crate::services::domain_event_subscriber_service::DomainEventEnvelope;

type EventRuleHandlerFuture = Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send>>;

pub struct EventDrivenRuleHandler {
    event_rule_repo: Arc<dyn EventRuleRepository + Send + Sync>,
    dispatch_service: Arc<DispatchService>,
}

impl EventDrivenRuleHandler {
    pub fn new(
        event_rule_repo: Arc<dyn EventRuleRepository + Send + Sync>,
        dispatch_service: Arc<DispatchService>,
    ) -> Self {
        Self {
            event_rule_repo,
            dispatch_service,
        }
    }

    pub fn can_handle(event_type: &str) -> bool {
        matches!(
            event_type,
            "flight.created_v2"
                | "flight.status_updated_v2"
                | "flight.resource_updated_v2"
                | "flight.leg_upserted_v2"
                | "flight.remarks_updated_v2"
        )
    }

    pub async fn handle(&self, envelope: DomainEventEnvelope) -> Result<(), DomainError> {
        if !Self::can_handle(&envelope.event_type) {
            return Ok(());
        }

        debug!(
            event_id = %envelope.event_id,
            event_type = %envelope.event_type,
            aggregate_id = %envelope.aggregate_id,
            "Processing event with event-driven rules"
        );

        let handler = DispatchOrderAdjusterHandler::new(self.event_rule_repo.clone())
            .with_order_gateway(self.dispatch_service.clone());

        match handler.process_event(&envelope).await {
            Ok((generated_orders, adjustment_results)) => {
                for order in &generated_orders {
                    let details = json!({
                        "event_id": envelope.event_id,
                        "event_type": envelope.event_type,
                        "aggregate_id": envelope.aggregate_id,
                        "source_change_id": envelope.source_change_id,
                        "rule_id": order.rule_id,
                        "rule_name": order.rule_name,
                    });
                    let saved = self
                        .dispatch_service
                        .save_event_generated_order_once(&order.order, details)
                        .await?;
                    if !saved {
                        debug!(
                            rule_id = %order.rule_id,
                            rule_name = %order.rule_name,
                            flight_id = %envelope.aggregate_id,
                            task_type = %order.order.task_type,
                            "Skipped duplicate event-generated order"
                        );
                        continue;
                    }

                    info!(
                        rule_id = %order.rule_id,
                        rule_name = %order.rule_name,
                        flight_id = %envelope.aggregate_id,
                        order_id = %order.order.id,
                        task_type = %order.order.task_type,
                        "Generated order from event rule"
                    );
                }

                for result in &adjustment_results {
                    if result.modified {
                        debug!(
                            rule_id = %result.rule_id,
                            rule_name = %result.rule_name,
                            reason = %result.reason,
                            fields = ?result.modified_fields,
                            "Applied adjustment rule"
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    event_id = %envelope.event_id,
                    error = %e,
                    "Failed to process event rules"
                );
            }
        }

        Ok(())
    }
}

impl EventRuleHandler for EventDrivenRuleHandler {
    fn can_handle(&self, event_type: &str) -> bool {
        Self::can_handle(event_type)
    }

    fn handle(&self, envelope: DomainEventEnvelope) -> EventRuleHandlerFuture {
        let event_rule_repo = self.event_rule_repo.clone();
        let dispatch_service = self.dispatch_service.clone();
        Box::pin(async move {
            let handler = Self::new(event_rule_repo, dispatch_service);
            handler.handle(envelope).await
        })
    }
}

pub trait EventRuleHandler: Send + Sync {
    fn can_handle(&self, event_type: &str) -> bool;
    fn handle(&self, envelope: DomainEventEnvelope) -> EventRuleHandlerFuture;
}
