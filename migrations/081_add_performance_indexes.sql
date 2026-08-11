-- Performance indexes for flight list query optimization
-- These indexes reduce seq scans and enable index-only scans for common flight listing queries.

-- Sort expression index: COALESCE(scheduled_departure, scheduled_arrival) DESC
-- This is the primary sort used by the flight list endpoint (GET /api/v2/flights).
-- Without this index, PostgreSQL must seq scan and sort all flights.
CREATE INDEX IF NOT EXISTS idx_flights_sort_time_desc
ON flights ((COALESCE(scheduled_departure, scheduled_arrival)) DESC);

-- Composite index for anomaly filtering by flight and status
-- The flight list endpoint often filters by has_open_anomaly.
-- This index makes the anomaly check per flight faster.
CREATE INDEX IF NOT EXISTS idx_anomalies_flight_status
ON anomalies (flight_id, status);

-- Composite index for flight legs lookup
-- Used by attach_legs() to load inbound/outbound legs per flight.
CREATE INDEX IF NOT EXISTS idx_flight_legs_flight_id_leg_type
ON flight_legs (flight_id, leg_type);
