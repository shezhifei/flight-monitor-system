//! Generates a dispatch reassignment proposal (`reassign`).

use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::{DispatchOrderRepository, TeamRepository};

use super::error::{repo_err, OntologyActionError};
use super::support::{arg_str, constraint, required_str, suggestion_envelope, CANDIDATE_TEAMS_SCANNED};

pub struct DispatchReplanAdvisorService {
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
}

impl DispatchReplanAdvisorService {
    pub fn new(
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
    ) -> Self {
        Self {
            dispatch_repo,
            team_repo,
        }
    }

    pub async fn suggest(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let order_id = required_str(args, "dispatch_order_id")?;
        let reason = required_str(args, "reason")?;
        let order = self
            .dispatch_repo
            .find_by_id(order_id, false, None)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("dispatch order {order_id}")))?;

        let target_team = match arg_str(args, "target_team_id") {
            Some(team_id) => self
                .team_repo
                .find_by_id(team_id, false)
                .await
                .map_err(repo_err)?
                .ok_or_else(|| OntologyActionError::NotFound(format!("team {team_id}")))?,
            None => {
                let available = self
                    .team_repo
                    .find_available_for_dispatch(None, order.terminal.as_deref())
                    .await
                    .map_err(repo_err)?;
                let mut teams = available
                    .into_iter()
                    .filter(|team| team.id != order.team_id.as_deref().unwrap_or(""))
                    .collect::<Vec<_>>();
                teams.truncate(CANDIDATE_TEAMS_SCANNED as usize);
                teams
                    .into_iter()
                    .next()
                    .ok_or_else(|| OntologyActionError::NotFound("no available team for replan".to_string()))?
            }
        };

        let conflicts = match (order.planned_start_time, order.planned_end_time) {
            (Some(start), Some(end)) if end > start => self
                .dispatch_repo
                .find_overlapping_orders(start, end, Some(&target_team.id), None, None, Some(&order.id))
                .await
                .map_err(repo_err)?,
            _ => Vec::new(),
        };

        let mut constraint_results = vec![
            constraint("target_team_exists", true, "error", None),
            constraint("target_team_active", target_team.is_active, "error", None),
            constraint(
                "target_team_different",
                target_team.id != order.team_id.as_deref().unwrap_or(""),
                "warning",
                None,
            ),
        ];
        if conflicts.is_empty() {
            constraint_results.push(constraint("no_window_conflict", true, "warning", None));
        } else {
            constraint_results.push(constraint(
                "no_window_conflict",
                false,
                "warning",
                Some(&format!("{} conflicting order(s) for target team", conflicts.len())),
            ));
        }

        let score_before = 0.5f64;
        let score_after = if conflicts.is_empty() && target_team.is_active {
            0.9
        } else {
            0.55
        };
        let confidence = if conflicts.is_empty() { 0.85 } else { 0.5 };

        Ok(suggestion_envelope(
            "DispatchOrder",
            order_id,
            "reassign",
            json!({ "assignee_id": target_team.id, "reason": reason }),
            "high",
            constraint_results,
            json!({ "team_id": order.team_id, "status": order.status.as_ref() }),
            json!({ "team_id": target_team.id, "team_name": target_team.name }),
            confidence,
            &format!("replan order {} to team {}: {}", order_id, target_team.id, reason),
            json!({
                "resource_changes": [{
                    "kind": "team",
                    "from": order.team_id,
                    "to": target_team.id,
                }],
                "score_before": score_before,
                "score_after": score_after,
                "conflicts": conflicts.iter().map(|c| json!({
                    "order_id": c.id,
                    "task_type": c.task_type,
                })).collect::<Vec<_>>(),
            }),
        ))
    }
}
