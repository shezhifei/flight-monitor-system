-- Add notification preferences and dispatch log indexes


CREATE TABLE IF NOT EXISTS notification_preferences (
    user_id VARCHAR(26) PRIMARY KEY,
    in_app_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    external_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    external_channel VARCHAR(32) NOT NULL DEFAULT 'none',
    mute_start VARCHAR(5),
    mute_end VARCHAR(5),
    critical_override BOOLEAN NOT NULL DEFAULT TRUE,
    category_overrides JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_notification_preferences_user
        FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_order_action
    ON dispatch_order_logs(dispatch_order_id, action, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_client_action
    ON dispatch_order_logs((details->>'client_action_id'))
    WHERE details ? 'client_action_id';

