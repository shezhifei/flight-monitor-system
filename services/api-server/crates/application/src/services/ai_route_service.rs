//! AI configuration and tool-catalog application service.
//!
//! Runtime execution state is owned by `AiRuntimeService`; HTTP runtime routes
//! depend on that service directly.

use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::error::DomainError;

use crate::schemas::ai_schemas::{
    ConnectionProbeRequest, EntityConfigUpdate, EntityToolsUpdateRequest, SystemPromptUpdate,
};
use crate::services::ai_admin_service::AiAdminService;
use crate::services::ai_runtime_service::AiToolExecutionSpec;

#[derive(Debug)]
pub enum AiRouteError {
    Domain(DomainError),
}

impl From<DomainError> for AiRouteError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

pub struct AiRouteService {
    admin_service: Arc<AiAdminService>,
}

impl AiRouteService {
    pub fn new(admin_service: Arc<AiAdminService>) -> Self {
        Self { admin_service }
    }

    pub async fn capabilities(&self, execute_permission: bool, chat_permission: bool) -> Result<Value, AiRouteError> {
        let ai_ready = self.admin_service.has_usable_ai_config().await?;
        let mut missing_reasons = Vec::new();
        if !ai_ready {
            missing_reasons.push("NO_AI_CONFIG");
        }
        if !execute_permission {
            missing_reasons.push("NO_AI_EXECUTE_PERMISSION");
        }
        if !chat_permission {
            missing_reasons.push("NO_AI_CHAT_PERMISSION");
        }
        Ok(json!({
            "ai_ready": ai_ready,
            "ai_execute_permission": execute_permission,
            "ai_chat_permission": chat_permission,
            "missing_reasons": missing_reasons,
        }))
    }

    pub fn validate_invocation_mode(&self, mode: Option<&str>) -> Result<(), AiRouteError> {
        if let Some(mode) = mode {
            let normalized = mode.trim().to_lowercase();
            if !normalized.is_empty() && normalized != "user_requested" && normalized != "agent_autonomous" {
                return Err(AiRouteError::Domain(DomainError::ValidationError(format!(
                    "无效调用模式: {mode}"
                ))));
            }
        }
        Ok(())
    }

    pub async fn list_tools(&self, category: Option<&str>) -> Result<Value, AiRouteError> {
        Ok(Value::Array(self.admin_service.list_tools_payload(category)?))
    }

    pub async fn list_tool_categories(&self) -> Value {
        self.admin_service.list_tool_categories_payload()
    }

    pub async fn find_tool_spec(
        &self,
        tool_name: &str,
        tool_args: &Value,
    ) -> Result<AiToolExecutionSpec, AiRouteError> {
        let item = self
            .admin_service
            .list_tools_payload(None)?
            .into_iter()
            .find(|item| item.get("name").and_then(Value::as_str) == Some(tool_name))
            .ok_or_else(|| {
                AiRouteError::Domain(DomainError::NotFound {
                    entity_type: "AiTool",
                    id: tool_name.to_string(),
                })
            })?;
        let payload = tool_args
            .as_object()
            .ok_or_else(|| DomainError::ValidationError("tool_args 必须为 JSON 对象".to_string()))?;
        let missing = item
            .get("required_params")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|field| !payload.contains_key(*field))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(AiRouteError::Domain(DomainError::ValidationError(format!(
                "缺少必填参数: {}",
                missing.join(", ")
            ))));
        }

        Ok(AiToolExecutionSpec {
            tool_name: tool_name.to_string(),
            category: item
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("custom")
                .to_string(),
            operation_level: item
                .get("operation_level")
                .and_then(Value::as_str)
                .unwrap_or("l0_read")
                .to_string(),
            side_effect: item.get("side_effect").and_then(Value::as_bool).unwrap_or(false),
            query_intent: None,
            query_dataset: None,
        })
    }

    pub async fn list_entities(&self) -> Result<Value, AiRouteError> {
        Ok(self.admin_service.list_entities_payload().await?)
    }

    pub async fn get_entity(&self, entity_id: &str) -> Result<Option<Value>, AiRouteError> {
        Ok(self.admin_service.get_entity_masked_config(entity_id).await?)
    }

    pub async fn update_entity(&self, entity_id: &str, update: EntityConfigUpdate) -> Result<Value, AiRouteError> {
        Ok(self.admin_service.update_entity(entity_id, update).await?)
    }

    pub async fn test_connection_base(&self, request: ConnectionProbeRequest) -> Result<Value, AiRouteError> {
        match self.admin_service.test_connection(request).await {
            Ok(payload) => Ok(payload),
            Err(DomainError::NotFound { .. }) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "AiEntityConfig",
                id: "unknown".to_string(),
            })),
            Err(DomainError::ValidationError(message)) | Err(DomainError::Internal(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(other) => Err(AiRouteError::Domain(other)),
        }
    }

    pub async fn list_models(&self) -> Value {
        self.admin_service.list_available_models_payload()
    }

    pub async fn get_entity_prompt(&self, entity_id: &str) -> Result<Option<Value>, AiRouteError> {
        Ok(self.admin_service.get_entity_prompt(entity_id).await?)
    }

    pub async fn update_entity_prompt(&self, entity_id: &str, data: SystemPromptUpdate) -> Result<(), AiRouteError> {
        Ok(self.admin_service.update_entity_prompt(entity_id, data).await?)
    }

    pub async fn registry_status(&self) -> Value {
        self.admin_service.registry_status_payload()
    }

    pub async fn registry_initialize(&self) -> Value {
        self.admin_service.registry_initialize_payload()
    }

    pub async fn get_entity_tools(&self, entity_id: &str) -> Result<Option<Value>, AiRouteError> {
        Ok(self.admin_service.get_entity_tools(entity_id).await?)
    }

    pub async fn update_entity_tools(
        &self,
        entity_id: &str,
        data: EntityToolsUpdateRequest,
    ) -> Result<Value, AiRouteError> {
        Ok(self.admin_service.update_entity_tools(entity_id, data).await?)
    }
}
