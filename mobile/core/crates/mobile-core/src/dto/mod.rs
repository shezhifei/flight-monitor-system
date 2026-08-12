//! Wire DTOs (plan §3.6).
//!
//! Field authority: the legacy Kotlin models under
//! `android/app/src/main/java/com/flightmonitor/mobile/api/model/` (read-only
//! reference), cross-checked against the backend schemas in
//! `services/api-server/crates/application/src/schemas/`.
//! All structs use `snake_case` field names; `Option` mirrors Kotlin
//! nullability. P1 + P2 domains are covered; BusinessCase / Operations arrive
//! in P3.

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
