
ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS sender_user_id VARCHAR(26),
    ADD COLUMN IF NOT EXISTS sender_username_snapshot VARCHAR(128);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_sender_user'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_sender_user
            FOREIGN KEY (sender_user_id) REFERENCES users(id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_notifications_sender_created_at
    ON notifications (sender_user_id, created_at DESC)
    WHERE sender_user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_sender_receipt_group_created_at
    ON notifications (sender_user_id, receipt_group_id, created_at DESC)
    WHERE sender_user_id IS NOT NULL AND receipt_group_id IS NOT NULL;

