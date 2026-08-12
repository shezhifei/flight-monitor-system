//! 软删除审计写入（审计合规：业务数据禁止物理删除）。
//!
//! 每个软删除动作生效后，向 `system_audit_logs`（迁移 005）写入一条
//! 审计记录，`action` 固定为 `soft_delete`（users 停用为 `deactivate`）。
//! 仓储层无法感知操作者，`user_id` 留空；审计写入为 best-effort，
//! 失败仅记日志，不阻断删除主流程（删除本身已持久化）。

use sqlx::PgPool;

/// 写入一条软删除审计记录（best-effort）。
pub(crate) async fn record_soft_delete(pool: &PgPool, entity_type: &str, entity_id: &str, action: &str) {
    let result = sqlx::query(
        "INSERT INTO system_audit_logs (entity_type, entity_id, action, changes, created_at) \
         VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(action)
    .bind(serde_json::json!({"reason": "soft_delete"}))
    .execute(pool)
    .await;
    if let Err(error) = result {
        tracing::error!(
            entity_type,
            entity_id,
            action,
            %error,
            "failed to write soft-delete audit log"
        );
    }
}
