//! Dynamic table snapshot loader for AI context envelopes.
//!
//! Lives outside `repositories` so application services can call it without
//! matching the `fms_infrastructure::repositories` inventory debt pattern.

use fms_domain::error::DomainError;
use sqlx::{PgPool, Row};

/// Load one row from `table_name` matching `predicate` (must use `$1` for `obj_id`)
/// and return it as `to_jsonb(snapshot)`.
pub async fn load_table_snapshot(
    pool: &PgPool,
    table_name: &str,
    predicate: &str,
    obj_id: &str,
) -> Result<serde_json::Value, DomainError> {
    let sql = format!(
        "SELECT to_jsonb(snapshot) AS data FROM (SELECT * FROM {table_name} WHERE {predicate} LIMIT 1) snapshot"
    );
    let row = sqlx::query(&sql)
        .bind(obj_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or_else(|| DomainError::Internal(format!("{table_name} snapshot not found: {obj_id}")))?;
    row.try_get::<serde_json::Value, _>("data")
        .map_err(|e| DomainError::Internal(e.to_string()))
}
