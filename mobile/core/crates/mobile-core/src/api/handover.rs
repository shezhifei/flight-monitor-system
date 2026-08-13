//! Shift handover API wrappers.
//!
//! - List / detail / item-ack → raw
//! - Whole-handover ack → envelope wrapping `ShiftHandover`

use crate::client::ApiClient;
use crate::dto::handover::{
    ShiftHandover, ShiftHandoverItem, ShiftHandoverItemAcknowledgeRequest,
};
use crate::error::CoreError;

/// `GET /api/v2/shift-handovers` — raw array. Optional `status` filter.
pub async fn shift_handovers(
    client: &ApiClient,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ShiftHandover>, CoreError> {
    let path = match status {
        Some(s) => format!(
            "/api/v2/shift-handovers?status={s}&limit={limit}&offset={offset}"
        ),
        None => format!("/api/v2/shift-handovers?limit={limit}&offset={offset}"),
    };
    client
        .call_raw("GET", &path, Option::<&()>::None)
        .await
}

/// `GET /api/v2/shift-handovers/{id}`.
pub async fn shift_handover_detail(
    client: &ApiClient,
    id: &str,
) -> Result<ShiftHandover, CoreError> {
    client
        .call_raw(
            "GET",
            &format!("/api/v2/shift-handovers/{id}"),
            Option::<&()>::None,
        )
        .await
}

/// `POST /api/v2/shift-handovers/{id}/items/{item_id}/ack`.
pub async fn ack_handover_item(
    client: &ApiClient,
    handover_id: &str,
    item_id: &str,
    acknowledged: bool,
) -> Result<ShiftHandoverItem, CoreError> {
    client
        .call_raw(
            "POST",
            &format!("/api/v2/shift-handovers/{handover_id}/items/{item_id}/ack"),
            Some(&ShiftHandoverItemAcknowledgeRequest { acknowledged }),
        )
        .await
}

/// `POST /api/v2/shift-handovers/{id}/ack` — envelope wrapping the handover.
pub async fn ack_handover(
    client: &ApiClient,
    handover_id: &str,
) -> Result<ShiftHandover, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/shift-handovers/{handover_id}/ack"),
            Option::<&()>::None,
        )
        .await
}
