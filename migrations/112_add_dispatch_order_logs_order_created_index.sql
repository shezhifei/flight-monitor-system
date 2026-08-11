-- no-transaction
-- Companion to 098: dispatch_order_logs by order + created_at.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_order_logs_order_created
    ON dispatch_order_logs (dispatch_order_id, created_at DESC);
