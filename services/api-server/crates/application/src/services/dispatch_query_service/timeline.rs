use std::collections::HashMap;

use chrono::{DateTime, Utc};
use fms_domain::error::DomainError;
use serde_json::{json, Value};

use super::helpers::{
    build_employee_view_items, build_equipment_view_items, build_flight_items, build_flight_summary_items,
    build_status_counts, build_status_orders, build_team_view_items, layout_dynamic_tracks, layout_fixed_lanes,
    normalize_order_for_timeline, resolve_window, serialize_lane, serialize_timeline_item,
};
use super::service::DispatchQueryService;

impl DispatchQueryService {
    pub async fn get_timeline(
        &self,
        view_mode: &str,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        terminal: Option<&str>,
        statuses: &[&str],
        source: Option<&str>,
        department: Option<&str>,
        include_cancelled: bool,
        is_admin: bool,
    ) -> Result<Value, DomainError> {
        let now = Utc::now();
        let (resolved_window_start, resolved_window_end) = resolve_window(window_start, window_end, now);

        let orders = self
            .order_repo
            .find_orders_in_window(
                resolved_window_start,
                resolved_window_end,
                statuses,
                source,
                department,
                terminal,
                include_cancelled,
            )
            .await?;

        let mut normalized_orders = orders.iter().map(normalize_order_for_timeline).collect::<Vec<_>>();
        normalized_orders.sort_by_key(|item| (item.start_time, item.end_time, item.order_id.clone()));

        let normalized_mode = match view_mode {
            "team" | "employee" | "equipment" | "flight" => view_mode,
            _ => "flight",
        };

        let display_items = match normalized_mode {
            "team" => build_team_view_items(&normalized_orders),
            "employee" => build_employee_view_items(&normalized_orders),
            "equipment" => build_equipment_view_items(&normalized_orders),
            _ if is_admin => build_flight_summary_items(&normalized_orders),
            _ => build_flight_items(&normalized_orders),
        };

        let (lanes, layout_items) = if normalized_mode == "flight" {
            layout_dynamic_tracks(display_items)
        } else {
            layout_fixed_lanes(display_items)
        };

        let mut order_focus_map: HashMap<String, String> = HashMap::new();
        let mut flight_focus_map: HashMap<String, String> = HashMap::new();
        for item in &layout_items {
            if let Some(order_id) = &item.order_id {
                order_focus_map
                    .entry(order_id.clone())
                    .or_insert_with(|| item.id.clone());
            }
            if item.is_flight_summary {
                flight_focus_map
                    .entry(item.flight_id.clone())
                    .or_insert_with(|| item.id.clone());
            }
        }

        Ok(json!({
            "view_mode": normalized_mode,
            "is_admin": is_admin,
            "window_start": resolved_window_start.to_rfc3339(),
            "window_end": resolved_window_end.to_rfc3339(),
            "generated_at": now.to_rfc3339(),
            "status_counts": build_status_counts(&normalized_orders),
            "status_orders": build_status_orders(&normalized_orders, &order_focus_map, &flight_focus_map),
            "lanes": lanes.iter().map(serialize_lane).collect::<Vec<_>>(),
            "items": layout_items.iter().map(serialize_timeline_item).collect::<Vec<_>>(),
        }))
    }
}
