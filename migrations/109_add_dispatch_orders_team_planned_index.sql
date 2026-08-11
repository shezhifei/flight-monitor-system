-- no-transaction
-- Companion to 098: dispatch_orders by team_id + planned time.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_orders_team_planned
    ON dispatch_orders (team_id, planned_start_time DESC NULLS LAST, created_at DESC)
    WHERE team_id IS NOT NULL;
