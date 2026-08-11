use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::schemas::dispatch_schemas::{
    DispatchReplanAnchorFreeWindow, DispatchReplanAnchorState, DispatchReplanAssignment, DispatchReplanSnapshotOrder,
    DispatchReplanTravelEdge, DispatchReplanUnavailableBlock,
};

use super::super::super::helpers::*;
use super::super::{DispatchFrontendReplanService, ResourceAnchorContext};

impl DispatchFrontendReplanService {
    pub(super) fn ensure_candidate_resource_windows(
        &self,
        context: &mut ResourceAnchorContext,
        resource_type: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        resource_ids: impl IntoIterator<Item = String>,
    ) {
        let mut changed = false;
        for resource_id in resource_ids {
            let resource_id = resource_id.trim();
            if resource_id.is_empty() || context.segments.contains_key(resource_id) {
                continue;
            }
            let window = DispatchReplanAnchorFreeWindow {
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                window_start: Some(window_start),
                window_end: Some(window_end),
                left_anchor_order_id: None,
                left_anchor_stand_id: None,
                right_anchor_order_id: None,
                right_anchor_stand_id: None,
            };
            context.segments.insert(resource_id.to_string(), vec![window.clone()]);
            context.states.push(DispatchReplanAnchorState {
                resource_type: resource_type.to_string(),
                resource_id: resource_id.to_string(),
                anchor_order_id: None,
                location_stand_id: None,
                available_from: Some(window_start),
                free_windows: vec![window],
            });
            changed = true;
        }
        if changed {
            context.states.sort_by(|left, right| {
                left.resource_type
                    .cmp(&right.resource_type)
                    .then(left.resource_id.cmp(&right.resource_id))
            });
        }
    }

    pub(super) fn build_resource_anchor_states(
        &self,
        resource_type: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        fixed_orders: &[DispatchReplanSnapshotOrder],
    ) -> ResourceAnchorContext {
        let mut grouped_orders: HashMap<String, Vec<DispatchReplanSnapshotOrder>> = HashMap::new();
        for order in fixed_orders {
            let Some(current_assignment) = order.current_assignment.as_ref() else {
                continue;
            };
            let resource_ids = if resource_type == "employee" {
                assignment_member_ids(current_assignment)
            } else {
                current_assignment.equipment_ids.clone()
            };
            for resource_id in resource_ids {
                let resource_id = resource_id.trim();
                if resource_id.is_empty() {
                    continue;
                }
                grouped_orders
                    .entry(resource_id.to_string())
                    .or_default()
                    .push(order.clone());
            }
        }

        let mut states = Vec::new();
        let mut segments_by_resource = HashMap::new();
        for (resource_id, mut resource_orders) in grouped_orders {
            resource_orders.sort_by(|left, right| {
                left.effective_start_time
                    .cmp(&right.effective_start_time)
                    .then(left.effective_end_time.cmp(&right.effective_end_time))
                    .then(left.order_id.cmp(&right.order_id))
            });

            let mut latest_anchor_before_window: Option<&DispatchReplanSnapshotOrder> = None;
            let mut current_available_from = window_start;
            let mut segments = Vec::new();

            for order in &resource_orders {
                let (Some(start_time), Some(end_time)) = (order.effective_start_time, order.effective_end_time) else {
                    continue;
                };
                if end_time <= window_start {
                    latest_anchor_before_window = Some(order);
                    current_available_from = current_available_from.max(end_time);
                    continue;
                }
                if start_time <= window_start && window_start < end_time {
                    latest_anchor_before_window = Some(order);
                    current_available_from = current_available_from.max(end_time);
                    continue;
                }
                if current_available_from < start_time {
                    segments.push(DispatchReplanAnchorFreeWindow {
                        resource_type: resource_type.to_string(),
                        resource_id: resource_id.clone(),
                        window_start: Some(current_available_from),
                        window_end: Some(start_time.min(window_end)),
                        left_anchor_order_id: latest_anchor_before_window.map(|item| item.order_id.clone()),
                        left_anchor_stand_id: latest_anchor_before_window.and_then(|item| item.stand_id.clone()),
                        right_anchor_order_id: Some(order.order_id.clone()),
                        right_anchor_stand_id: order.stand_id.clone(),
                    });
                }
                latest_anchor_before_window = Some(order);
                current_available_from = current_available_from.max(end_time);
                if current_available_from >= window_end {
                    break;
                }
            }

            if current_available_from < window_end {
                segments.push(DispatchReplanAnchorFreeWindow {
                    resource_type: resource_type.to_string(),
                    resource_id: resource_id.clone(),
                    window_start: Some(current_available_from),
                    window_end: Some(window_end),
                    left_anchor_order_id: latest_anchor_before_window.map(|item| item.order_id.clone()),
                    left_anchor_stand_id: latest_anchor_before_window.and_then(|item| item.stand_id.clone()),
                    right_anchor_order_id: None,
                    right_anchor_stand_id: None,
                });
            }

            let anchor_available_from = latest_anchor_before_window
                .and_then(|item| item.effective_end_time)
                .map(|value| value.max(window_start))
                .unwrap_or(window_start);
            states.push(DispatchReplanAnchorState {
                resource_type: resource_type.to_string(),
                resource_id: resource_id.clone(),
                anchor_order_id: latest_anchor_before_window.map(|item| item.order_id.clone()),
                location_stand_id: latest_anchor_before_window.and_then(|item| item.stand_id.clone()),
                available_from: Some(anchor_available_from),
                free_windows: segments.clone(),
            });
            segments_by_resource.insert(resource_id, segments);
        }
        states.sort_by(|left, right| {
            left.resource_type
                .cmp(&right.resource_type)
                .then(left.resource_id.cmp(&right.resource_id))
        });
        ResourceAnchorContext {
            states,
            segments: segments_by_resource,
        }
    }

    pub(super) fn build_resource_unavailable_blocks(
        &self,
        resource_type: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        fixed_orders: &[DispatchReplanSnapshotOrder],
    ) -> Vec<DispatchReplanUnavailableBlock> {
        let mut blocks = Vec::new();
        let mut seen = HashSet::new();

        for order in fixed_orders {
            let Some(current_assignment) = order.current_assignment.as_ref() else {
                continue;
            };
            let (Some(start_time), Some(end_time)) = (order.effective_start_time, order.effective_end_time) else {
                continue;
            };
            let clipped_start = start_time.max(window_start);
            let clipped_end = end_time.min(window_end);
            if clipped_end <= clipped_start {
                continue;
            }

            let resource_ids = if resource_type == "employee" {
                assignment_member_ids(current_assignment)
            } else {
                current_assignment.equipment_ids.clone()
            };

            for resource_id in resource_ids {
                let resource_id = resource_id.trim();
                if resource_id.is_empty() {
                    continue;
                }
                let signature = format!(
                    "{resource_type}:{resource_id}:{}:{}:{}",
                    clipped_start.timestamp(),
                    clipped_end.timestamp(),
                    order.order_id
                );
                if !seen.insert(signature) {
                    continue;
                }
                blocks.push(DispatchReplanUnavailableBlock {
                    resource_type: resource_type.to_string(),
                    resource_id: resource_id.to_string(),
                    block_type: "anchor_order".to_string(),
                    start_time: clipped_start,
                    end_time: clipped_end,
                    reason: Some("anchored_order".to_string()),
                    source_id: Some(order.order_id.clone()),
                    metadata: HashMap::from([
                        ("order_id".to_string(), json!(order.order_id)),
                        ("flight_id".to_string(), json!(order.flight_id)),
                        ("order_class".to_string(), json!(order.order_class)),
                        ("lock_level".to_string(), json!(order.lock_level)),
                    ]),
                });
            }
        }

        blocks.sort_by(|left, right| {
            left.resource_type
                .cmp(&right.resource_type)
                .then(left.resource_id.cmp(&right.resource_id))
                .then(left.start_time.cmp(&right.start_time))
                .then(left.end_time.cmp(&right.end_time))
                .then(left.source_id.cmp(&right.source_id))
        });
        blocks
    }

    pub(super) async fn assignment_fits_anchor_windows(
        &self,
        order: &DispatchReplanSnapshotOrder,
        assignment: &DispatchReplanAssignment,
        user_segments: &HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
        equipment_segments: &HashMap<String, Vec<DispatchReplanAnchorFreeWindow>>,
    ) -> bool {
        let (Some(order_start), Some(order_end)) = (order.effective_start_time, order.effective_end_time) else {
            return true;
        };
        for user_id in assignment_member_ids(assignment) {
            if !self
                .resource_has_feasible_window(
                    user_segments.get(&user_id),
                    order_start,
                    order_end,
                    order.stand_id.as_deref(),
                )
                .await
            {
                return false;
            }
        }
        for equipment_id in &assignment.equipment_ids {
            if equipment_id.trim().is_empty() {
                continue;
            }
            if !self
                .resource_has_feasible_window(
                    equipment_segments.get(equipment_id),
                    order_start,
                    order_end,
                    order.stand_id.as_deref(),
                )
                .await
            {
                return false;
            }
        }
        true
    }

    /// Decides whether a resource can host a job spanning `order_start..order_end`.
    ///
    /// Absent or empty segments mean "available": [`build_resource_anchor_states`]
    /// only creates a key for resources that appear in `fixed_orders`, and always
    /// pushes at least one segment when it does. So a missing key is a resource
    /// with no fixed occupancy anywhere in the window — the idlest resource there
    /// is, not an unknown one.
    pub(super) async fn resource_has_feasible_window(
        &self,
        segments: Option<&Vec<DispatchReplanAnchorFreeWindow>>,
        order_start: DateTime<Utc>,
        order_end: DateTime<Utc>,
        stand_id: Option<&str>,
    ) -> bool {
        let Some(segments) = segments else {
            return true;
        };
        if segments.is_empty() {
            return true;
        }
        for segment in segments {
            let (Some(window_start), Some(window_end)) = (segment.window_start, segment.window_end) else {
                continue;
            };
            let left_travel = Duration::minutes(
                self.travel_minutes_between_stands(segment.left_anchor_stand_id.as_deref(), stand_id)
                    .await,
            );
            let right_travel = Duration::minutes(
                self.travel_minutes_between_stands(stand_id, segment.right_anchor_stand_id.as_deref())
                    .await,
            );
            if order_start >= window_start + left_travel && order_end + right_travel <= window_end {
                return true;
            }
        }
        false
    }

    async fn travel_minutes_between_stands(&self, from_stand: Option<&str>, to_stand: Option<&str>) -> i64 {
        let (Some(from_stand), Some(to_stand)) = (from_stand, to_stand) else {
            return 0;
        };
        if from_stand == to_stand {
            return 0;
        }
        if let Some(travel_stats_repo) = self.travel_stats_repo.as_ref() {
            if let Ok(Some(minutes)) = travel_stats_repo.get_average_travel(from_stand, to_stand).await {
                return minutes.round() as i64;
            }
        }
        5
    }

    pub(super) async fn build_travel_edges(
        &self,
        orders: &[DispatchReplanSnapshotOrder],
    ) -> Vec<DispatchReplanTravelEdge> {
        let mut buckets: HashMap<String, Vec<&DispatchReplanSnapshotOrder>> = HashMap::new();
        for order in orders.iter().filter(|item| !item.is_locked) {
            let Some(assignment) = order.current_assignment.as_ref() else {
                continue;
            };
            for resource_key in assignment_resource_keys(assignment) {
                buckets.entry(resource_key).or_default().push(order);
            }
        }
        let mut edges = Vec::new();
        for (resource_key, mut resource_orders) in buckets {
            resource_orders.sort_by_key(|item| item.effective_start_time);
            for pair in resource_orders.windows(2) {
                let travel_minutes = self.estimate_travel_minutes(pair[0], pair[1]).await;
                let (resource_type, resource_id) = parse_resource_key(&resource_key);
                edges.push(DispatchReplanTravelEdge {
                    resource_type,
                    resource_id,
                    from_node: pair[0].order_id.clone(),
                    to_node: pair[1].order_id.clone(),
                    travel_minutes,
                    mode: None,
                });
            }
        }
        edges
    }

    async fn estimate_travel_minutes(
        &self,
        from_order: &DispatchReplanSnapshotOrder,
        to_order: &DispatchReplanSnapshotOrder,
    ) -> i64 {
        let Some(from_stand) = from_order.stand_id.as_deref() else {
            return 0;
        };
        let Some(to_stand) = to_order.stand_id.as_deref() else {
            return 0;
        };
        if from_stand == to_stand {
            return 0;
        }
        if let Some(travel_stats_repo) = self.travel_stats_repo.as_ref() {
            if let Ok(Some(minutes)) = travel_stats_repo.get_average_travel(from_stand, to_stand).await {
                return minutes.round() as i64;
            }
        }
        5
    }
}
