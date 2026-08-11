-- Add delivery and acknowledgement fields for notifications


ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS delivery_status VARCHAR(16) NOT NULL DEFAULT 'sent',
    ADD COLUMN IF NOT EXISTS delivered_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ack_status VARCHAR(16) NOT NULL DEFAULT 'pending',
    ADD COLUMN IF NOT EXISTS ack_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ack_note TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_notification_delivery_status'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT chk_notification_delivery_status
            CHECK (delivery_status IN ('sent', 'delivered', 'failed'));
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_notification_ack_status'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT chk_notification_ack_status
            CHECK (ack_status IN ('pending', 'acknowledged', 'rejected'));
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_notifications_user_ack_status
    ON notifications(user_id, ack_status, created_at DESC);

