//! Wire DTOs (plan §3.6).
//!
//! Field authority: the legacy Kotlin models under
//! `android/app/src/main/java/com/flightmonitor/mobile/api/model/` (read-only
//! reference), cross-checked against the backend schemas in
//! `services/api-server/crates/application/src/schemas/`.
//! All structs use `snake_case` field names; `Option` mirrors Kotlin
//! nullability. Only the structures needed by the P1 main flow are included —
//! this is NOT a full transcription of the 34 endpoints.

pub mod auth;
pub mod dispatch;
pub mod mobile;

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
