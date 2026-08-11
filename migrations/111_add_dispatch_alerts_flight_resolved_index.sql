-- no-transaction
-- Companion to 098: dispatch_alerts by flight_id + is_resolved.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_dispatch_alerts_flight_resolved
    ON dispatch_alerts (flight_id, is_resolved);
