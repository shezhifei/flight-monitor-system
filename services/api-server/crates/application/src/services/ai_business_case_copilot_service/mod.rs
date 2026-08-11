//! AI Copilot service for voice-derived business-case drafts.
//!
//! This module is split into focused sub-modules:
//! - [`schemas`]: request / response / error / notification DTOs and serde
//!   payload structs used for LLM parsing.
//! - [`access`]: the `AiCopilotBatchAccess` helper that gates batch reads
//!   per actor identity.
//! - [`config`]: AI extraction / business case property config types and
//!   the normalization helpers that merge legacy and case-property configs.
//! - [`service`]: the `AiBusinessCaseCopilotService` struct, its constructor
//!   and full impl, including all draft / commit / batch / recovery /
//!   flight-matching logic and the `#[cfg(test)]` suite.

pub mod access;
pub mod config;
pub mod schemas;

mod batch;
mod commit;
mod draft;
mod helpers;
mod service;

#[cfg(test)]
mod tests;

pub use access::AiCopilotBatchAccess;
pub use service::AiBusinessCaseCopilotService;
pub use service::DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS;

// Re-export schema types that are part of the public surface so existing
// `use crate::services::ai_business_case_copilot_service::Foo` imports
// continue to resolve without modification.
pub use schemas::*;
