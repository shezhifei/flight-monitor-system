-- no-transaction
-- 098: first composite index for high-frequency query paths (audit #121)
-- NOTE: only one CREATE INDEX CONCURRENTLY per file (sqlx + PG implicit TX).
-- Companion indexes: 109–112_*.sql

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_orders_flight_planned
    ON dispatch_orders (flight_id, planned_start_time DESC NULLS LAST);
