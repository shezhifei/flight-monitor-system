use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::models::flight::Flight;
use fms_domain::models::flight_leg::FlightTypeCode;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, GeneratedFlightDispatchRequest, PreparedWindowOrder, ReplanExecutionResult, NULL_VALUE};

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
