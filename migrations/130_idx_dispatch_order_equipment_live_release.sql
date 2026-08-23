-- no-transaction
-- Speed up dispatch_orders filtered by released_at/status on live rows.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_orders_released_status
    ON dispatch_orders (released_at, status)
    WHERE deleted_at IS NULL;
