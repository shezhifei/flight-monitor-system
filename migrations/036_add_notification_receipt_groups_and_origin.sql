
ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS origin_type VARCHAR(32) NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS receipt_required BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS receipt_group_id VARCHAR(26) NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_receipt_group_created_at
    ON notifications (receipt_group_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_dispatch_order_created_at
    ON notifications (dispatch_order_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_flight_created_at
    ON notifications (flight_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_origin_type_created_at
    ON notifications (origin_type, created_at DESC);

