-- Create in-app notifications table


CREATE TABLE IF NOT EXISTS notifications (
    notification_id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL,
    title VARCHAR(255) NOT NULL,
    body TEXT,
    category VARCHAR(32) NOT NULL DEFAULT 'system',
    severity VARCHAR(16) NOT NULL DEFAULT 'info',
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    related_entity_type VARCHAR(32),
    related_entity_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    read_at TIMESTAMPTZ,
    CONSTRAINT fk_notifications_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
    ON notifications(user_id, is_read, created_at DESC);

