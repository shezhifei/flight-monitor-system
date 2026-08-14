//! PostgreSQL implementation of [`RunAuthorizationContextLoader`].
//!
//! Loads authorization context from Rust-persisted data:
//! - `requester_user_id` from `ai_jobs`
//! - `entity_id` from `ai_runs.input_envelope`
//! - User roles/permissions from `users`/`user_roles`/`roles`/`role_permissions`/`permissions`
//! - Entity tool allowlist from `ai_entities` config (scoped to current entity)
//! - Tool governance (`required_account_permissions`, etc.) from the entity's
//!   tool definitions, overlaid on top of [`RustToolGovernanceResolver`] defaults
//!
//! Fields embedded in MQ payloads by the Python sidecar are NOT trusted.

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::tool_authorization::ToolAuthorizationContext;
use fms_domain::models::tool_governance::{AuthorizationMode, ResolvedToolGovernance, RustToolGovernanceResolver};
use fms_domain::ports::ai_auth_context_loader::{AuthContextLoaderError, RunAuthorizationContextLoader};
use fms_domain::ports::ai_entity_config_repository::AiEntityConfigRepository;

pub struct PgRunAuthorizationContextLoader {
    pool: PgPool,
    entity_config_repo: Arc<dyn AiEntityConfigRepository + Send + Sync>,
}

impl PgRunAuthorizationContextLoader {
    pub fn new(pool: PgPool, entity_config_repo: Arc<dyn AiEntityConfigRepository + Send + Sync>) -> Self {
        Self {
            pool,
            entity_config_repo,
        }
    }

    async fn load_requester_user_id(&self, job_id: &str) -> Result<Option<String>, AuthContextLoaderError> {
        let row = sqlx::query("SELECT requester_user_id FROM ai_jobs WHERE job_id = $1")
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AuthContextLoaderError::Internal(format!("db error loading job: {e}")))?;

        match row {
            Some(r) => {
                let uid: Option<String> = r
                    .try_get("requester_user_id")
                    .map_err(|e| AuthContextLoaderError::Internal(format!("column error: {e}")))?;
                Ok(uid)
            }
            None => Err(AuthContextLoaderError::JobNotFound(job_id.to_string())),
        }
    }

    async fn load_user_roles_and_permissions(
        &self,
        user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), AuthContextLoaderError> {
        let mut roles: Vec<String> = Vec::new();
        let mut permissions: Vec<String> = Vec::new();

        let role_rows = sqlx::query(
            r#"
            SELECT r.name
            FROM roles r
            JOIN user_roles ur ON ur.role_id = r.id
            WHERE ur.user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthContextLoaderError::Internal(format!("db error loading roles: {e}")))?;

        for row in &role_rows {
            let name: String = row
                .try_get("name")
                .map_err(|e| AuthContextLoaderError::Internal(format!("column error: {e}")))?;
            roles.push(name);
        }

        let perm_rows = sqlx::query(
            r#"
            SELECT DISTINCT p.name
            FROM permissions p
            JOIN role_permissions rp ON rp.permission_id = p.id
            JOIN user_roles ur ON ur.role_id = rp.role_id
            WHERE ur.user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AuthContextLoaderError::Internal(format!("db error loading permissions: {e}")))?;

        for row in &perm_rows {
            let name: String = row
                .try_get("name")
                .map_err(|e| AuthContextLoaderError::Internal(format!("column error: {e}")))?;
            permissions.push(name);
        }

        Ok((roles, permissions))
    }

    /// Extract the entity_id from the run's `input_envelope`.
    /// The entity_id is stored in `input_envelope.entity_id` or
    /// `input_envelope.context.entity_id`.
    async fn resolve_entity_id(&self, run_id: &str) -> Result<Option<String>, AuthContextLoaderError> {
        let row = sqlx::query("SELECT input_envelope FROM ai_runs WHERE run_id = $1")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AuthContextLoaderError::Internal(format!("db error loading run: {e}")))?;

        let Some(r) = row else {
            return Err(AuthContextLoaderError::RunNotFound(run_id.to_string()));
        };

        let envelope: Option<Value> = r
            .try_get("input_envelope")
            .map_err(|e| AuthContextLoaderError::Internal(format!("column error: {e}")))?;

        let Some(envelope) = envelope else {
            return Ok(None);
        };

        // Try direct entity_id field first
        if let Some(eid) = envelope.get("entity_id").and_then(|v| v.as_str()) {
            if !eid.is_empty() {
                return Ok(Some(eid.to_string()));
            }
        }
        // Try nested context.entity_id
        if let Some(eid) = envelope
            .get("context")
            .and_then(|c| c.get("entity_id"))
            .and_then(|v| v.as_str())
        {
            if !eid.is_empty() {
                return Ok(Some(eid.to_string()));
            }
        }
        // Try context.entityType (some envelopes use this pattern)
        if let Some(eid) = envelope
            .get("context")
            .and_then(|c| c.get("entity_type"))
            .and_then(|v| v.as_str())
        {
            if !eid.is_empty() {
                return Ok(Some(eid.to_string()));
            }
        }

        Ok(None)
    }

    /// Load the entity tool allowlist for the current entity only.
    async fn load_entity_tool_allowlist(&self, entity_id: &str) -> Result<Vec<String>, AuthContextLoaderError> {
        let entity = self
            .entity_config_repo
            .find_by_id(entity_id)
            .await
            .map_err(|e: DomainError| AuthContextLoaderError::Internal(format!("entity config error: {e}")))?;

        let Some(entity) = entity else {
            return Ok(Vec::new());
        };

        let mut tools = Vec::new();
        let allowed = entity
            .config
            .pointer("/tooling/allowed_tools")
            .or_else(|| entity.config.get("allowed_tools"))
            .and_then(|v| v.as_array());
        if let Some(allowed) = allowed {
            for t in allowed {
                if let Some(name) = t.as_str() {
                    tools.push(name.to_string());
                }
            }
        }
        tools.sort();
        tools.dedup();
        Ok(tools)
    }

    /// Load tool-specific governance from the entity config and overlay
    /// it on top of the resolver's default governance.
    ///
    /// The resolver provides a safe default (RustPdp for unknown tools,
    /// PublicDirect for known L0 tools). The entity config's tool
    /// definitions may specify `required_account_permissions`,
    /// `authorization_mode`, `object_policy`, etc. These are trusted
    /// because they come from Rust-persisted entity config, not from
    /// Python MQ payloads.
    fn resolve_tool_governance(entity_config: Option<&Value>, tool_name: &str) -> ResolvedToolGovernance {
        let base = RustToolGovernanceResolver::resolve(tool_name);

        let Some(config) = entity_config else {
            return base;
        };

        let Some(tools) = config.get("tools").and_then(|v| v.as_array()) else {
            return base;
        };

        let Some(tool_def) = tools
            .iter()
            .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(tool_name))
        else {
            return base;
        };

        let mut governance = base.clone();

        // Overlay required_account_permissions from entity config
        if let Some(perms) = tool_def.get("required_account_permissions").and_then(|v| v.as_array()) {
            governance.required_account_permissions =
                perms.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        }

        // Overlay authorization_mode from entity config (trusted:
        // entity config is Rust-persisted, not Python payload)
        if let Some(mode) = tool_def.get("authorization_mode").and_then(|v| v.as_str()) {
            if mode == "public_direct" && base.is_public_direct() {
                // Entity config confirms the resolver's classification
            } else if mode == "rust_pdp" {
                governance.authorization_mode = AuthorizationMode::RustPdp;
                governance.public = false;
            }
        }

        // Overlay object_policy from entity config
        if let Some(op) = tool_def.get("object_policy") {
            if let Some(ota) = op.get("object_type_arg").and_then(|v| v.as_str()) {
                governance.object_policy.object_type_arg = Some(ota.to_string());
            }
            if let Some(oia) = op.get("object_id_arg").and_then(|v| v.as_str()) {
                governance.object_policy.object_id_arg = Some(oia.to_string());
            }
            if let Some(perm) = op.get("permission").and_then(|v| v.as_str()) {
                governance.object_policy.permission = Some(perm.to_string());
            }
        }

        governance
    }
}

#[async_trait]
impl RunAuthorizationContextLoader for PgRunAuthorizationContextLoader {
    async fn load_context(
        &self,
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        tool_name: &str,
        tool_args: &Value,
    ) -> Result<ToolAuthorizationContext, AuthContextLoaderError> {
        let requester_user_id = self
            .load_requester_user_id(job_id)
            .await?
            .ok_or_else(|| AuthContextLoaderError::RequesterNotFound(job_id.to_string()))?;

        let (requester_user_roles, requester_permissions) =
            self.load_user_roles_and_permissions(&requester_user_id).await?;

        // Resolve entity_id from the run's input_envelope (Rust-persisted).
        // If entity_id is not found, we fall back to an empty allowlist
        // and resolver-only governance — the tool will still go through
        // Rust PDP but won't have entity-specific permission overrides.
        let entity_id = self.resolve_entity_id(run_id).await?;
        let (entity_tool_allowlist, tool_governance) = if let Some(ref eid) = entity_id {
            let entity_config = self
                .entity_config_repo
                .find_by_id(eid)
                .await
                .map_err(|e: DomainError| AuthContextLoaderError::Internal(format!("entity config error: {e}")))?;
            let allowlist = self.load_entity_tool_allowlist(eid).await?;
            let config_value = entity_config.as_ref().map(|e| &e.config);
            let governance = Self::resolve_tool_governance(config_value, tool_name);
            (allowlist, governance)
        } else {
            tracing::debug!(
                target: "ai_auth_context_loader",
                run_id = %run_id,
                "entity_id not found in input_envelope; using resolver-only governance"
            );
            (Vec::new(), RustToolGovernanceResolver::resolve(tool_name))
        };

        Ok(ToolAuthorizationContext {
            requester_user_id,
            requester_user_roles,
            requester_permissions,
            requester_object_policies: Vec::new(),
            entity_tool_allowlist,
            tool_governance,
            tool_call_pk: tool_call_pk.to_string(),
            tool_args: tool_args.clone(),
            feature_flags: HashMap::new(),
        })
    }
}
