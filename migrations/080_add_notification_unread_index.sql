-- no-transaction
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_unread
    ON notifications(user_id)
    WHERE is_read = FALSE;
