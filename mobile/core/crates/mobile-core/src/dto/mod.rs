//! Wire DTOs for every mobile domain.
//!
//! Field authority: archived Kotlin models under
//! `legacy/android-kotlin/` (read-only reference), cross-checked against
//! the backend schemas in `services/api-server/crates/application/src/schemas/`.
//! All structs use `snake_case` field names; `Option` mirrors Kotlin
//! nullability.

pub mod auth;
pub mod business_case;
pub mod chat;
pub mod dispatch;
pub mod handover;
pub mod mobile;
pub mod notification;
pub mod operations;

use serde::Deserialize;

/// Standard response envelope `GenericApiResponse<T>`
/// (Kotlin `MobileModels.kt`; backend `ok_resp` family).
#[derive(Debug, Clone, Deserialize)]
pub struct GenericApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
    pub error: Option<serde_json::Value>,
    pub request_id: Option<String>,
}
