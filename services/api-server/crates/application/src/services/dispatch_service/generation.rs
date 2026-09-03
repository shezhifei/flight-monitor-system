use serde_json::Value;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;

use super::DispatchService;

impl super::EventRuleOrderGateway for DispatchService {
    fn find_adjustable_orders_for_event<'a>(
        &'a self,
        flight_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<DispatchOrder>, DomainError>> + Send + 'a>> {
        Box::pin(async move { self.order.order_repo.find_pending_for_flight(flight_id).await })
    }

    fn save_event_adjusted_order<'a>(
        &'a self,
        order: &'a DispatchOrder,
        details: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async move {
            self.order.order_repo.save(order).await?;
            self.order
                .order_repo
                .append_log(
                    &order.id,
                    "event_rule_adjusted",
                    Some("system:event-rules"),
                    Some(details),
                )
                .await?;
            Ok(())
        })
    }
}
