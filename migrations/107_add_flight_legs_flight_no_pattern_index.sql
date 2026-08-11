-- no-transaction
-- Companion to 077: pattern index for flight_legs.flight_no prefix search.
-- Split out because sqlx cannot run multiple CREATE INDEX CONCURRENTLY in one file.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_flight_legs_flight_no_pattern
ON flight_legs (flight_no varchar_pattern_ops);
