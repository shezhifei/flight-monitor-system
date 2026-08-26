use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::ports::dispatch_repository::TeamRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{evidence, required_str};

/// `Team.get_context`：读取班组的在岗名册与成员列表。只读、不进执行器；
/// 由 `dispatch_read_action` 经 `read_action_permission` 调用。
pub struct TeamContextService {
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
}

impl TeamContextService {
    pub fn new(team_repo: Arc<dyn TeamRepository + Send + Sync>) -> Self {
        Self { team_repo }
    }

    pub async fn get(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let team_id = required_str(args, "team_id")?;

        let team = self
            .team_repo
            .find_by_id(team_id, true)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("team {team_id}")))?;

        // 面向上本体的只读投影：班组名册是配置/展示数据，暴露脱敏行。
        let profile = json!({
            "team_id": team.id,
            "name": team.name,
            "code": team.code,
            "leader_id": team.leader_id,
            "current_status": team.current_status,
            "current_stand_id": team.current_stand_id,
            "current_position_lat": team.current_position_lat,
            "current_position_lng": team.current_position_lng,
            "last_position_update": team.last_position_update,
            "is_active": team.is_active,
            "team_type_code": team.team_type.as_ref().and_then(|t| t.code.as_deref()),
            "team_type_name": team.team_type.as_ref().map(|t| t.name.as_str()),
        });

        // 名册只暴露活跃成员；每行钉 user_id，供按人查询资质等后续只读联动。
        let members: Vec<Value> = team
            .members
            .iter()
            .filter(|m| m.is_active)
            .map(|m| {
                json!({
                    "team_member_id": m.id,
                    "user_id": m.user_id,
                    "role": m.role,
                    "can_drive": m.can_drive,
                    "joined_at": m.joined_at,
                    "username": m.username,
                    "display_name": m.user_display_name,
                })
            })
            .collect();

        let mut response = json!({
            "team": profile,
            "members": members,
            "active_member_count": members.len(),
        });
        response["evidence"] = evidence(None);
        Ok(response)
    }
}