//! AI 路由应用服务
//!
//! 承载 `crate::api::routes::ai` 中的业务编排、DTO 转换与批量操作逻辑，
//! 使路由层保持为轻量包装器。

use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::error::DomainError;

use crate::schemas::ai_schemas::{
    ConnectionProbeRequest, EntityConfigUpdate, EntityToolsUpdateRequest, SystemPromptUpdate,
};
use crate::services::ai_admin_service::AiAdminService;
use crate::services::ai_runtime_service::{AiRuntimeError, AiRuntimeService, AiToolExecutionSpec};
use crate::services::nl_query_service::NLQueryService;

/// AI 路由服务层错误。
#[derive(Debug)]
pub enum AiRouteError {
    Domain(DomainError),
    Runtime(AiRuntimeError),
}

impl From<DomainError> for AiRouteError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<AiRuntimeError> for AiRouteError {
    fn from(error: AiRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

/// SSE 广播事件负载。
#[derive(Debug, Clone)]
pub struct AiEventPayload {
    pub event: String,
    pub payload: Value,
}

/// AI 路由应用服务。
pub struct AiRouteService {
    admin_service: Arc<AiAdminService>,
    runtime_service: Option<Arc<AiRuntimeService>>,
    nl_query_service: Option<Arc<NLQueryService>>,
}

impl AiRouteService {
    pub fn new(admin_service: Arc<AiAdminService>) -> Self {
        Self {
            admin_service,
            runtime_service: None,
            nl_query_service: None,
        }
    }

    pub fn with_runtime_service(mut self, service: Arc<AiRuntimeService>) -> Self {
        self.runtime_service = Some(service);
        self
    }

    pub fn with_nl_query_service(mut self, service: Arc<NLQueryService>) -> Self {
        self.nl_query_service = Some(service);
        self
    }

    fn runtime(&self) -> Result<&Arc<AiRuntimeService>, AiRouteError> {
        self.runtime_service
            .as_ref()
            .ok_or_else(|| AiRouteError::Domain(DomainError::Internal("AI runtime service not configured".to_string())))
    }

    // ------------------------------------------------------------------
    // Capabilities
    // ------------------------------------------------------------------

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

    // ------------------------------------------------------------------
    // Tools
    // ------------------------------------------------------------------

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
        Ok(Value::Array(
            self.admin_service
                .list_tools_payload(category)
                .map_err(AiRouteError::from)?,
        ))
    }

    pub async fn list_tool_categories(&self) -> Value {
        self.admin_service.list_tool_categories_payload()
    }

    pub async fn execute_tool(
        &self,
        tool_name: String,
        tool_args: Value,
        user_id: String,
        roles: Vec<String>,
    ) -> Result<(Value, Option<AiEventPayload>), AiRouteError> {
        let spec = self.find_tool_spec(&tool_name, &tool_args).await?;
        let payload = self
            .runtime()?
            .execute_tool(spec, tool_args.clone(), Some(user_id), roles)
            .await;
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("error")
            .to_string();
        let accepted = payload.get("success").and_then(Value::as_bool).unwrap_or(false) || status == "pending_approval";
        let execution_id = payload.get("execution_id").and_then(Value::as_str).map(str::to_string);

        let event = AiEventPayload {
            event: if status == "pending_approval" {
                "tool_pending_approval".to_string()
            } else {
                "tool_executed".to_string()
            },
            payload: json!({
                "status": status,
                "tool_name": tool_name,
                "execution_id": execution_id,
                "payload": payload,
            }),
        };

        let legacy_data = json!({
            "tool_name": tool_name,
            "result": payload.get("data").unwrap_or(&Value::Null),
            "error": payload.get("error").unwrap_or(&Value::Null),
        });

        let response = if !ai_feature_enabled("AI_EXEC_STATUS_V2", true) {
            json!({
                "success": accepted,
                "data": legacy_data,
            })
        } else {
            json!({
                "success": accepted,
                "accepted": accepted,
                "status": status,
                "code": payload.get("code"),
                "message": payload.get("message"),
                "recoverable": payload.get("recoverable"),
                "retryable": payload.get("retryable"),
                "execution_id": execution_id,
                "tool_name": tool_name,
                "severity": payload.get("severity"),
                "approval_required": payload.get("approval_required"),
                "approval_id": payload.get("approval_id"),
                "data": legacy_data,
                "result_data": payload.get("data").unwrap_or(&Value::Null),
                "error": payload.get("error").unwrap_or(&Value::Null),
                "meta": payload.get("meta").cloned().unwrap_or_else(|| json!({ "contract_version": "2.0" })),
            })
        };

        Ok((response, Some(event)))
    }

    pub async fn find_tool_spec(
        &self,
        tool_name: &str,
        tool_args: &Value,
    ) -> Result<AiToolExecutionSpec, AiRouteError> {
        let tools = self
            .admin_service
            .list_tools_payload(None)
            .map_err(AiRouteError::from)?;
        let item = tools
            .into_iter()
            .find(|item| item.get("name").and_then(Value::as_str) == Some(tool_name))
            .ok_or_else(|| {
                AiRouteError::Domain(DomainError::NotFound {
                    entity_type: "AiTool",
                    id: tool_name.to_string(),
                })
            })?;

        let required = item
            .get("required_params")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let payload = tool_args
            .as_object()
            .ok_or_else(|| DomainError::ValidationError("tool_args 必须为 JSON 对象".to_string()))?;
        let missing = required
            .iter()
            .filter_map(Value::as_str)
            .filter(|field| !payload.contains_key(*field))
            .map(str::to_string)
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

    // ------------------------------------------------------------------
    // Pending actions
    // ------------------------------------------------------------------

    pub async fn list_pending_actions(
        &self,
        status: Option<&str>,
        tool_name: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Value, AiRouteError> {
        Ok(self
            .runtime()?
            .list_pending_actions(status, tool_name, limit, offset)
            .await)
    }

    pub async fn get_action_diff(&self, action_id: &str) -> Result<Value, AiRouteError> {
        match self.runtime()?.get_pending_action_diff(action_id).await {
            Ok(data) => Ok(data),
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "PendingAction",
                id: action_id.to_string(),
            })),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::Conflict { .. }) => Err(AiRouteError::Domain(DomainError::ValidationError(
                "待审批动作状态冲突".to_string(),
            ))),
        }
    }

    pub async fn get_action_result(&self, action_id: &str) -> Result<Value, AiRouteError> {
        match self.runtime()?.get_pending_action_result(action_id).await {
            Ok(data) => Ok(data),
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "PendingAction",
                id: action_id.to_string(),
            })),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::Conflict { .. }) => Err(AiRouteError::Domain(DomainError::ValidationError(
                "待审批动作状态冲突".to_string(),
            ))),
        }
    }

    pub async fn approve_action(
        &self,
        action_id: String,
        approver_id: String,
    ) -> Result<(Value, Option<AiEventPayload>), AiRouteError> {
        match self
            .runtime()?
            .approve_pending_action(&action_id, &approver_id, None)
            .await
        {
            Ok(data) => {
                let event = AiEventPayload {
                    event: "action_approved".to_string(),
                    payload: json!({
                        "event": "approval_result",
                        "status": "success",
                        "action_id": action_id,
                        "approver_id": approver_id,
                        "pending_action": data.get("pending_action"),
                        "execution_result": data.get("execution_result"),
                    }),
                };
                let execution_result = data.get("execution_result").unwrap_or(&Value::Null);
                let response = json!({
                    "success": execution_result.get("status").and_then(Value::as_str) == Some("success"),
                    "status": execution_result.get("status").cloned().unwrap_or_else(|| json!("error")),
                    "code": execution_result.get("code"),
                    "message": execution_result.get("message"),
                    "recoverable": execution_result.get("recoverable"),
                    "retryable": execution_result.get("retryable"),
                    "severity": execution_result.get("severity"),
                    "approval_id": data.get("pending_action").and_then(|item| item.get("action_id")),
                    "data": data,
                    "meta": { "contract_version": "2.0" },
                });
                Ok((response, Some(event)))
            }
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "PendingAction",
                id: action_id,
            })),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::Conflict {
                code,
                message,
                blocked_reason,
            }) => Err(AiRouteError::Runtime(AiRuntimeError::Conflict {
                code,
                message,
                blocked_reason,
            })),
        }
    }

    pub async fn reject_action(
        &self,
        action_id: String,
        approver_id: String,
        reason: Option<String>,
    ) -> Result<(Value, Option<AiEventPayload>), AiRouteError> {
        match self
            .runtime()?
            .reject_pending_action(&action_id, &approver_id, reason.as_deref())
            .await
        {
            Ok(data) => {
                let event = AiEventPayload {
                    event: "action_rejected".to_string(),
                    payload: json!({
                        "event": "approval_result",
                        "status": "error",
                        "code": "APPROVAL_REJECTED",
                        "message": "approval request rejected by human reviewer",
                        "action_id": action_id,
                        "approver_id": approver_id,
                        "reason": reason,
                        "pending_action": data.get("pending_action"),
                    }),
                };
                let response = json!({
                    "success": true,
                    "status": "success",
                    "code": "APPROVAL_REJECTED",
                    "message": "approval request rejected by human reviewer",
                    "recoverable": true,
                    "retryable": false,
                    "severity": "warning",
                    "approval_id": data.get("pending_action").and_then(|item| item.get("action_id")),
                    "data": data,
                    "meta": { "contract_version": "2.0" },
                });
                Ok((response, Some(event)))
            }
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "PendingAction",
                id: action_id,
            })),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::Conflict {
                code,
                message,
                blocked_reason,
            }) => Err(AiRouteError::Runtime(AiRuntimeError::Conflict {
                code,
                message,
                blocked_reason,
            })),
        }
    }

    pub async fn approve_modified(
        &self,
        action_id: String,
        approver_id: String,
        modified_arguments: Value,
    ) -> Result<(Value, Option<AiEventPayload>), AiRouteError> {
        match self
            .runtime()?
            .approve_pending_action(&action_id, &approver_id, Some(modified_arguments))
            .await
        {
            Ok(data) => {
                let event = AiEventPayload {
                    event: "action_approved".to_string(),
                    payload: json!({
                        "event": "approval_result",
                        "status": "success",
                        "action_id": action_id,
                        "approver_id": approver_id,
                        "pending_action": data.get("pending_action"),
                        "execution_result": data.get("execution_result"),
                        "modification": data.get("modification"),
                    }),
                };
                let execution_result = data.get("execution_result").unwrap_or(&Value::Null);
                let response = json!({
                    "success": execution_result.get("status").and_then(Value::as_str) == Some("success"),
                    "status": execution_result.get("status").cloned().unwrap_or_else(|| json!("error")),
                    "code": execution_result.get("code"),
                    "message": execution_result.get("message"),
                    "recoverable": execution_result.get("recoverable"),
                    "retryable": execution_result.get("retryable"),
                    "severity": execution_result.get("severity"),
                    "approval_id": data.get("pending_action").and_then(|item| item.get("action_id")),
                    "data": data,
                    "meta": { "contract_version": "2.0" },
                });
                Ok((response, Some(event)))
            }
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "PendingAction",
                id: action_id,
            })),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::Conflict {
                code,
                message,
                blocked_reason,
            }) => Err(AiRouteError::Runtime(AiRuntimeError::Conflict {
                code,
                message,
                blocked_reason,
            })),
        }
    }

    pub async fn batch_approve(
        &self,
        action_ids: Vec<String>,
        approver_id: String,
    ) -> Result<(Value, Option<AiEventPayload>), AiRouteError> {
        Self::validate_batch_action_ids(&action_ids)?;
        let mut results = Vec::new();
        let mut succeeded = 0usize;
        for action_id in &action_ids {
            match self
                .runtime()?
                .approve_pending_action(action_id, &approver_id, None)
                .await
            {
                Ok(data) => {
                    let item = batch_approve_success_result(action_id, &data);
                    if item.get("success").and_then(Value::as_bool).unwrap_or(false) {
                        succeeded += 1;
                    }
                    results.push(item);
                }
                Err(AiRuntimeError::NotFound(_)) => {
                    results.push(batch_error_result(
                        action_id,
                        "error",
                        "PENDING_ACTION_NOT_FOUND",
                        format!("'{action_id}'"),
                    ));
                }
                Err(AiRuntimeError::Validation(message)) => {
                    results.push(batch_error_result(
                        action_id,
                        "error",
                        "PENDING_ACTION_BATCH_ERROR",
                        message,
                    ));
                }
                Err(AiRuntimeError::Conflict { code, message, .. }) => {
                    let status = if code == "PENDING_ACTION_EXPIRED" {
                        "expired"
                    } else {
                        "conflict"
                    };
                    results.push(batch_error_result(action_id, status, &code, message));
                }
            }
        }
        let payload = json!({
            "total": action_ids.len(),
            "succeeded": succeeded,
            "failed": action_ids.len() - succeeded,
            "results": results,
        });
        let event = AiEventPayload {
            event: "batch_approved".to_string(),
            payload: json!({
                "event": "batch_approval_result",
                "approver_id": approver_id,
                "total": payload.get("total"),
                "succeeded": payload.get("succeeded"),
                "failed": payload.get("failed"),
                "results": payload.get("results"),
                "generated_at": Utc::now().to_rfc3339(),
            }),
        };
        Ok((
            json!({ "success": action_ids.len() == succeeded, "data": payload }),
            Some(event),
        ))
    }

    pub async fn batch_reject(
        &self,
        action_ids: Vec<String>,
        reason: Option<String>,
        approver_id: String,
    ) -> Result<(Value, Option<AiEventPayload>), AiRouteError> {
        Self::validate_batch_action_ids(&action_ids)?;
        let mut results = Vec::new();
        let mut succeeded = 0usize;
        for action_id in &action_ids {
            match self
                .runtime()?
                .reject_pending_action(action_id, &approver_id, reason.as_deref())
                .await
            {
                Ok(data) => {
                    succeeded += 1;
                    results.push(batch_reject_success_result(action_id, &data));
                }
                Err(AiRuntimeError::NotFound(_)) => {
                    results.push(batch_error_result(
                        action_id,
                        "error",
                        "PENDING_ACTION_NOT_FOUND",
                        format!("'{action_id}'"),
                    ));
                }
                Err(AiRuntimeError::Validation(message)) => {
                    results.push(batch_error_result(
                        action_id,
                        "error",
                        "PENDING_ACTION_BATCH_ERROR",
                        message,
                    ));
                }
                Err(AiRuntimeError::Conflict { code, message, .. }) => {
                    let status = if code == "PENDING_ACTION_EXPIRED" {
                        "expired"
                    } else {
                        "conflict"
                    };
                    results.push(batch_error_result(action_id, status, &code, message));
                }
            }
        }
        let payload = json!({
            "total": action_ids.len(),
            "succeeded": succeeded,
            "failed": action_ids.len() - succeeded,
            "reason": reason,
            "results": results,
        });
        let event = AiEventPayload {
            event: "batch_rejected".to_string(),
            payload: json!({
                "event": "batch_rejection_result",
                "approver_id": approver_id,
                "total": payload.get("total"),
                "succeeded": payload.get("succeeded"),
                "failed": payload.get("failed"),
                "reason": payload.get("reason"),
                "results": payload.get("results"),
                "generated_at": Utc::now().to_rfc3339(),
            }),
        };
        Ok((
            json!({ "success": action_ids.len() == succeeded, "data": payload }),
            Some(event),
        ))
    }

    fn validate_batch_action_ids(action_ids: &[String]) -> Result<(), AiRouteError> {
        if action_ids.is_empty() {
            return Err(AiRouteError::Domain(DomainError::ValidationError(
                "action_ids 不可为空".to_string(),
            )));
        }
        if action_ids.len() > 50 {
            return Err(AiRouteError::Domain(DomainError::ValidationError(
                "单次批量操作上限为 50 条".to_string(),
            )));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Entities
    // ------------------------------------------------------------------

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
        let result = self.admin_service.test_connection(request).await;
        match result {
            Ok(payload) => Ok(payload),
            Err(DomainError::NotFound { .. }) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "AiEntityConfig",
                id: "unknown".to_string(),
            })),
            Err(DomainError::ValidationError(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(DomainError::Internal(message)) => Err(AiRouteError::Domain(DomainError::ValidationError(message))),
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

    // ------------------------------------------------------------------
    // Todos / executions
    // ------------------------------------------------------------------

    pub async fn execute_todo(
        &self,
        todo_id: String,
        entity_id: Option<String>,
        max_iterations: usize,
        system_prompt_override: Option<String>,
        user_id: String,
        roles: Vec<String>,
    ) -> Result<(Value, AiEventPayload), AiRouteError> {
        let data = self
            .runtime()?
            .execute_todo(
                &todo_id,
                entity_id,
                max_iterations,
                system_prompt_override,
                Some(user_id),
                roles,
            )
            .await;
        let event = AiEventPayload {
            event: "todo_executed".to_string(),
            payload: json!({
                "todo_id": todo_id,
                "execution": data,
            }),
        };
        Ok((data, event))
    }

    pub async fn execute_todo_tree(
        &self,
        todo_id: String,
        max_iterations_per_todo: usize,
        fail_fast: bool,
        user_id: String,
        roles: Vec<String>,
    ) -> Result<(Value, AiEventPayload), AiRouteError> {
        let data = self
            .runtime()?
            .execute_todo_tree(&todo_id, max_iterations_per_todo, fail_fast, Some(user_id), roles)
            .await;
        let event = AiEventPayload {
            event: "todo_tree_executed".to_string(),
            payload: json!({
                "root_todo_id": todo_id,
                "result": data,
            }),
        };
        Ok((data, event))
    }

    pub async fn create_chain(&self, template_id: &str, context: Value) -> Result<Value, AiRouteError> {
        match self.runtime()?.create_chain_from_template(template_id, context).await {
            Ok(data) => Ok(data),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "TodoChain",
                id: template_id.to_string(),
            })),
            Err(AiRuntimeError::Conflict { message, .. }) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
        }
    }

    pub async fn get_chain_status(&self, root_todo_id: &str) -> Result<Value, AiRouteError> {
        match self.runtime()?.get_chain_status(root_todo_id).await {
            Ok(data) => Ok(data),
            Err(AiRuntimeError::NotFound(_)) => Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "TodoChain",
                id: root_todo_id.to_string(),
            })),
            Err(AiRuntimeError::Validation(message)) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
            Err(AiRuntimeError::Conflict { message, .. }) => {
                Err(AiRouteError::Domain(DomainError::ValidationError(message)))
            }
        }
    }

    pub async fn list_chain_templates(&self) -> Result<Value, AiRouteError> {
        Ok(self.runtime()?.list_chain_templates().await)
    }

    pub async fn get_execution(
        &self,
        run_id: &str,
        requester_permissions: &[String],
        requester_user_id: &str,
    ) -> Result<Option<Value>, AiRouteError> {
        let data = if let Some(data) = self.runtime()?.get_execution(run_id).await {
            Some(data)
        } else if let Some(svc) = self.nl_query_service.as_ref() {
            svc.get_runtime_execution(run_id).await
        } else {
            None
        };
        let Some(data) = data else {
            return Ok(None);
        };
        if !can_access_execution_value(requester_permissions, requester_user_id, &data) {
            return Err(AiRouteError::Domain(DomainError::PermissionDenied(
                "无权访问该执行记录".to_string(),
            )));
        }
        Ok(Some(data))
    }

    pub async fn list_executions(
        &self,
        todo_id: Option<&str>,
        entity_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
        can_view_all: bool,
        requester_user_id: &str,
    ) -> Result<Value, AiRouteError> {
        let executions = self.runtime()?.list_executions(todo_id, entity_id, status, limit).await;
        let filtered: Vec<Value> = if can_view_all {
            executions
        } else {
            executions
                .into_iter()
                .filter(|item| {
                    execution_owner_id(item)
                        .map(|value| value == requester_user_id)
                        .unwrap_or(false)
                })
                .collect()
        };
        Ok(json!({
            "executions": filtered,
            "total": filtered.len(),
        }))
    }

    pub async fn cancel_execution(
        &self,
        run_id: String,
        requester_permissions: &[String],
        requester_user_id: String,
    ) -> Result<(bool, Option<AiEventPayload>), AiRouteError> {
        let Some(data) = self.runtime()?.get_execution(&run_id).await else {
            return Err(AiRouteError::Domain(DomainError::NotFound {
                entity_type: "Execution",
                id: run_id,
            }));
        };
        if !can_access_execution_value(requester_permissions, &requester_user_id, &data) {
            return Err(AiRouteError::Domain(DomainError::PermissionDenied(
                "无权取消该执行".to_string(),
            )));
        }
        let success = self.runtime()?.cancel_execution(&run_id).await;
        let event = if success {
            Some(AiEventPayload {
                event: "execution_cancelled".to_string(),
                payload: json!({
                    "execution_id": run_id,
                    "status": "cancelled",
                }),
            })
        } else {
            None
        };
        Ok((success, event))
    }

    // ------------------------------------------------------------------
    // Metrics
    // ------------------------------------------------------------------

    pub async fn rate_limit_status(&self) -> Result<Value, AiRouteError> {
        Ok(self.runtime()?.rate_limit_status().await)
    }

    pub async fn query_routing_metrics(&self) -> Result<Value, AiRouteError> {
        Ok(self.runtime()?.query_routing_metrics().await)
    }

    pub async fn report_schema_metrics(&self) -> Result<Value, AiRouteError> {
        Ok(self.runtime()?.report_schema_metrics().await)
    }

    pub async fn execution_visibility_metrics(&self) -> Result<Value, AiRouteError> {
        Ok(self.runtime()?.execution_visibility_metrics().await)
    }

    pub async fn todo_graph_pilot_metrics(
        &self,
        entity_id: Option<String>,
        window_hours: i32,
        sample_limit: i32,
        pending_stale_after_minutes: i32,
    ) -> Result<Value, AiRouteError> {
        Ok(self
            .runtime()?
            .todo_graph_pilot_metrics(entity_id, window_hours, sample_limit, pending_stale_after_minutes)
            .await)
    }
}

// ------------------------------------------------------------------
// Shared helpers
// ------------------------------------------------------------------

pub fn execution_owner_id(execution: &Value) -> Option<&str> {
    execution.get("user_id").and_then(Value::as_str)
}

pub fn can_access_execution_value(
    requester_permissions: &[String],
    requester_user_id: &str,
    execution: &Value,
) -> bool {
    if requester_permissions.iter().any(|value| value == "ai:monitor") {
        return true;
    }
    let owner = execution_owner_id(execution).unwrap_or("");
    !owner.is_empty() && owner == requester_user_id
}

pub fn parse_ai_feature_flag(value: Option<&str>, default: bool) -> bool {
    value
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "0" | "false" | "off" | "no" => false,
            "1" | "true" | "on" | "yes" => true,
            _ => default,
        })
        .unwrap_or(default)
}

pub fn ai_feature_enabled(flag_name: &str, default: bool) -> bool {
    let value = std::env::var(flag_name).ok();
    parse_ai_feature_flag(value.as_deref(), default)
}

pub fn batch_error_result(action_id: &str, status: &str, code: &str, message: impl Into<String>) -> Value {
    let message = message.into();
    json!({
        "action_id": action_id,
        "success": false,
        "status": status,
        "code": code,
        "message": message,
    })
}

pub fn batch_approve_success_result(action_id: &str, data: &Value) -> Value {
    let pending_action = data.get("pending_action").unwrap_or(&Value::Null);
    let exec_result = data.get("execution_result").unwrap_or(&Value::Null);
    let pending_status = pending_action
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let exec_status = exec_result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let normalized_status = if !pending_status.is_empty() {
        pending_status
    } else if !exec_status.is_empty() {
        exec_status.clone()
    } else {
        "unknown".to_string()
    };
    let success = matches!(normalized_status.as_str(), "approved" | "executed" | "success")
        && matches!(exec_status.as_str(), "success" | "executed" | "");

    json!({
        "action_id": action_id,
        "success": success,
        "status": normalized_status,
        "code": exec_result.get("code").or_else(|| pending_action.get("status_code")).unwrap_or(&Value::Null),
        "message": exec_result.get("message").or_else(|| pending_action.get("execution_error")).unwrap_or(&Value::Null),
        "data": data,
    })
}

pub fn batch_reject_success_result(action_id: &str, data: &Value) -> Value {
    let pending_action = data.get("pending_action").unwrap_or(&Value::Null);
    json!({
        "action_id": action_id,
        "success": true,
        "status": "rejected",
        "code": pending_action.get("status_code").cloned().unwrap_or_else(|| json!("APPROVAL_REJECTED")),
        "message": "approval request rejected by human reviewer",
        "data": data,
    })
}
