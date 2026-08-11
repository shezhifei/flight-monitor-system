-- no-transaction
-- Migration: 077_add_flight_pattern_matching_indexes.sql
-- Description: Add pattern matching index for flights.flight_number prefix search
-- NOTE: sqlx executes each migration as one multi-statement query. PostgreSQL
-- treats multi-statement simple queries as an implicit transaction, so only
-- ONE CREATE INDEX CONCURRENTLY is allowed per migration file.
-- Remaining pattern index lives in 107_*.sql.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_flights_flight_number_pattern
ON flights (flight_number varchar_pattern_ops);
