//! Mobile workbench API wrapper (plan §0.5 Mobile group).
//!
//! `GET /api/v2/mobile/workbench` — enveloped `MobileWorkbenchResponse`
//! (backend `routes/mobile.rs`).

use crate::client::ApiClient;
use crate::dto::mobile::MobileWorkbenchResponse;
use crate::error::CoreError;

/// Load the mobile workbench. `pending_sync_action_count` /
/// `max_orders` mirror the legacy query params (backend defaults 0 / 50,
/// clamps 0..=100000 / 1..=200).
pub async fn workbench(
    client: &ApiClient,
    pending_sync_action_count: i64,
    max_orders: i64,
) -> Result<MobileWorkbenchResponse, CoreError> {
    client
        .call_with_envelope::<MobileWorkbenchResponse, ()>(
            "GET",
            &format!(
                "/api/v2/mobile/workbench?pending_sync_action_count={pending_sync_action_count}&max_orders={max_orders}"
            ),
            None,
        )
        .await
}
