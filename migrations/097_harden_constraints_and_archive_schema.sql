-- Migration 097: Harden constraints and fix archive schema drift
-- Addresses audit issues #41, #47, #48


-- =====================================================
-- #48: Restore driver_type CHECK constraint on dispatch_orders
-- Migration 054 replaced the original constraint but dropped
-- the driver_type validation that was in migration 007.
-- =====================================================

ALTER TABLE dispatch_orders DROP CONSTRAINT IF EXISTS dispatch_orders_check;

ALTER TABLE dispatch_orders
    ADD CONSTRAINT dispatch_orders_check CHECK (
        -- assignee consistency
        (
            (
                assignee_type = 'team'
                AND individual_user_id IS NULL
                AND (
                    team_id IS NOT NULL
                    OR status = 'pending'
                    OR COALESCE(
                        jsonb_array_length(COALESCE(task_crew->'members', '[]'::jsonb)),
                        0
                    ) > 0
                )
            )
            OR (
                assignee_type = 'individual'
                AND team_id IS NULL
                AND (
                    individual_user_id IS NOT NULL
                    OR status = 'pending'
                )
            )
        )
        -- driver_type consistency (restored from migration 007)
        AND (
            (driver_type IS NULL AND driver_team_id IS NULL AND driver_user_id IS NULL)
            OR (driver_type = 'team' AND driver_team_id IS NOT NULL AND driver_user_id IS NULL)
            OR (driver_type = 'individual' AND driver_user_id IS NOT NULL AND driver_team_id IS NULL)
        )
    );

-- =====================================================
-- #47: Add FK on flight_sync_bindings.flight_id
-- Ensures sync bindings always reference a valid flight.
-- =====================================================

ALTER TABLE flight_sync_bindings
    ADD CONSTRAINT fk_flight_sync_bindings_flight
    FOREIGN KEY (flight_id) REFERENCES flights(flight_id) ON DELETE CASCADE;

-- =====================================================
-- #41: archived_flights schema drift safeguard
-- The archive table was created via LIKE INCLUDING ALL in
-- migration 004. The archive_flight_data() function already
-- uses information_schema for dynamic column resolution,
-- so it is schema-safe. Add a documentation comment.
-- =====================================================

COMMENT ON TABLE archived_flights IS
    'Archive of flights. Created via LIKE INCLUDING ALL in migration 004. '
    'Column resolution in archive_flight_data() uses information_schema '
    'to stay schema-safe even if flights table evolves.';

