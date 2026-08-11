//! 标签应用服务。

use std::sync::Arc;

use fms_domain::broadcaster::Broadcaster;
use fms_domain::error::DomainError;
use fms_domain::models::label::{LabelDefinition, LabelScope};
use fms_domain::ports::label_repository::{CreateLabelDefinitionParams, LabelRepository, UpdateLabelDefinitionParams};

use crate::schemas::label_schemas::{AttachLabelRequest, CreateLabelRequest, LabelResponse, UpdateLabelRequest};

pub struct LabelService {
    repo: Arc<dyn LabelRepository + Send + Sync>,
    broadcaster: Arc<dyn Broadcaster + Send + Sync>,
}

impl LabelService {
    pub fn new(repo: Arc<dyn LabelRepository + Send + Sync>, broadcaster: Arc<dyn Broadcaster + Send + Sync>) -> Self {
        Self { repo, broadcaster }
    }

    pub async fn list_labels(&self, active_only: bool) -> Result<Vec<LabelResponse>, DomainError> {
        Ok(self
            .repo
            .get_all_definitions(active_only)
            .await?
            .into_iter()
            .map(to_response)
            .collect())
    }

    pub async fn create_label(
        &self,
        dto: CreateLabelRequest,
        actor: Option<String>,
    ) -> Result<LabelResponse, DomainError> {
        let scope = LabelScope::from_api(dto.scope.trim())
            .ok_or_else(|| DomainError::ValidationError(format!("无效的 scope: {}", dto.scope)))?;

        let code = dto.code.trim().to_string();
        if self.repo.get_definition_by_code(&code).await?.is_some() {
            return Err(DomainError::Conflict(format!("标签代码 '{code}' 已存在")));
        }

        let label = self
            .repo
            .create_definition(CreateLabelDefinitionParams {
                code,
                name: dto.name.trim().to_string(),
                color: dto.color.trim().to_string(),
                icon: dto.icon.map(|value| value.trim().to_string()),
                scope,
                created_by: actor,
            })
            .await?;
        Ok(to_response(label))
    }

    pub async fn update_label(&self, label_id: &str, dto: UpdateLabelRequest) -> Result<bool, DomainError> {
        self.repo
            .update_definition(
                label_id,
                UpdateLabelDefinitionParams {
                    name: dto.name.map(|value| value.trim().to_string()),
                    color: dto.color.map(|value| value.trim().to_string()),
                    icon: dto.icon.map(|value| value.trim().to_string()),
                    is_active: dto.is_active,
                    sort_order: dto.sort_order,
                },
            )
            .await
    }

    pub async fn delete_label(&self, label_id: &str) -> Result<bool, DomainError> {
        self.repo.delete_definition(label_id).await
    }

    /// Ontology 契约 §3.3.9 `label.add` 的受控写入口：为航班附加标签。
    /// 标签定义校验、scope 校验与广播复用 attach_flight_label，审计身份由调用方 JWT 注入。
    pub async fn add_to_flight(&self, flight_id: &str, label: &str, _actor: Option<&str>) -> Result<(), DomainError> {
        let code = label.trim();
        if code.is_empty() {
            return Err(DomainError::ValidationError("label is required".to_string()));
        }
        self.attach_flight_label(
            flight_id,
            AttachLabelRequest {
                code: code.to_string(),
            },
        )
        .await
    }

    pub async fn attach_flight_label(&self, flight_id: &str, dto: AttachLabelRequest) -> Result<(), DomainError> {
        let code = dto.code.trim().to_string();
        let definition = self
            .repo
            .get_definition_by_code(&code)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "label",
                id: code.clone(),
            })?;
        if definition.scope == LabelScope::Leg {
            return Err(DomainError::ValidationError(format!(
                "标签 '{}' 只能贴到航段，不能贴到航班",
                code
            )));
        }

        self.repo.attach_flight_label(flight_id, &code).await?;

        let updated_labels = self.repo.get_flight_labels(flight_id).await.unwrap_or_default();

        self.broadcaster
            .broadcast_event(
                "flights",
                Some("flight_labels_changed"),
                serde_json::json!({
                    "flight_id": flight_id,
                    "action": "attach",
                    "code": code,
                    "labels": updated_labels,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            )
            .await;

        Ok(())
    }

    pub async fn detach_flight_label(&self, flight_id: &str, code: &str) -> Result<(), DomainError> {
        let code = code.trim().to_string();
        self.repo.detach_flight_label(flight_id, &code).await?;

        let updated_labels = self.repo.get_flight_labels(flight_id).await.unwrap_or_default();

        self.broadcaster
            .broadcast_event(
                "flights",
                Some("flight_labels_changed"),
                serde_json::json!({
                    "flight_id": flight_id,
                    "action": "detach",
                    "code": code,
                    "labels": updated_labels,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            )
            .await;

        Ok(())
    }

    pub async fn attach_leg_label(
        &self,
        flight_id: &str,
        leg_type: &str,
        dto: AttachLabelRequest,
    ) -> Result<(), DomainError> {
        let leg_type = leg_type.trim();
        if !matches!(leg_type, "inbound" | "outbound") {
            return Err(DomainError::ValidationError(format!("无效的 leg_type: {leg_type}")));
        }

        let code = dto.code.trim().to_string();
        let definition = self
            .repo
            .get_definition_by_code(&code)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "label",
                id: code.clone(),
            })?;
        if definition.scope == LabelScope::Flight {
            return Err(DomainError::ValidationError(format!(
                "标签 '{}' 只能贴到航班，不能贴到航段",
                code
            )));
        }

        self.repo.attach_leg_label(flight_id, leg_type, &code).await?;

        let updated_labels = self.repo.get_leg_labels(flight_id, leg_type).await.unwrap_or_default();

        self.broadcaster
            .broadcast_event(
                "flights",
                Some("flight_labels_changed"),
                serde_json::json!({
                    "flight_id": flight_id,
                    "leg_type": leg_type,
                    "action": "attach",
                    "code": code,
                    "labels": updated_labels,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            )
            .await;

        Ok(())
    }

    pub async fn detach_leg_label(&self, flight_id: &str, leg_type: &str, code: &str) -> Result<(), DomainError> {
        let leg_type = leg_type.trim();
        if !matches!(leg_type, "inbound" | "outbound") {
            return Err(DomainError::ValidationError(format!("无效的 leg_type: {leg_type}")));
        }

        let code = code.trim().to_string();
        self.repo.detach_leg_label(flight_id, leg_type, &code).await?;

        let updated_labels = self.repo.get_leg_labels(flight_id, leg_type).await.unwrap_or_default();

        self.broadcaster
            .broadcast_event(
                "flights",
                Some("flight_labels_changed"),
                serde_json::json!({
                    "flight_id": flight_id,
                    "leg_type": leg_type,
                    "action": "detach",
                    "code": code,
                    "labels": updated_labels,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }),
            )
            .await;

        Ok(())
    }
}

fn to_response(label: LabelDefinition) -> LabelResponse {
    LabelResponse {
        label_id: label.label_id,
        code: label.code,
        name: label.name,
        color: label.color,
        icon: label.icon,
        scope: label.scope.as_str().to_string(),
        category: label.category.as_str().to_string(),
        is_active: label.is_active,
        sort_order: label.sort_order,
        created_by: label.created_by,
        created_at: label.created_at,
        updated_at: label.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Utc;
    use fms_domain::models::label::{LabelCategory, LabelDefinition};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeLabelRepo {
        definitions: Mutex<HashMap<String, LabelDefinition>>,
    }

    #[async_trait::async_trait]
    impl LabelRepository for FakeLabelRepo {
        async fn get_all_definitions(&self, active_only: bool) -> Result<Vec<LabelDefinition>, DomainError> {
            Ok(self
                .definitions
                .lock()
                .expect("lock definitions")
                .values()
                .filter(|item| !active_only || item.is_active)
                .cloned()
                .collect())
        }

        async fn get_definition_by_code(&self, code: &str) -> Result<Option<LabelDefinition>, DomainError> {
            Ok(self.definitions.lock().expect("lock definitions").get(code).cloned())
        }

        async fn create_definition(&self, params: CreateLabelDefinitionParams) -> Result<LabelDefinition, DomainError> {
            let label = LabelDefinition {
                label_id: "label-1".to_string(),
                code: params.code,
                name: params.name,
                color: params.color,
                icon: params.icon,
                scope: params.scope,
                category: LabelCategory::Custom,
                is_active: true,
                sort_order: 0,
                created_by: params.created_by,
                created_at: Some(Utc::now()),
                updated_at: Some(Utc::now()),
            };
            self.definitions
                .lock()
                .expect("lock definitions")
                .insert(label.code.clone(), label.clone());
            Ok(label)
        }

        async fn update_definition(
            &self,
            _label_id: &str,
            _params: UpdateLabelDefinitionParams,
        ) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn delete_definition(&self, _label_id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn attach_flight_label(&self, _flight_id: &str, _code: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn detach_flight_label(&self, _flight_id: &str, _code: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn attach_leg_label(&self, _flight_id: &str, _leg_type: &str, _code: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn detach_leg_label(&self, _flight_id: &str, _leg_type: &str, _code: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get_flight_labels(&self, _flight_id: &str) -> Result<Vec<String>, DomainError> {
            Ok(vec![])
        }

        async fn get_leg_labels(&self, _flight_id: &str, _leg_type: &str) -> Result<Vec<String>, DomainError> {
            Ok(vec![])
        }
    }

    struct FakeBroadcaster;

    #[async_trait::async_trait]
    impl Broadcaster for FakeBroadcaster {
        async fn broadcast_event(&self, _topic: &str, _event_name: Option<&str>, _payload: serde_json::Value) {}
    }

    fn make_definition(code: &str, scope: LabelScope) -> LabelDefinition {
        LabelDefinition {
            label_id: format!("label-{code}"),
            code: code.to_string(),
            name: code.to_string(),
            color: "#6B7280".to_string(),
            icon: None,
            scope,
            category: LabelCategory::Custom,
            is_active: true,
            sort_order: 0,
            created_by: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        }
    }

    #[tokio::test]
    async fn create_label_rejects_duplicate_code() {
        let repo = Arc::new(FakeLabelRepo::default());
        repo.definitions
            .lock()
            .expect("lock definitions")
            .insert("vip".to_string(), make_definition("vip", LabelScope::Leg));
        let broadcaster = Arc::new(FakeBroadcaster);
        let service = LabelService::new(repo, broadcaster);

        let result = service
            .create_label(
                CreateLabelRequest {
                    code: "vip".to_string(),
                    name: "VIP".to_string(),
                    color: "#ffffff".to_string(),
                    icon: None,
                    scope: "leg".to_string(),
                },
                Some("tester".to_string()),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[tokio::test]
    async fn attach_flight_label_rejects_leg_only_definition() {
        let repo = Arc::new(FakeLabelRepo::default());
        repo.definitions
            .lock()
            .expect("lock definitions")
            .insert("vip".to_string(), make_definition("vip", LabelScope::Leg));
        let broadcaster = Arc::new(FakeBroadcaster);
        let service = LabelService::new(repo, broadcaster);

        let result = service
            .attach_flight_label(
                "flight-1",
                AttachLabelRequest {
                    code: "vip".to_string(),
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }

    #[tokio::test]
    async fn attach_leg_label_rejects_invalid_leg_type() {
        let repo = Arc::new(FakeLabelRepo::default());
        let broadcaster = Arc::new(FakeBroadcaster);
        let service = LabelService::new(repo, broadcaster);

        let result = service
            .attach_leg_label(
                "flight-1",
                "middle",
                AttachLabelRequest {
                    code: "vip".to_string(),
                },
            )
            .await;

        assert!(matches!(result, Err(DomainError::ValidationError(_))));
    }
}
