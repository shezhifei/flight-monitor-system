-- no-transaction
-- Migration: 078_add_flight_legs_lateral_optimization_index.sql
-- Description: Add composite index for LATERAL subquery optimization in dispatch_order queries
-- Created: 2026-05-20
-- Issue: LEFT JOIN LATERAL in base_order_select() causes Nested Loop + Run-time Sort
--         due to complex ORDER BY with CASE WHEN and multiple time fields
-- Solution: Create composite index optimized for the LATERAL lookup pattern

-- This index covers the LATERAL subquery pattern:
--   SELECT leg.flight_no FROM flight_legs leg
--   WHERE leg.flight_id = d.flight_id
--   ORDER BY
--     CASE WHEN leg.leg_type = 'outbound' THEN 0 ELSE 1 END,
--     leg.updated_at DESC NULLS LAST,
--     leg.created_at DESC NULLS LAST
--   LIMIT 1

-- Primary index: flight_id (equality condition) + leg_type (first sort key)
-- The ORDER BY CASE WHEN is converted to leg_type sorting (outbound first = 0)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_flight_legs_lateral_lookup
ON flight_legs (flight_id, leg_type, updated_at DESC, created_at DESC);

-- Alternative approach: Add denormalized flight_no column to dispatch_orders
-- This completely eliminates the LATERAL join but requires application changes
-- See migration 079_add_dispatch_order_flight_no_denormalization.sql (if created)

-- Verify the index was created
-- SELECT indexname, indexdef FROM pg_indexes WHERE indexname = 'idx_flight_legs_lateral_lookup';

-- After creating the index, analyze the table to update statistics
-- ANALYZE flight_legs;

-- Verify the query plan improvement with:
-- EXPLAIN ANALYZE SELECT d.*, fl.flight_no
-- FROM dispatch_orders d
-- LEFT JOIN LATERAL (
--     SELECT leg.flight_no FROM flight_legs leg
--     WHERE leg.flight_id = d.flight_id
--     ORDER BY CASE WHEN leg.leg_type = 'outbound' THEN 0 ELSE 1 END,
--              leg.updated_at DESC NULLS LAST, leg.created_at DESC NULLS LAST
--     LIMIT 1
-- ) fl ON TRUE
-- LIMIT 100;
