
CREATE TABLE IF NOT EXISTS dispatch_collaboration_events (
    event_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    dispatch_order_id VARCHAR(26),
    group_id VARCHAR(26),
    event_type VARCHAR(64) NOT NULL,
    actor_user_id VARCHAR(26),
    correlation_id VARCHAR(64),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_table VARCHAR(64),
    source_record_id VARCHAR(64),
    CONSTRAINT fk_dispatch_collab_event_flight FOREIGN KEY (flight_id) REFERENCES flights(flight_id) ON DELETE CASCADE,
    CONSTRAINT fk_dispatch_collab_event_order FOREIGN KEY (dispatch_order_id) REFERENCES dispatch_orders(id) ON DELETE SET NULL,
    CONSTRAINT fk_dispatch_collab_event_group FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE SET NULL,
    CONSTRAINT fk_dispatch_collab_event_actor FOREIGN KEY (actor_user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_flight_occurred_desc
    ON dispatch_collaboration_events(flight_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_order_occurred_desc
    ON dispatch_collaboration_events(dispatch_order_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_group_occurred_desc
    ON dispatch_collaboration_events(group_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_correlation_id
    ON dispatch_collaboration_events(correlation_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_type_occurred_desc
    ON dispatch_collaboration_events(event_type, occurred_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_collab_events_source_record
    ON dispatch_collaboration_events(source_table, source_record_id)
    WHERE source_table IS NOT NULL AND source_record_id IS NOT NULL;

ALTER TABLE dispatch_order_logs
    ADD COLUMN IF NOT EXISTS event_id VARCHAR(26);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_dispatch_order_logs_event'
    ) THEN
        ALTER TABLE dispatch_order_logs
            ADD CONSTRAINT fk_dispatch_order_logs_event
            FOREIGN KEY (event_id) REFERENCES dispatch_collaboration_events(event_id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_event_id
    ON dispatch_order_logs(event_id)
    WHERE event_id IS NOT NULL;

ALTER TABLE dispatch_chat_messages
    ADD COLUMN IF NOT EXISTS dispatch_order_id VARCHAR(26);

ALTER TABLE dispatch_chat_messages
    ADD COLUMN IF NOT EXISTS event_id VARCHAR(26);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_dispatch_chat_messages_order'
    ) THEN
        ALTER TABLE dispatch_chat_messages
            ADD CONSTRAINT fk_dispatch_chat_messages_order
            FOREIGN KEY (dispatch_order_id) REFERENCES dispatch_orders(id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_dispatch_chat_messages_event'
    ) THEN
        ALTER TABLE dispatch_chat_messages
            ADD CONSTRAINT fk_dispatch_chat_messages_event
            FOREIGN KEY (event_id) REFERENCES dispatch_collaboration_events(event_id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_order_sent_desc
    ON dispatch_chat_messages(dispatch_order_id, sent_at DESC)
    WHERE dispatch_order_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_event_id
    ON dispatch_chat_messages(event_id)
    WHERE event_id IS NOT NULL;

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS flight_id VARCHAR(26);

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS dispatch_order_id VARCHAR(26);

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS group_id VARCHAR(26);

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS event_id VARCHAR(26);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_flight'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_flight
            FOREIGN KEY (flight_id) REFERENCES flights(flight_id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_dispatch_order'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_dispatch_order
            FOREIGN KEY (dispatch_order_id) REFERENCES dispatch_orders(id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_group'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_group
            FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE SET NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_event'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_event
            FOREIGN KEY (event_id) REFERENCES dispatch_collaboration_events(event_id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_notifications_flight_created_desc
    ON notifications(flight_id, created_at DESC)
    WHERE flight_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_dispatch_order_created_desc
    ON notifications(dispatch_order_id, created_at DESC)
    WHERE dispatch_order_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_group_created_desc
    ON notifications(group_id, created_at DESC)
    WHERE group_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_event_id
    ON notifications(event_id)
    WHERE event_id IS NOT NULL;

COMMENT ON TABLE dispatch_collaboration_events IS '派工协同统一审计账本';

