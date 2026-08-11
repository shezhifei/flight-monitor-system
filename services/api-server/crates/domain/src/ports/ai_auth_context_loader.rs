//! Run authorization context loader (Phase 4 hardening).
//!
//! This port is the **trust boundary** for tool authorization. It loads
//! the [`ToolAuthorizationContext`] from Rust-persisted data sources
//! (ai_jobs, user/role/permission tables, entity config) instead of
//! trusting fields embedded in MQ payloads sent by the Python sidecar.
//!
//! The Python sidecar may include `requester`, `governance`, and
//! `entity_allowlist` fields in MQ events for audit/logging purposes,
//! but they MUST NOT be used for authorization decisions.

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::models::tool_authorization::ToolAuthorizationContext;

#[derive(Debug, Error)]
pub enum AuthContextLoaderError {
    #[error("run not found: {0}")]
    RunNotFound(String),
    #[error("job not found for run: {0}")]
    JobNotFound(String),
    #[error("requester not found for job: {0}")]
    RequesterNotFound(String),
    #[error("entity config not found: {0}")]
    EntityConfigNotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Loads the authorization context for a tool call from trusted
/// Rust-persisted data. Implementations MUST NOT trust fields sent
/// by the Python sidecar in MQ payloads.
#[async_trait]
pub trait RunAuthorizationContextLoader: Send + Sync {
    /// Build a [`ToolAuthorizationContext`] for a tool call within
    /// the given run. The `tool_name`, `tool_args`, and `tool_call_pk`
    /// come from the MQ event (they are operationally necessary but
    /// do not convey trust). All identity/permission/governance fields
    /// are loaded from Rust-persisted storage.
    async fn load_context(
        &self,
        run_id: &str,
        job_id: &str,
        tool_call_pk: &str,
        tool_name: &str,
        tool_args: &Value,
    ) -> Result<ToolAuthorizationContext, AuthContextLoaderError>;
}
