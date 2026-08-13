//! Mobile operations event feed.

use crate::client::ApiClient;
use crate::dto::operations::OperationsEventsResponse;
use crate::error::CoreError;

/// `GET /api/v2/mobile/operations/events?limit=…` — enveloped.
pub async fn operations_events(
    client: &ApiClient,
    limit: i64,
) -> Result<OperationsEventsResponse, CoreError> {
    client
        .call_with_envelope(
            "GET",
            &format!("/api/v2/mobile/operations/events?limit={limit}"),
            Option::<&()>::None,
        )
        .await
}
