use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::{DispatchOrderRepository, TeamRepository};

use super::error::{repo_err, OntologyActionError};
use super::support::{evidence, required_str};

pub struct DispatchStatusService {
    dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
}

impl DispatchStatusService {
    pub fn new(
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
    ) -> Self {
        Self {
            dispatch_repo,
            team_repo,
        }
    }

    pub async fn get(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let order_id = required_str(args, "dispatch_order_id")?;
        let order = self
            .dispatch_repo
            .find_by_id(order_id, true, None)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("dispatch order {order_id}")))?;

        let team = match &order.team_id {
            Some(team_id) => self.team_repo.find_by_id(team_id, true).await.map_err(repo_err)?,
            None => None,
        };

        let mut conflicts = Vec::new();
        if let Some(reason) = &order.conflict_reason {
            conflicts.push(json!({
                "type": "resource_conflict",
                "description": reason,
            }));
        }

        Ok(json!({
            "dispatch_order": order,
            "team": team,
            "equipment": order.equipment_assignment,
            "conflicts": conflicts,
            "evidence": evidence(None),
        }))
    }
}
