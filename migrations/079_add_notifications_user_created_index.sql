-- no-transaction
-- Performance optimization: composite index for notification list queries
-- Supports: WHERE user_id = $1 ORDER BY created_at DESC
-- NOTE: only one CREATE INDEX CONCURRENTLY per file (sqlx + PG implicit TX).
-- Companion index: 108_add_notifications_user_read_created_index.sql

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_user_created_desc
    ON notifications (user_id, created_at DESC);
