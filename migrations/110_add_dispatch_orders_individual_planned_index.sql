-- no-transaction
-- Companion to 098: dispatch_orders by individual_user_id + planned time.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_orders_individual_planned
    ON dispatch_orders (individual_user_id, planned_start_time DESC NULLS LAST, created_at DESC)
    WHERE individual_user_id IS NOT NULL;
