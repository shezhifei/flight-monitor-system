-- no-transaction
-- Companion to 079: unread/list composite for notifications.
-- Split out because sqlx cannot run multiple CREATE INDEX CONCURRENTLY in one file.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_user_read_created
    ON notifications (user_id, is_read, created_at DESC);
