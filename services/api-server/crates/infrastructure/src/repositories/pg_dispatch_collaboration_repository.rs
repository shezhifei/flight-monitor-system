//! PostgreSQL 派工协作 / 聊天仓储实现。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch_collaboration::{
    DispatchChatDispatcherCandidate, DispatchChatGroupList, DispatchChatGroupSummary, DispatchChatMember,
    DispatchChatMemberUpsert, DispatchChatMessage, DispatchChatMessageList, DispatchChatUserProfile,
    DispatchCollaborationEvent, NewDispatchChatMessage, NotificationReceiptSummary,
};
use fms_domain::models::notification::Notification;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;

pub struct PgDispatchCollaborationRepository {
    pool: PgPool,
}

impl PgDispatchCollaborationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DispatchCollaborationRepository for PgDispatchCollaborationRepository {
    async fn get_group_by_id(&self, group_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM dispatch_chat_groups g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE g.group_id = $1
            LIMIT 1
            "#,
        )
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_group_summary))
    }

    async fn get_group_for_user(
        &self,
        group_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                g.*,
                m.is_active AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                GREATEST(COALESCE(last_msg.seq_no, 0) - COALESCE(m.last_read_seq, 0), 0) AS unread_count
            FROM dispatch_chat_group_members m
            JOIN dispatch_chat_groups g ON g.group_id = m.group_id
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE m.user_id = $1
              AND m.group_id = $2
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_group_summary))
    }

    async fn get_group_for_user_by_flight(
        &self,
        flight_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                g.*,
                m.is_active AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                GREATEST(COALESCE(last_msg.seq_no, 0) - COALESCE(m.last_read_seq, 0), 0) AS unread_count
            FROM dispatch_chat_group_members m
            JOIN dispatch_chat_groups g ON g.group_id = m.group_id
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE m.user_id = $1
              AND g.flight_id = $2
              AND g.channel_type = 'system_flight_dispatch'
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(flight_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_group_summary))
    }

    async fn get_group_by_flight(&self, flight_id: &str) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM dispatch_chat_groups g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE g.flight_id = $1
              AND g.channel_type = 'system_flight_dispatch'
            LIMIT 1
            "#,
        )
        .bind(flight_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_group_summary))
    }

    async fn list_user_groups(
        &self,
        user_id: &str,
        status: &str,
        limit: i64,
        offset: i64,
    ) -> Result<DispatchChatGroupList, DomainError> {
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let status_clause = match status {
            "active" => " AND g.status = 'active' ",
            "archived" => " AND g.status = 'archived' ",
            _ => "",
        };

        let list_sql = format!(
            r#"
            SELECT
                g.*,
                m.is_active AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                GREATEST(COALESCE(last_msg.seq_no, 0) - COALESCE(m.last_read_seq, 0), 0) AS unread_count
            FROM dispatch_chat_group_members m
            JOIN dispatch_chat_groups g ON g.group_id = m.group_id
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE m.user_id = $1
              AND m.is_active = TRUE
              {status_clause}
            ORDER BY COALESCE(last_msg.sent_at, g.updated_at, g.created_at) DESC
            LIMIT $2 OFFSET $3
            "#
        );
        let count_sql = format!(
            r#"
            SELECT COUNT(*)::BIGINT AS total
            FROM dispatch_chat_group_members m
            JOIN dispatch_chat_groups g ON g.group_id = m.group_id
            WHERE m.user_id = $1
              AND m.is_active = TRUE
              {status_clause}
            "#
        );

        let rows = sqlx::query(&list_sql)
            .bind(user_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(internal_error)?;
        let total_row = sqlx::query(&count_sql)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await
            .map_err(internal_error)?;

        Ok(DispatchChatGroupList {
            items: rows.iter().map(row_to_group_summary).collect(),
            total: total_row.get::<i64, _>("total"),
            limit,
            offset,
            unread_total: 0,
        })
    }

    async fn list_group_messages(
        &self,
        group_id: &str,
        limit: i64,
        before_seq: Option<i64>,
    ) -> Result<DispatchChatMessageList, DomainError> {
        let limit = limit.clamp(1, 200);
        let (query_sql, count_sql, query_rows, total) = if let Some(before_seq) = before_seq {
            let rows = sqlx::query(
                r#"
                SELECT m.*, u.username AS sender_username
                FROM dispatch_chat_messages m
                LEFT JOIN users u ON u.id = m.sender_user_id
                WHERE m.group_id = $1
                  AND m.seq_no < $2
                ORDER BY m.seq_no DESC
                LIMIT $3
                "#,
            )
            .bind(group_id)
            .bind(before_seq)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(internal_error)?;
            let total_row = sqlx::query(
                r#"
                SELECT COUNT(*)::BIGINT AS total
                FROM dispatch_chat_messages
                WHERE group_id = $1
                  AND seq_no < $2
                "#,
            )
            .bind(group_id)
            .bind(before_seq)
            .fetch_one(&self.pool)
            .await
            .map_err(internal_error)?;
            ("", "", rows, total_row.get::<i64, _>("total"))
        } else {
            let rows = sqlx::query(
                r#"
                SELECT m.*, u.username AS sender_username
                FROM dispatch_chat_messages m
                LEFT JOIN users u ON u.id = m.sender_user_id
                WHERE m.group_id = $1
                ORDER BY m.seq_no DESC
                LIMIT $2
                "#,
            )
            .bind(group_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(internal_error)?;
            let total_row = sqlx::query(
                r#"
                SELECT COUNT(*)::BIGINT AS total
                FROM dispatch_chat_messages
                WHERE group_id = $1
                "#,
            )
            .bind(group_id)
            .fetch_one(&self.pool)
            .await
            .map_err(internal_error)?;
            ("", "", rows, total_row.get::<i64, _>("total"))
        };
        let _ = query_sql;
        let _ = count_sql;

        let mut items_desc = query_rows.iter().map(row_to_message).collect::<Vec<_>>();
        items_desc.reverse();
        let has_more = total > items_desc.len() as i64;
        let next_before_seq = if has_more {
            items_desc.first().map(|item| item.seq_no)
        } else {
            None
        };

        Ok(DispatchChatMessageList {
            items: items_desc,
            total,
            limit,
            before_seq,
            has_more,
            next_before_seq,
        })
    }

    async fn insert_message(&self, message: &NewDispatchChatMessage) -> Result<DispatchChatMessage, DomainError> {
        let row = sqlx::query(
            r#"
            WITH inserted AS (
                INSERT INTO dispatch_chat_messages (
                    message_id,
                    group_id,
                    sender_user_id,
                    dispatch_order_id,
                    event_id,
                    message_type,
                    content,
                    is_at_all,
                    metadata
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb)
                RETURNING *
            )
            SELECT i.*, u.username AS sender_username
            FROM inserted i
            LEFT JOIN users u ON u.id = i.sender_user_id
            "#,
        )
        .bind(&message.message_id)
        .bind(&message.group_id)
        .bind(&message.sender_user_id)
        .bind(&message.dispatch_order_id)
        .bind(&message.event_id)
        .bind(&message.message_type)
        .bind(&message.content)
        .bind(message.is_at_all)
        .bind(&message.metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row_to_message(&row))
    }

    async fn update_message_event_id(
        &self,
        message_id: &str,
        event_id: &str,
    ) -> Result<Option<DispatchChatMessage>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE dispatch_chat_messages m
            SET event_id = $1
            WHERE m.message_id = $2
            RETURNING m.*, NULL::VARCHAR AS sender_username
            "#,
        )
        .bind(event_id)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_message))
    }

    async fn mark_group_read(
        &self,
        group_id: &str,
        user_id: &str,
        read_seq: i64,
    ) -> Result<Option<DispatchChatMember>, DomainError> {
        let row = sqlx::query(
            r#"
            UPDATE dispatch_chat_group_members m
            SET
                last_read_seq = GREATEST(last_read_seq, $1),
                last_read_at = CURRENT_TIMESTAMP
            FROM users u
            WHERE m.group_id = $2
              AND m.user_id = $3
              AND m.is_active = TRUE
              AND u.id = m.user_id
            RETURNING m.*, u.username
            "#,
        )
        .bind(read_seq.max(0))
        .bind(group_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_member))
    }

    async fn get_group_latest_seq(&self, group_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(MAX(seq_no), 0)::BIGINT AS latest_seq
            FROM dispatch_chat_messages
            WHERE group_id = $1
            "#,
        )
        .bind(group_id)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.get::<i64, _>("latest_seq"))
    }

    async fn count_group_unread(&self, group_id: &str, user_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                GREATEST(COALESCE(last_msg.max_seq, 0) - COALESCE(m.last_read_seq, 0), 0)::BIGINT AS unread_count
            FROM dispatch_chat_group_members m
            LEFT JOIN LATERAL (
                SELECT MAX(seq_no) AS max_seq
                FROM dispatch_chat_messages
                WHERE group_id = m.group_id
            ) last_msg ON TRUE
            WHERE m.group_id = $1
              AND m.user_id = $2
              AND m.is_active = TRUE
            LIMIT 1
            "#,
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.map(|row| row.get::<i64, _>("unread_count")).unwrap_or(0))
    }

    async fn count_total_unread(&self, user_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(
                SUM(GREATEST(COALESCE(last_msg.max_seq, 0) - COALESCE(m.last_read_seq, 0), 0)),
                0
            )::BIGINT AS unread_total
            FROM dispatch_chat_group_members m
            LEFT JOIN LATERAL (
                SELECT MAX(seq_no) AS max_seq
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = m.group_id
            ) last_msg ON TRUE
            WHERE m.user_id = $1
              AND m.is_active = TRUE
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.get::<i64, _>("unread_total"))
    }

    async fn find_active_members(&self, group_id: &str) -> Result<Vec<DispatchChatMember>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                m.*,
                u.username
            FROM dispatch_chat_group_members m
            LEFT JOIN users u ON u.id = m.user_id
            WHERE m.group_id = $1
              AND m.is_active = TRUE
            ORDER BY m.joined_at ASC
            "#,
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_member).collect())
    }

    async fn find_users_by_ids(&self, user_ids: &[String]) -> Result<Vec<DispatchChatUserProfile>, DomainError> {
        let normalized_user_ids = normalized_values(user_ids);
        if normalized_user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT
                u.id AS user_id,
                u.username,
                u.department,
                u.job_title,
                u.is_active
            FROM users u
            WHERE u.id = ANY($1)
            "#,
        )
        .bind(&normalized_user_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        let mut row_map = std::collections::HashMap::new();
        for row in rows {
            let profile = row_to_user_profile(&row);
            row_map.insert(profile.user_id.clone(), profile);
        }

        Ok(normalized_user_ids
            .into_iter()
            .filter_map(|user_id| row_map.remove(&user_id))
            .collect())
    }

    async fn find_dispatchers_by_departments(
        &self,
        departments: &[String],
    ) -> Result<Vec<DispatchChatDispatcherCandidate>, DomainError> {
        let normalized_departments = normalized_values(departments);
        if normalized_departments.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT DISTINCT
                u.id AS user_id,
                u.username,
                u.department,
                u.job_title
            FROM users u
            WHERE u.is_active = TRUE
              AND u.department = ANY($1)
              AND (
                u.job_title IN ('调度员', '主管')
                OR EXISTS (
                    SELECT 1
                    FROM user_roles ur
                    JOIN role_permissions rp ON rp.role_id = ur.role_id
                    JOIN permissions p ON p.id = rp.permission_id
                    WHERE ur.user_id = u.id
                      AND p.name = 'dispatch:manage'
                )
              )
            ORDER BY u.username ASC, u.id ASC
            "#,
        )
        .bind(&normalized_departments)
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_dispatcher_candidate).collect())
    }

    async fn upsert_group_for_flight(
        &self,
        flight_id: &str,
        group_name: &str,
        archive_at: Option<DateTime<Utc>>,
        metadata: &serde_json::Value,
    ) -> Result<DispatchChatGroupSummary, DomainError> {
        let row = sqlx::query(
            r#"
            WITH upserted AS (
                INSERT INTO dispatch_chat_groups (
                    group_id,
                    channel_type,
                    flight_id,
                    group_name,
                    status,
                    read_only,
                    archive_at,
                    metadata
                ) VALUES (
                    $1,
                    'system_flight_dispatch',
                    $2,
                    $3,
                    'active',
                    FALSE,
                    $4,
                    $5::jsonb
                )
                ON CONFLICT (channel_type, flight_id) DO UPDATE SET
                    group_name = EXCLUDED.group_name,
                    archive_at = COALESCE(EXCLUDED.archive_at, dispatch_chat_groups.archive_at),
                    metadata = dispatch_chat_groups.metadata || EXCLUDED.metadata,
                    status = CASE
                        WHEN dispatch_chat_groups.status = 'archived' THEN dispatch_chat_groups.status
                        ELSE 'active'
                    END,
                    read_only = CASE
                        WHEN dispatch_chat_groups.status = 'archived' THEN dispatch_chat_groups.read_only
                        ELSE FALSE
                    END,
                    updated_at = CURRENT_TIMESTAMP
                RETURNING *
            )
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM upserted g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            "#,
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(flight_id)
        .bind(group_name)
        .bind(archive_at)
        .bind(metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row_to_group_summary(&row))
    }

    async fn upsert_group_memberships(
        &self,
        group_id: &str,
        memberships: &[DispatchChatMemberUpsert],
    ) -> Result<(), DomainError> {
        for membership in memberships {
            sqlx::query(
                r#"
                INSERT INTO dispatch_chat_group_members (
                    id,
                    group_id,
                    user_id,
                    is_assignee,
                    is_dispatcher,
                    is_active,
                    last_read_seq,
                    last_read_at
                ) VALUES (
                    $1, $2, $3, $4, $5, TRUE, $6, $7
                )
                ON CONFLICT (group_id, user_id) DO UPDATE SET
                    is_assignee = dispatch_chat_group_members.is_assignee OR EXCLUDED.is_assignee,
                    is_dispatcher = dispatch_chat_group_members.is_dispatcher OR EXCLUDED.is_dispatcher,
                    is_active = TRUE,
                    left_at = NULL,
                    last_read_seq = CASE
                        WHEN dispatch_chat_group_members.is_active = TRUE THEN GREATEST(
                            dispatch_chat_group_members.last_read_seq,
                            EXCLUDED.last_read_seq
                        )
                        ELSE EXCLUDED.last_read_seq
                    END,
                    last_read_at = CASE
                        WHEN dispatch_chat_group_members.is_active = TRUE
                             AND EXCLUDED.last_read_seq <= dispatch_chat_group_members.last_read_seq
                            THEN dispatch_chat_group_members.last_read_at
                        ELSE EXCLUDED.last_read_at
                    END
                "#,
            )
            .bind(ulid::Ulid::new().to_string())
            .bind(group_id)
            .bind(&membership.user_id)
            .bind(membership.is_assignee)
            .bind(membership.is_dispatcher)
            .bind(membership.last_read_seq.max(0))
            .bind(membership.last_read_at)
            .execute(&self.pool)
            .await
            .map_err(internal_error)?;
        }

        Ok(())
    }

    async fn deactivate_members_except(
        &self,
        group_id: &str,
        active_user_ids: &[String],
    ) -> Result<Vec<DispatchChatMember>, DomainError> {
        let rows = if active_user_ids.is_empty() {
            sqlx::query(
                r#"
                UPDATE dispatch_chat_group_members m
                SET is_active = FALSE,
                    left_at = CURRENT_TIMESTAMP
                FROM users u
                WHERE m.group_id = $1
                  AND m.is_active = TRUE
                  AND u.id = m.user_id
                RETURNING m.*, u.username
                "#,
            )
            .bind(group_id)
            .fetch_all(&self.pool)
            .await
            .map_err(internal_error)?
        } else {
            sqlx::query(
                r#"
                UPDATE dispatch_chat_group_members m
                SET is_active = FALSE,
                    left_at = CURRENT_TIMESTAMP
                FROM users u
                WHERE m.group_id = $1
                  AND m.is_active = TRUE
                  AND NOT (m.user_id = ANY($2))
                  AND u.id = m.user_id
                RETURNING m.*, u.username
                "#,
            )
            .bind(group_id)
            .bind(active_user_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(internal_error)?
        };

        Ok(rows.iter().map(row_to_member).collect())
    }

    async fn clear_group_deprecation(
        &self,
        group_id: &str,
        reason: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        let row = sqlx::query(
            r#"
            WITH updated AS (
                UPDATE dispatch_chat_groups g
                SET deprecated_at = NULL,
                    deprecation_reason = NULL,
                    updated_at = CURRENT_TIMESTAMP
                WHERE g.group_id = $1
                  AND g.deprecation_reason = $2
                RETURNING *
            )
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM updated g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            "#,
        )
        .bind(group_id)
        .bind(reason)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_group_summary))
    }

    async fn mark_group_deprecated(
        &self,
        group_id: &str,
        reason: &str,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        let row = sqlx::query(
            r#"
            WITH updated AS (
                UPDATE dispatch_chat_groups g
                SET deprecated_at = COALESCE(g.deprecated_at, CURRENT_TIMESTAMP),
                    deprecation_reason = $2,
                    updated_at = CURRENT_TIMESTAMP
                WHERE g.group_id = $1
                  AND g.status <> 'archived'
                  AND g.deprecated_at IS NULL
                RETURNING *
            )
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM updated g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            "#,
        )
        .bind(group_id)
        .bind(reason)
        .fetch_optional(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row.as_ref().map(row_to_group_summary))
    }

    async fn find_groups_pending_deprecation(&self, limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM dispatch_chat_groups g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE g.channel_type = 'system_flight_dispatch'
              AND g.status = 'active'
              AND g.deprecated_at IS NULL
            ORDER BY COALESCE(g.updated_at, g.created_at) ASC
            LIMIT $1
            "#,
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_group_summary).collect())
    }

    async fn find_due_archive_groups(&self, limit: i64) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM dispatch_chat_groups g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            WHERE g.channel_type = 'system_flight_dispatch'
              AND g.status = 'active'
              AND g.archive_at IS NOT NULL
              AND g.archive_at <= CURRENT_TIMESTAMP
            ORDER BY g.archive_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_group_summary).collect())
    }

    async fn archive_groups_batch(&self, group_ids: &[String]) -> Result<Vec<DispatchChatGroupSummary>, DomainError> {
        if group_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            WITH updated AS (
                UPDATE dispatch_chat_groups g
                SET status = 'archived',
                    read_only = TRUE,
                    archived_at = COALESCE(g.archived_at, CURRENT_TIMESTAMP),
                    updated_at = CURRENT_TIMESTAMP
                WHERE g.group_id = ANY($1)
                  AND g.status <> 'archived'
                RETURNING *
            )
            SELECT
                g.*,
                TRUE AS member_is_active,
                COALESCE(member_stat.member_count, 0) AS member_count,
                last_msg.seq_no AS last_message_seq,
                last_msg.content AS last_message_content,
                last_msg.sent_at AS last_message_at,
                0::BIGINT AS unread_count
            FROM updated g
            LEFT JOIN LATERAL (
                SELECT seq_no, content, sent_at
                FROM dispatch_chat_messages dcm
                WHERE dcm.group_id = g.group_id
                ORDER BY dcm.seq_no DESC
                LIMIT 1
            ) last_msg ON TRUE
            LEFT JOIN LATERAL (
                SELECT COUNT(*)::BIGINT AS member_count
                FROM dispatch_chat_group_members m2
                WHERE m2.group_id = g.group_id
                  AND m2.is_active = TRUE
            ) member_stat ON TRUE
            "#,
        )
        .bind(group_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_group_summary).collect())
    }

    async fn create_event(
        &self,
        event: &DispatchCollaborationEvent,
    ) -> Result<DispatchCollaborationEvent, DomainError> {
        let row = sqlx::query(
            r#"
            INSERT INTO dispatch_collaboration_events (
                event_id,
                flight_id,
                dispatch_order_id,
                group_id,
                event_type,
                actor_user_id,
                correlation_id,
                payload,
                occurred_at,
                source_table,
                source_record_id
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8::jsonb, $9, $10, $11
            )
            ON CONFLICT (event_id) DO UPDATE SET
                payload = dispatch_collaboration_events.payload
            RETURNING *
            "#,
        )
        .bind(&event.event_id)
        .bind(&event.flight_id)
        .bind(&event.dispatch_order_id)
        .bind(&event.group_id)
        .bind(&event.event_type)
        .bind(&event.actor_user_id)
        .bind(&event.correlation_id)
        .bind(&event.payload)
        .bind(event.occurred_at)
        .bind(&event.source_table)
        .bind(&event.source_record_id)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row_to_event(&row))
    }

    async fn list_events_by_flight(
        &self,
        flight_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchCollaborationEvent>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT e.*, u.username AS actor_username
            FROM dispatch_collaboration_events e
            LEFT JOIN users u ON u.id = e.actor_user_id
            WHERE e.flight_id = $1
            ORDER BY e.occurred_at DESC, e.event_id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(flight_id)
        .bind(limit.clamp(1, 200))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_event).collect())
    }

    async fn list_events_by_order(
        &self,
        order_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchCollaborationEvent>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT e.*, u.username AS actor_username
            FROM dispatch_collaboration_events e
            LEFT JOIN users u ON u.id = e.actor_user_id
            WHERE e.dispatch_order_id = $1
            ORDER BY e.occurred_at DESC, e.event_id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(order_id)
        .bind(limit.clamp(1, 200))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_event).collect())
    }

    async fn find_recent_notifications_by_flight(
        &self,
        flight_id: &str,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM notifications
            WHERE flight_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(flight_id)
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_notification).collect())
    }

    async fn find_recent_notifications_by_order(
        &self,
        order_id: &str,
        limit: i64,
    ) -> Result<Vec<Notification>, DomainError> {
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM notifications
            WHERE dispatch_order_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(order_id)
        .bind(limit.clamp(1, 50))
        .fetch_all(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(rows.iter().map(row_to_notification).collect())
    }

    async fn summarize_receipts_for_flight(&self, flight_id: &str) -> Result<NotificationReceiptSummary, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE receipt_group_id IS NOT NULL)::BIGINT AS total_count,
                COUNT(*) FILTER (WHERE receipt_required = TRUE AND ack_status = 'pending')::BIGINT AS pending_count,
                COUNT(*) FILTER (WHERE ack_status = 'acknowledged')::BIGINT AS acknowledged_count,
                COUNT(*) FILTER (WHERE ack_status = 'rejected')::BIGINT AS rejected_count,
                MAX(COALESCE(ack_at, read_at, delivered_at, created_at)) AS latest_updated_at,
                ARRAY_REMOVE(ARRAY_AGG(DISTINCT receipt_group_id), NULL) AS receipt_group_ids
            FROM notifications
            WHERE flight_id = $1
              AND receipt_group_id IS NOT NULL
            "#,
        )
        .bind(flight_id)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row_to_receipt_summary(&row))
    }

    async fn summarize_receipts_for_order(&self, order_id: &str) -> Result<NotificationReceiptSummary, DomainError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE receipt_group_id IS NOT NULL)::BIGINT AS total_count,
                COUNT(*) FILTER (WHERE receipt_required = TRUE AND ack_status = 'pending')::BIGINT AS pending_count,
                COUNT(*) FILTER (WHERE ack_status = 'acknowledged')::BIGINT AS acknowledged_count,
                COUNT(*) FILTER (WHERE ack_status = 'rejected')::BIGINT AS rejected_count,
                MAX(COALESCE(ack_at, read_at, delivered_at, created_at)) AS latest_updated_at,
                ARRAY_REMOVE(ARRAY_AGG(DISTINCT receipt_group_id), NULL) AS receipt_group_ids
            FROM notifications
            WHERE dispatch_order_id = $1
              AND receipt_group_id IS NOT NULL
            "#,
        )
        .bind(order_id)
        .fetch_one(&self.pool)
        .await
        .map_err(internal_error)?;

        Ok(row_to_receipt_summary(&row))
    }
}

fn row_to_group_summary(row: &sqlx::postgres::PgRow) -> DispatchChatGroupSummary {
    let deprecated_at = row.try_get::<Option<DateTime<Utc>>, _>("deprecated_at").ok().flatten();
    let metadata = row
        .try_get::<serde_json::Value, _>("metadata")
        .unwrap_or_else(|_| serde_json::json!({}));

    DispatchChatGroupSummary {
        group_id: row.get("group_id"),
        channel_type: row
            .try_get::<Option<String>, _>("channel_type")
            .ok()
            .flatten()
            .unwrap_or_else(|| "system_flight_dispatch".to_string()),
        flight_id: row.get("flight_id"),
        group_name: row
            .try_get::<Option<String>, _>("group_name")
            .ok()
            .flatten()
            .unwrap_or_default(),
        status: row
            .try_get::<Option<String>, _>("status")
            .ok()
            .flatten()
            .unwrap_or_else(|| "active".to_string()),
        read_only: row
            .try_get::<Option<bool>, _>("read_only")
            .ok()
            .flatten()
            .unwrap_or(false),
        deprecated: deprecated_at.is_some(),
        deprecated_at,
        deprecation_reason: row.try_get("deprecation_reason").ok().flatten(),
        archive_at: row.try_get("archive_at").ok().flatten(),
        archived_at: row.try_get("archived_at").ok().flatten(),
        metadata,
        member_count: row
            .try_get::<Option<i64>, _>("member_count")
            .ok()
            .flatten()
            .unwrap_or(0),
        unread_count: row
            .try_get::<Option<i64>, _>("unread_count")
            .ok()
            .flatten()
            .unwrap_or(0),
        last_message_seq: row.try_get("last_message_seq").ok().flatten(),
        last_message_preview: row.try_get("last_message_content").ok().flatten(),
        last_message_at: row.try_get("last_message_at").ok().flatten(),
        member_is_active: row
            .try_get::<Option<bool>, _>("member_is_active")
            .ok()
            .flatten()
            .unwrap_or(true),
    }
}

fn row_to_member(row: &sqlx::postgres::PgRow) -> DispatchChatMember {
    DispatchChatMember {
        id: row.get("id"),
        group_id: row.get("group_id"),
        user_id: row.get("user_id"),
        username: row.try_get("username").ok().flatten(),
        is_assignee: row
            .try_get::<Option<bool>, _>("is_assignee")
            .ok()
            .flatten()
            .unwrap_or(false),
        is_dispatcher: row
            .try_get::<Option<bool>, _>("is_dispatcher")
            .ok()
            .flatten()
            .unwrap_or(false),
        is_active: row
            .try_get::<Option<bool>, _>("is_active")
            .ok()
            .flatten()
            .unwrap_or(true),
        joined_at: row.try_get("joined_at").ok().flatten(),
        left_at: row.try_get("left_at").ok().flatten(),
        last_read_seq: row
            .try_get::<Option<i64>, _>("last_read_seq")
            .ok()
            .flatten()
            .unwrap_or(0),
        last_read_at: row.try_get("last_read_at").ok().flatten(),
    }
}

fn row_to_user_profile(row: &sqlx::postgres::PgRow) -> DispatchChatUserProfile {
    DispatchChatUserProfile {
        user_id: row.get("user_id"),
        username: row.try_get("username").ok().flatten(),
        department: row.try_get("department").ok().flatten(),
        job_title: row.try_get("job_title").ok().flatten(),
        is_active: row
            .try_get::<Option<bool>, _>("is_active")
            .ok()
            .flatten()
            .unwrap_or(true),
    }
}

fn row_to_dispatcher_candidate(row: &sqlx::postgres::PgRow) -> DispatchChatDispatcherCandidate {
    DispatchChatDispatcherCandidate {
        user_id: row.get("user_id"),
        username: row.try_get("username").ok().flatten(),
        department: row.try_get("department").ok().flatten(),
        job_title: row.try_get("job_title").ok().flatten(),
    }
}

fn row_to_message(row: &sqlx::postgres::PgRow) -> DispatchChatMessage {
    DispatchChatMessage {
        message_id: row.get("message_id"),
        seq_no: row.try_get::<Option<i64>, _>("seq_no").ok().flatten().unwrap_or(0),
        group_id: row.get("group_id"),
        sender_user_id: row.try_get("sender_user_id").ok().flatten(),
        sender_username: row.try_get("sender_username").ok().flatten(),
        message_type: row
            .try_get::<Option<String>, _>("message_type")
            .ok()
            .flatten()
            .unwrap_or_else(|| "text".to_string()),
        content: row
            .try_get::<Option<String>, _>("content")
            .ok()
            .flatten()
            .unwrap_or_default(),
        is_at_all: row
            .try_get::<Option<bool>, _>("is_at_all")
            .ok()
            .flatten()
            .unwrap_or(false),
        metadata: row
            .try_get::<serde_json::Value, _>("metadata")
            .unwrap_or_else(|_| serde_json::json!({})),
        sent_at: row.get("sent_at"),
        dispatch_order_id: row.try_get("dispatch_order_id").ok().flatten(),
        event_id: row.try_get("event_id").ok().flatten(),
    }
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> DispatchCollaborationEvent {
    DispatchCollaborationEvent {
        event_id: row.get("event_id"),
        flight_id: row.get("flight_id"),
        dispatch_order_id: row.try_get("dispatch_order_id").ok().flatten(),
        group_id: row.try_get("group_id").ok().flatten(),
        event_type: row
            .try_get::<Option<String>, _>("event_type")
            .ok()
            .flatten()
            .unwrap_or_else(|| "unknown".to_string()),
        actor_user_id: row.try_get("actor_user_id").ok().flatten(),
        actor_username: row.try_get("actor_username").ok().flatten(),
        correlation_id: row.try_get("correlation_id").ok().flatten(),
        payload: row
            .try_get::<serde_json::Value, _>("payload")
            .unwrap_or_else(|_| serde_json::json!({})),
        occurred_at: row.get("occurred_at"),
        source_table: row.try_get("source_table").ok().flatten(),
        source_record_id: row.try_get("source_record_id").ok().flatten(),
    }
}

fn row_to_notification(row: &sqlx::postgres::PgRow) -> Notification {
    Notification {
        notification_id: row.get("notification_id"),
        user_id: row.get("user_id"),
        title: row.get("title"),
        body: row
            .try_get::<Option<String>, _>("body")
            .ok()
            .flatten()
            .unwrap_or_default(),
        category: row
            .try_get::<Option<String>, _>("category")
            .ok()
            .flatten()
            .unwrap_or_else(|| "system".to_string()),
        severity: row
            .try_get::<Option<String>, _>("severity")
            .ok()
            .flatten()
            .unwrap_or_else(|| "info".to_string()),
        is_read: row
            .try_get::<Option<bool>, _>("is_read")
            .ok()
            .flatten()
            .unwrap_or(false),
        flight_id: row.try_get("flight_id").ok().flatten(),
        related_entity_type: row.try_get("related_entity_type").ok().flatten(),
        related_entity_id: row.try_get("related_entity_id").ok().flatten(),
        dispatch_order_id: row.try_get("dispatch_order_id").ok().flatten(),
        group_id: row.try_get("group_id").ok().flatten(),
        event_id: row.try_get("event_id").ok().flatten(),
        sender_user_id: row.try_get("sender_user_id").ok().flatten(),
        sender_username_snapshot: row.try_get("sender_username_snapshot").ok().flatten(),
        recipient_username_snapshot: row.try_get("recipient_username").ok().flatten(),
        recipient_display_name_snapshot: row.try_get("recipient_display_name").ok().flatten(),
        recipient_department_snapshot: row.try_get("recipient_department").ok().flatten(),
        recipient_job_title_snapshot: row.try_get("recipient_job_title").ok().flatten(),
        origin_type: row
            .try_get::<Option<String>, _>("origin_type")
            .ok()
            .flatten()
            .unwrap_or_else(|| "manual".to_string()),
        receipt_required: row
            .try_get::<Option<bool>, _>("receipt_required")
            .ok()
            .flatten()
            .unwrap_or(false),
        receipt_group_id: row.try_get("receipt_group_id").ok().flatten(),
        delivery_status: row
            .try_get::<Option<String>, _>("delivery_status")
            .ok()
            .flatten()
            .unwrap_or_else(|| "sent".to_string()),
        delivered_at: row.try_get("delivered_at").ok().flatten(),
        ack_status: row
            .try_get::<Option<String>, _>("ack_status")
            .ok()
            .flatten()
            .unwrap_or_else(|| "pending".to_string()),
        ack_at: row.try_get("ack_at").ok().flatten(),
        ack_note: row.try_get("ack_note").ok().flatten(),
        created_at: row.get("created_at"),
        read_at: row.try_get("read_at").ok().flatten(),
    }
}

fn row_to_receipt_summary(row: &sqlx::postgres::PgRow) -> NotificationReceiptSummary {
    NotificationReceiptSummary {
        total_count: row.try_get::<Option<i64>, _>("total_count").ok().flatten().unwrap_or(0),
        pending_count: row
            .try_get::<Option<i64>, _>("pending_count")
            .ok()
            .flatten()
            .unwrap_or(0),
        acknowledged_count: row
            .try_get::<Option<i64>, _>("acknowledged_count")
            .ok()
            .flatten()
            .unwrap_or(0),
        rejected_count: row
            .try_get::<Option<i64>, _>("rejected_count")
            .ok()
            .flatten()
            .unwrap_or(0),
        latest_updated_at: row.try_get("latest_updated_at").ok().flatten(),
        receipt_group_ids: row
            .try_get::<Option<Vec<String>>, _>("receipt_group_ids")
            .ok()
            .flatten()
            .unwrap_or_default(),
    }
}

fn internal_error(error: sqlx::Error) -> DomainError {
    DomainError::Internal(error.to_string())
}

fn normalized_values(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || normalized.iter().any(|existing| existing == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}
