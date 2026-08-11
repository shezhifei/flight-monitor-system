-- =====================================================
-- Migration 004: Create Archive Tables
-- Description: Sets up the archiving system for flights.
--   Depends on baseline tables created in migration 000:
--   flights, flight_state_changes, flight_business_cases,
--   snapshots, event_stream_versions.
-- =====================================================

-- 1. Create Archive Tables
-- =====================================================

-- 1.1 Archived Flights (Master Table)
CREATE TABLE IF NOT EXISTS archived_flights (LIKE flights INCLUDING ALL);
ALTER TABLE archived_flights ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
-- Indexes are copied via INCLUDING ALL, but verify/ensure critical ones
CREATE INDEX IF NOT EXISTS idx_archived_flights_flight_id ON archived_flights(flight_id);
CREATE INDEX IF NOT EXISTS idx_archived_flights_archived_at ON archived_flights(archived_at);

-- 1.2 Archived Flight State Changes
CREATE TABLE IF NOT EXISTS archived_flight_state_changes (LIKE flight_state_changes INCLUDING ALL);
ALTER TABLE archived_flight_state_changes ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
-- Re-point Foreign Key to archived_flights
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'flight_state_changes_flight_id_fkey' AND conrelid = 'archived_flight_state_changes'::regclass) THEN
        ALTER TABLE archived_flight_state_changes DROP CONSTRAINT flight_state_changes_flight_id_fkey;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_archived_fsc_flight'
          AND conrelid = 'archived_flight_state_changes'::regclass
    ) THEN
        ALTER TABLE archived_flight_state_changes
            ADD CONSTRAINT fk_archived_fsc_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;

-- 1.3 Archived Flight Business Cases
CREATE TABLE IF NOT EXISTS archived_flight_business_cases (LIKE flight_business_cases INCLUDING ALL);
ALTER TABLE archived_flight_business_cases ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'flight_business_cases_flight_id_fkey' AND conrelid = 'archived_flight_business_cases'::regclass) THEN
        ALTER TABLE archived_flight_business_cases DROP CONSTRAINT flight_business_cases_flight_id_fkey;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_archived_fbc_flight'
          AND conrelid = 'archived_flight_business_cases'::regclass
    ) THEN
        ALTER TABLE archived_flight_business_cases
            ADD CONSTRAINT fk_archived_fbc_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;

-- 1.4 Archived Snapshots
CREATE TABLE IF NOT EXISTS archived_snapshots (LIKE snapshots INCLUDING ALL);
ALTER TABLE archived_snapshots ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'snapshots_flight_id_fkey' AND conrelid = 'archived_snapshots'::regclass) THEN
        ALTER TABLE archived_snapshots DROP CONSTRAINT snapshots_flight_id_fkey;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_archived_snapshot_flight'
          AND conrelid = 'archived_snapshots'::regclass
    ) THEN
        ALTER TABLE archived_snapshots
            ADD CONSTRAINT fk_archived_snapshot_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;

-- 1.5 Archived Event Stream Versions
CREATE TABLE IF NOT EXISTS archived_event_stream_versions (LIKE event_stream_versions INCLUDING ALL);
ALTER TABLE archived_event_stream_versions ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'event_stream_versions_flight_id_fkey' AND conrelid = 'archived_event_stream_versions'::regclass) THEN
        ALTER TABLE archived_event_stream_versions DROP CONSTRAINT event_stream_versions_flight_id_fkey;
    END IF;
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_archived_esv_flight'
          AND conrelid = 'archived_event_stream_versions'::regclass
    ) THEN
        ALTER TABLE archived_event_stream_versions
            ADD CONSTRAINT fk_archived_esv_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;


-- 2. Archive Stored Procedure
-- =====================================================
-- NOTE: pgAgent jobs were removed from migration 004 because pgagent may not
-- be installed in all target databases. Job scheduling is now handled by the
-- application runtime or external cron, not by a migration-time assumption.

CREATE OR REPLACE FUNCTION archive_flight_data(
    p_cutoff_date DATE,       
    p_target_date DATE DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_flight_ids VARCHAR[];
    v_count INT;
    v_source_table TEXT;
    v_target_table TEXT;
    v_columns TEXT;
BEGIN
    -- 1. Identify Flight IDs to archive
    SELECT ARRAY_AGG(flight_id) INTO v_flight_ids
    FROM flights
    WHERE (p_target_date IS NOT NULL AND workspace_date = p_target_date)
       OR (p_target_date IS NULL AND workspace_date < p_cutoff_date); 

    IF v_flight_ids IS NULL OR array_length(v_flight_ids, 1) IS NULL THEN
        RETURN jsonb_build_object('status', 'no_data', 'archived_count', 0);
    END IF;

    v_count := array_length(v_flight_ids, 1);

    -- 2. Move to archive tables with schema-safe dynamic columns.
    FOR v_source_table, v_target_table IN
        SELECT *
        FROM unnest(
            ARRAY[
                'flights',
                'flight_state_changes',
                'flight_business_cases',
                'snapshots',
                'event_stream_versions'
            ]::TEXT[],
            ARRAY[
                'archived_flights',
                'archived_flight_state_changes',
                'archived_flight_business_cases',
                'archived_snapshots',
                'archived_event_stream_versions'
            ]::TEXT[]
        )
    LOOP
        SELECT STRING_AGG(quote_ident(c.column_name), ', ' ORDER BY c.ordinal_position)
          INTO v_columns
        FROM information_schema.columns c
        WHERE c.table_schema = 'public'
          AND c.table_name = v_target_table
          AND c.column_name <> 'archived_at'
          AND COALESCE(c.is_generated, 'NEVER') = 'NEVER'
          AND EXISTS (
              SELECT 1
              FROM information_schema.columns s
              WHERE s.table_schema = 'public'
                AND s.table_name = v_source_table
                AND s.column_name = c.column_name
                AND COALESCE(s.is_generated, 'NEVER') = 'NEVER'
          );

        IF v_columns IS NULL OR btrim(v_columns) = '' THEN
            RAISE EXCEPTION 'Archive column resolution failed for target=% source=%', v_target_table, v_source_table;
        END IF;

        EXECUTE format(
            'INSERT INTO %I (%s, archived_at) ' ||
            'SELECT %s, NOW() FROM %I WHERE flight_id = ANY($1) ON CONFLICT DO NOTHING',
            v_target_table,
            v_columns,
            v_columns,
            v_source_table
        )
        USING v_flight_ids;
    END LOOP;

    -- 3. Delete from Active Tables
    -- Simple explicit deletion for safety/clarity without relying on implicit cascades if schema drifts
    DELETE FROM flight_business_cases WHERE flight_id = ANY(v_flight_ids);
    DELETE FROM flight_state_changes WHERE flight_id = ANY(v_flight_ids); 
    DELETE FROM snapshots WHERE flight_id = ANY(v_flight_ids);
    DELETE FROM event_stream_versions WHERE flight_id = ANY(v_flight_ids);
    DELETE FROM flights WHERE flight_id = ANY(v_flight_ids);
    
    RETURN jsonb_build_object('status', 'success', 'archived_count', v_count, 'flight_ids', v_flight_ids);
END;
$$ LANGUAGE plpgsql;
