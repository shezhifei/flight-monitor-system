//! PostgreSQL 通知仓储实现

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};

use fms_domain::error::DomainError;
use fms_domain::models::notification::{Notification, NotificationPreference};
use fms_domain::ports::notification_repository::{
    NotificationPreferenceRepository, NotificationRepository, NotificationTransactionalRepository,
};

pub struct PgNotificationRepository {
    pool: PgPool,
}

impl PgNotificationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotificationRepository for PgNotificationRepository {
    async fn save(&self, n: &Notification) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO notifications (
                notification_id, user_id, title, body, category, severity,
                flight_id, dispatch_order_id, group_id, event_id,
                sender_user_id, sender_username_snapshot,
                origin_type, receipt_required, receipt_group_id,
                delivery_status, delivered_at, is_read, ack_status, ack_at, ack_note,
                related_entity_type, related_entity_id, created_at, read_at,
                recipient_username_snapshot, recipient_display_name_snapshot,
                recipient_department_snapshot, recipient_job_title_snapshot
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,
                -- 发送时刻定格接收人快照（migration 100 的设计意图）；
                -- 调用方未显式提供时从 users 表取当前值
                COALESCE($26, (SELECT username FROM users WHERE id = $2)),
                COALESCE($27, (SELECT display_name FROM users WHERE id = $2)),
                COALESCE($28, (SELECT department FROM users WHERE id = $2)),
                COALESCE($29, (SELECT job_title FROM users WHERE id = $2))
            )
            ON CONFLICT (notification_id) DO UPDATE SET
                title = EXCLUDED.title,
                body = EXCLUDED.body,
                category = EXCLUDED.category,
                severity = EXCLUDED.severity,
                is_read = EXCLUDED.is_read,
                delivery_status = EXCLUDED.delivery_status,
                delivered_at = EXCLUDED.delivered_at,
                ack_status = EXCLUDED.ack_status,
                ack_at = EXCLUDED.ack_at,
                ack_note = EXCLUDED.ack_note,
                related_entity_type = EXCLUDED.related_entity_type,
                related_entity_id = EXCLUDED.related_entity_id,
                updated_at = NOW(),
                read_at = EXCLUDED.read_at"#,
        )
        .bind(&n.notification_id)
        .bind(&n.user_id)
        .bind(&n.title)
        .bind(&n.body)
        .bind(&n.category)
        .bind(&n.severity)
        .bind(&n.flight_id)
        .bind(&n.dispatch_order_id)
        .bind(&n.group_id)
        .bind(&n.event_id)
        .bind(&n.sender_user_id)
        .bind(&n.sender_username_snapshot)
        .bind(&n.origin_type)
        .bind(n.receipt_required)
        .bind(&n.receipt_group_id)
        .bind(&n.delivery_status)
        .bind(n.delivered_at)
        .bind(n.is_read)
        .bind(&n.ack_status)
        .bind(n.ack_at)
        .bind(&n.ack_note)
        .bind(&n.related_entity_type)
        .bind(&n.related_entity_id)
        .bind(n.created_at)
        .bind(n.read_at)
        .bind(&n.recipient_username_snapshot)
        .bind(&n.recipient_display_name_snapshot)
        .bind(&n.recipient_department_snapshot)
        .bind(&n.recipient_job_title_snapshot)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, notification_id: &str) -> Result<Option<Notification>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                n.notification_id, n.user_id, n.title, n.body, n.category, n.severity,
                n.is_read, n.flight_id,
                n.related_entity_type, n.related_entity_id,
                n.dispatch_order_id, n.group_id, n.event_id,
                n.sender_user_id, n.sender_username_snapshot,
                n.origin_type, n.receipt_required, n.receipt_group_id,
                n.delivery_status, n.delivered_at,
                n.ack_status, n.ack_at, n.ack_note,
                n.created_at, n.read_at,
                recipient.username AS recipient_username,
                recipient.display_name AS recipient_display_name,
                recipient.department AS recipient_department,
                recipient.job_title AS recipient_job_title
            FROM notifications n
            LEFT JOIN users recipient ON recipient.id = n.user_id
            WHERE n.notification_id = $1
            "#,
        )
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_notification(&r)))
    }

    async fn find_by_id_for_user(
        &self,
        notification_id: &str,
        user_id: &str,
    ) -> Result<Option<Notification>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                n.notification_id, n.user_id, n.title, n.body, n.category, n.severity,
                n.is_read, n.flight_id,
                n.related_entity_type, n.related_entity_id,
                n.dispatch_order_id, n.group_id, n.event_id,
                n.sender_user_id, n.sender_username_snapshot,
                n.origin_type, n.receipt_required, n.receipt_group_id,
                n.delivery_status, n.delivered_at,
                n.ack_status, n.ack_at, n.ack_note,
                n.created_at, n.read_at,
                recipient.username AS recipient_username,
                recipient.display_name AS recipient_display_name,
                recipient.department AS recipient_department,
                recipient.job_title AS recipient_job_title
            FROM notifications n
            LEFT JOIN users recipient ON recipient.id = n.user_id
            WHERE n.notification_id = $1
              AND n.user_id = $2
            "#,
        )
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|r| row_to_notification(&r)))
    }

    async fn find_by_user(
        &self,
        user_id: &str,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        let sql = if unread_only {
            "SELECT notification_id, user_id, title, body, category, severity, \
             is_read, flight_id, \
             related_entity_type, related_entity_id, \
             dispatch_order_id, group_id, event_id, \
             sender_user_id, sender_username_snapshot, \
             origin_type, receipt_required, receipt_group_id, \
             delivery_status, delivered_at, \
             ack_status, ack_at, ack_note, \
             created_at, read_at \
             FROM notifications WHERE user_id = $1 AND is_read = FALSE \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        } else {
            "SELECT notification_id, user_id, title, body, category, severity, \
             is_read, flight_id, \
             related_entity_type, related_entity_id, \
             dispatch_order_id, group_id, event_id, \
             sender_user_id, sender_username_snapshot, \
             origin_type, receipt_required, receipt_group_id, \
             delivery_status, delivered_at, \
             ack_status, ack_at, ack_note, \
             created_at, read_at \
             FROM notifications WHERE user_id = $1 \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        };
        let rows = sqlx::query(sql)
            .bind(user_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_notification).collect())
    }

    async fn mark_read(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError> {
        let result = sqlx::query(
            "UPDATE notifications SET is_read = TRUE, read_at = $1 WHERE notification_id = $2 AND user_id = $3 AND is_read = FALSE",
        )
        .bind(Utc::now())
        .bind(notification_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn mark_delivered(&self, notification_id: &str, user_id: &str) -> Result<bool, DomainError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET delivery_status = 'delivered',
                delivered_at = COALESCE(delivered_at, $1)
            WHERE notification_id = $2
              AND user_id = $3
              AND (delivery_status IS DISTINCT FROM 'delivered' OR delivered_at IS NULL)
            "#,
        )
        .bind(now)
        .bind(notification_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn mark_all_read(&self, user_id: &str) -> Result<i64, DomainError> {
        let result = sqlx::query(
            "UPDATE notifications SET is_read = TRUE, read_at = $1 WHERE user_id = $2 AND is_read = FALSE AND COALESCE(receipt_required, FALSE) = FALSE",
        )
        .bind(Utc::now())
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(result.rows_affected() as i64)
    }

    async fn count_unread(&self, user_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM notifications WHERE user_id = $1 AND is_read = FALSE")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.get::<i64, _>("cnt"))
    }

    async fn acknowledge(
        &self,
        notification_id: &str,
        user_id: &str,
        action: &str,
        note: Option<&str>,
    ) -> Result<Option<Notification>, DomainError> {
        let row = sqlx::query(
            r#"
            WITH updated AS (
                UPDATE notifications
                SET ack_status = $1,
                    ack_note = $2,
                    ack_at = NOW(),
                    is_read = TRUE,
                    read_at = COALESCE(read_at, NOW())
                WHERE notification_id = $3
                  AND user_id = $4
                  AND ack_status = 'pending'
                RETURNING notification_id, user_id, title, body, category, severity,
                          is_read, flight_id,
                          related_entity_type, related_entity_id,
                          dispatch_order_id, group_id, event_id,
                          sender_user_id, sender_username_snapshot,
                          origin_type, receipt_required, receipt_group_id,
                          delivery_status, delivered_at,
                          ack_status, ack_at, ack_note,
                          created_at, read_at
            )
            SELECT
                updated.notification_id, updated.user_id,
                updated.title, updated.body, updated.category, updated.severity,
                updated.is_read, updated.flight_id,
                updated.related_entity_type, updated.related_entity_id,
                updated.dispatch_order_id, updated.group_id, updated.event_id,
                updated.sender_user_id, updated.sender_username_snapshot,
                updated.origin_type, updated.receipt_required, updated.receipt_group_id,
                updated.delivery_status, updated.delivered_at,
                updated.ack_status, updated.ack_at, updated.ack_note,
                updated.created_at, updated.read_at,
                recipient.username AS recipient_username,
                recipient.display_name AS recipient_display_name,
                recipient.department AS recipient_department,
                recipient.job_title AS recipient_job_title
            FROM updated
            LEFT JOIN users recipient ON recipient.id = updated.user_id
            "#,
        )
        .bind(action)
        .bind(note)
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|value| row_to_notification(&value)))
    }

    async fn find_by_receipt_group(&self, receipt_group_id: &str) -> Result<Vec<Notification>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                n.notification_id, n.user_id, n.title, n.body, n.category, n.severity,
                n.is_read, n.flight_id,
                n.related_entity_type, n.related_entity_id,
                n.dispatch_order_id, n.group_id, n.event_id,
                n.sender_user_id, n.sender_username_snapshot,
                n.origin_type, n.receipt_required, n.receipt_group_id,
                n.delivery_status, n.delivered_at,
                n.ack_status, n.ack_at, n.ack_note,
                n.created_at, n.read_at,
                recipient.username AS recipient_username,
                recipient.display_name AS recipient_display_name,
                recipient.department AS recipient_department,
                recipient.job_title AS recipient_job_title
            FROM notifications n
            LEFT JOIN users recipient ON recipient.id = n.user_id
            WHERE n.receipt_group_id = $1
            ORDER BY n.created_at ASC, n.notification_id ASC
            "#,
        )
        .bind(receipt_group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(rows.iter().map(row_to_notification).collect())
    }

    async fn summarize_receipt_group(&self, receipt_group_id: &str) -> Result<Option<Value>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                MIN(title) AS title,
                MIN(severity) AS severity,
                MIN(flight_id) AS flight_id,
                MIN(dispatch_order_id) AS dispatch_order_id,
                MIN(group_id) AS group_id,
                MIN(created_at) AS created_at,
                MIN(sender_user_id) AS sender_user_id,
                MIN(sender_username_snapshot) AS sender_username,
                MIN(origin_type) AS origin_type,
                BOOL_OR(receipt_required) AS receipt_required,
                COUNT(*) AS total_count,
                COUNT(*) FILTER (WHERE ack_status = 'pending') AS pending_count,
                COUNT(*) FILTER (WHERE ack_status = 'acknowledged') AS acknowledged_count,
                COUNT(*) FILTER (WHERE ack_status = 'rejected') AS rejected_count,
                MAX(COALESCE(ack_at, read_at, delivered_at, created_at)) AS latest_updated_at
            FROM notifications
            WHERE receipt_group_id = $1
            GROUP BY receipt_group_id
            "#,
        )
        .bind(receipt_group_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(|value| {
            json!({
                "title": value.try_get::<Option<String>, _>("title").ok().flatten(),
                "severity": value.try_get::<Option<String>, _>("severity").ok().flatten(),
                "flight_id": value.try_get::<Option<String>, _>("flight_id").ok().flatten(),
                "dispatch_order_id": value.try_get::<Option<String>, _>("dispatch_order_id").ok().flatten(),
                "group_id": value.try_get::<Option<String>, _>("group_id").ok().flatten(),
                "created_at": value.try_get::<Option<chrono::DateTime<Utc>>, _>("created_at").ok().flatten(),
                "sender_user_id": value.try_get::<Option<String>, _>("sender_user_id").ok().flatten(),
                "sender_username": value.try_get::<Option<String>, _>("sender_username").ok().flatten(),
                "origin_type": normalize_origin_type(value.try_get::<Option<String>, _>("origin_type").ok().flatten().as_deref()),
                "receipt_required": value.try_get::<Option<bool>, _>("receipt_required").ok().flatten().unwrap_or(true),
                "total_count": value.try_get::<i64, _>("total_count").unwrap_or(0),
                "pending_count": value.try_get::<i64, _>("pending_count").unwrap_or(0),
                "acknowledged_count": value.try_get::<i64, _>("acknowledged_count").unwrap_or(0),
                "rejected_count": value.try_get::<i64, _>("rejected_count").unwrap_or(0),
                "latest_updated_at": value.try_get::<Option<chrono::DateTime<Utc>>, _>("latest_updated_at").ok().flatten(),
            })
        }))
    }

    async fn list_sent_receipt_groups(
        &self,
        sender_user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                receipt_group_id,
                MIN(title) AS title,
                MIN(severity) AS severity,
                MIN(flight_id) AS flight_id,
                MIN(dispatch_order_id) AS dispatch_order_id,
                MIN(group_id) AS group_id,
                MIN(created_at) AS created_at,
                MIN(origin_type) AS origin_type,
                COUNT(*)::BIGINT AS total_count,
                COUNT(*) FILTER (WHERE ack_status = 'pending')::BIGINT AS pending_count,
                COUNT(*) FILTER (WHERE ack_status = 'acknowledged')::BIGINT AS acknowledged_count,
                COUNT(*) FILTER (WHERE ack_status = 'rejected')::BIGINT AS rejected_count,
                MAX(COALESCE(ack_at, read_at, delivered_at, created_at)) AS latest_updated_at,
                COUNT(*) OVER()::BIGINT AS matched_groups
            FROM notifications
            WHERE sender_user_id = $1
              AND receipt_group_id IS NOT NULL
              AND receipt_required = TRUE
            GROUP BY receipt_group_id
            ORDER BY MIN(created_at) DESC, receipt_group_id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(sender_user_id)
        .bind(limit.max(1))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|value| {
                json!({
                    "receipt_group_id": value.try_get::<Option<String>, _>("receipt_group_id").ok().flatten(),
                    "title": value.try_get::<Option<String>, _>("title").ok().flatten(),
                    "severity": value.try_get::<Option<String>, _>("severity").ok().flatten(),
                    "flight_id": value.try_get::<Option<String>, _>("flight_id").ok().flatten(),
                    "dispatch_order_id": value.try_get::<Option<String>, _>("dispatch_order_id").ok().flatten(),
                    "group_id": value.try_get::<Option<String>, _>("group_id").ok().flatten(),
                    "created_at": value.try_get::<Option<chrono::DateTime<Utc>>, _>("created_at").ok().flatten(),
                    "origin_type": normalize_origin_type(value.try_get::<Option<String>, _>("origin_type").ok().flatten().as_deref()),
                    "total_count": value.try_get::<i64, _>("total_count").unwrap_or(0),
                    "pending_count": value.try_get::<i64, _>("pending_count").unwrap_or(0),
                    "acknowledged_count": value.try_get::<i64, _>("acknowledged_count").unwrap_or(0),
                    "rejected_count": value.try_get::<i64, _>("rejected_count").unwrap_or(0),
                    "latest_updated_at": value.try_get::<Option<chrono::DateTime<Utc>>, _>("latest_updated_at").ok().flatten(),
                    "matched_groups": value.try_get::<i64, _>("matched_groups").unwrap_or(0),
                })
            })
            .collect())
    }
}

#[async_trait]
impl<'tx> NotificationTransactionalRepository<Transaction<'tx, Postgres>> for PgNotificationRepository {
    async fn save_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, n: &Notification) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO notifications (
                notification_id, user_id, title, body, category, severity,
                flight_id, dispatch_order_id, group_id, event_id,
                sender_user_id, sender_username_snapshot,
                origin_type, receipt_required, receipt_group_id,
                delivery_status, delivered_at, is_read, ack_status, ack_at, ack_note,
                related_entity_type, related_entity_id, created_at, read_at,
                recipient_username_snapshot, recipient_display_name_snapshot,
                recipient_department_snapshot, recipient_job_title_snapshot
            ) VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,
                -- 发送时刻定格接收人快照；调用方未显式提供时从 users 表取当前值
                COALESCE($26, (SELECT username FROM users WHERE id = $2)),
                COALESCE($27, (SELECT display_name FROM users WHERE id = $2)),
                COALESCE($28, (SELECT department FROM users WHERE id = $2)),
                COALESCE($29, (SELECT job_title FROM users WHERE id = $2))
            )
            ON CONFLICT (notification_id) DO UPDATE SET
                title = EXCLUDED.title,
                body = EXCLUDED.body,
                category = EXCLUDED.category,
                severity = EXCLUDED.severity,
                dispatch_order_id = EXCLUDED.dispatch_order_id,
                group_id = EXCLUDED.group_id,
                event_id = EXCLUDED.event_id,
                sender_user_id = EXCLUDED.sender_user_id,
                sender_username_snapshot = EXCLUDED.sender_username_snapshot,
                origin_type = EXCLUDED.origin_type,
                receipt_required = EXCLUDED.receipt_required,
                receipt_group_id = EXCLUDED.receipt_group_id,
                delivery_status = EXCLUDED.delivery_status,
                delivered_at = EXCLUDED.delivered_at,
                is_read = EXCLUDED.is_read,
                ack_status = EXCLUDED.ack_status,
                ack_at = EXCLUDED.ack_at,
                ack_note = EXCLUDED.ack_note,
                related_entity_type = EXCLUDED.related_entity_type,
                related_entity_id = EXCLUDED.related_entity_id,
                read_at = EXCLUDED.read_at"#,
        )
        .bind(&n.notification_id)
        .bind(&n.user_id)
        .bind(&n.title)
        .bind(&n.body)
        .bind(&n.category)
        .bind(&n.severity)
        .bind(&n.flight_id)
        .bind(&n.dispatch_order_id)
        .bind(&n.group_id)
        .bind(&n.event_id)
        .bind(&n.sender_user_id)
        .bind(&n.sender_username_snapshot)
        .bind(&n.origin_type)
        .bind(n.receipt_required)
        .bind(&n.receipt_group_id)
        .bind(&n.delivery_status)
        .bind(n.delivered_at)
        .bind(n.is_read)
        .bind(&n.ack_status)
        .bind(n.ack_at)
        .bind(&n.ack_note)
        .bind(&n.related_entity_type)
        .bind(&n.related_entity_id)
        .bind(n.created_at)
        .bind(n.read_at)
        .bind(&n.recipient_username_snapshot)
        .bind(&n.recipient_display_name_snapshot)
        .bind(&n.recipient_department_snapshot)
        .bind(&n.recipient_job_title_snapshot)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl NotificationPreferenceRepository for PgNotificationRepository {
    async fn find_by_user(&self, user_id: &str) -> Result<Option<NotificationPreference>, DomainError> {
        let row = sqlx::query("SELECT * FROM notification_preferences WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(row.map(|value| row_to_preference(&value)))
    }

    async fn save(&self, pref: &NotificationPreference) -> Result<(), DomainError> {
        let category_overrides = serde_json::to_value(&pref.category_overrides).unwrap_or_else(|_| json!({}));
        sqlx::query(
            r#"
            INSERT INTO notification_preferences (
                user_id, in_app_enabled, external_enabled, external_channel,
                mute_start, mute_end, critical_override, category_overrides, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT (user_id) DO UPDATE SET
                in_app_enabled = EXCLUDED.in_app_enabled,
                external_enabled = EXCLUDED.external_enabled,
                external_channel = EXCLUDED.external_channel,
                mute_start = EXCLUDED.mute_start,
                mute_end = EXCLUDED.mute_end,
                critical_override = EXCLUDED.critical_override,
                category_overrides = EXCLUDED.category_overrides,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(&pref.user_id)
        .bind(pref.in_app_enabled)
        .bind(pref.external_enabled)
        .bind(&pref.external_channel)
        .bind(&pref.mute_start)
        .bind(&pref.mute_end)
        .bind(pref.critical_override)
        .bind(category_overrides)
        .bind(pref.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
}

fn row_to_notification(r: &sqlx::postgres::PgRow) -> Notification {
    Notification {
        notification_id: r.get("notification_id"),
        user_id: r.get("user_id"),
        title: r.get("title"),
        body: r.get("body"),
        category: r.get("category"),
        severity: r.get("severity"),
        is_read: r.get("is_read"),
        flight_id: r.get("flight_id"),
        related_entity_type: r.get("related_entity_type"),
        related_entity_id: r.get("related_entity_id"),
        dispatch_order_id: r.get("dispatch_order_id"),
        group_id: r.get("group_id"),
        event_id: r.get("event_id"),
        sender_user_id: r.get("sender_user_id"),
        sender_username_snapshot: r.get("sender_username_snapshot"),
        recipient_username_snapshot: r.try_get("recipient_username").ok().flatten(),
        recipient_display_name_snapshot: r.try_get("recipient_display_name").ok().flatten(),
        recipient_department_snapshot: r.try_get("recipient_department").ok().flatten(),
        recipient_job_title_snapshot: r.try_get("recipient_job_title").ok().flatten(),
        origin_type: normalize_origin_type(r.get::<Option<String>, _>("origin_type").as_deref()),
        receipt_required: r.get::<Option<bool>, _>("receipt_required").unwrap_or(false),
        receipt_group_id: r.get("receipt_group_id"),
        delivery_status: r
            .get::<Option<String>, _>("delivery_status")
            .unwrap_or_else(|| "sent".into()),
        delivered_at: r.get("delivered_at"),
        ack_status: r
            .get::<Option<String>, _>("ack_status")
            .unwrap_or_else(|| "pending".into()),
        ack_at: r.get("ack_at"),
        ack_note: r.get("ack_note"),
        created_at: r.get("created_at"),
        read_at: r.get("read_at"),
    }
}

fn normalize_origin_type(value: Option<&str>) -> String {
    if value.unwrap_or("manual").trim().eq_ignore_ascii_case("workflow") {
        "workflow".to_string()
    } else {
        "manual".to_string()
    }
}

fn row_to_preference(r: &sqlx::postgres::PgRow) -> NotificationPreference {
    let raw_overrides = r
        .get::<Option<serde_json::Value>, _>("category_overrides")
        .unwrap_or_else(|| json!({}));
    let category_overrides = raw_overrides
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| value.as_bool().map(|flag| (key.clone(), flag)))
                .collect()
        })
        .unwrap_or_default();
    NotificationPreference {
        user_id: r.get("user_id"),
        in_app_enabled: r.get("in_app_enabled"),
        external_enabled: r.get("external_enabled"),
        external_channel: r
            .get::<Option<String>, _>("external_channel")
            .unwrap_or_else(|| "none".into()),
        mute_start: r.get("mute_start"),
        mute_end: r.get("mute_end"),
        critical_override: r.get("critical_override"),
        category_overrides,
        updated_at: r.get("updated_at"),
    }
}
