use std::sync::Arc;

use serde_json::{json, Value};

use chrono::Utc;

use fms_domain::models::dispatch::PersonnelStatus;
use fms_domain::ports::dispatch_repository::{PersonnelRuntimeRepository, QualificationGrantRepository};
use fms_domain::ports::user_repository::UserRepository;

use super::error::{repo_err, OntologyActionError};
use super::support::{evidence, required_str};

/// `Personnel.get_context`：读取个人的名册档案（脱敏）、在岗 runtime 与资质发放。
/// 只读、不进执行器；由 `dispatch_read_action` 经 `read_action_permission` 调用。
pub struct PersonnelContextService {
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    personnel_runtime_repo: Arc<dyn PersonnelRuntimeRepository + Send + Sync>,
    qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
}

impl PersonnelContextService {
    pub fn new(
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        personnel_runtime_repo: Arc<dyn PersonnelRuntimeRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    ) -> Self {
        Self {
            user_repo,
            personnel_runtime_repo,
            qualification_grant_repo,
        }
    }

    pub async fn get(&self, args: &Value) -> Result<Value, OntologyActionError> {
        let user_id = required_str(args, "user_id")?;

        let user = self
            .user_repo
            .find_by_id(user_id)
            .await
            .map_err(repo_err)?
            .ok_or_else(|| OntologyActionError::NotFound(format!("person {user_id}")))?;

        // 名册档案面向本体读取，只暴露脱敏字段，绝不含密码哈希/令牌。
        let person = json!({
            "user_id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "department_id": user.department_id,
            "department": user.department,
            "job_title": user.job_title,
            "is_active": user.is_active,
            "is_admin": user.is_admin,
        });

        let runtime = match self
            .personnel_runtime_repo
            .find_by_user(user_id)
            .await
            .map_err(repo_err)?
        {
            Some(row) => json!({
                "user_id": row.user_id,
                "current_status": row.current_status,
                "current_stand_id": row.current_stand_id,
                "current_position_lat": row.current_position_lat,
                "current_position_lng": row.current_position_lng,
                "last_position_update": row.last_position_update,
                "updated_at": row.updated_at,
                "updated_by": row.updated_by,
            }),
            // 无行视为 off_duty。
            None => json!({
                "user_id": user_id,
                "current_status": PersonnelStatus::OffDuty,
                "current_stand_id": null,
                "current_position_lat": null,
                "current_position_lng": null,
                "last_position_update": null,
                "updated_at": null,
                "updated_by": null,
            }),
        };

        let qualification_grants = match user.department_id.as_deref() {
            Some(department_id) => self
                .qualification_grant_repo
                .find_by_department(department_id, Some(Utc::now()), &[user_id.to_string()], false)
                .await
                .map_err(repo_err)?,
            None => Vec::new(),
        };

        let mut response = json!({
            "person": person,
            "runtime": runtime,
        });
        response["qualification_grants"] = json!(qualification_grants);
        response["evidence"] = evidence(None);
        Ok(response)
    }
}
