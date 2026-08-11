-- Migration 032: strong-cut flight leg / dispatch timeline / anomaly-domain decoupling


ALTER TABLE flights
    ADD COLUMN IF NOT EXISTS flight_no_inbound VARCHAR(16),
    ADD COLUMN IF NOT EXISTS flight_no_outbound VARCHAR(16),
    ADD COLUMN IF NOT EXISTS missions SMALLINT[],
    ADD COLUMN IF NOT EXISTS flight_types SMALLINT[],
    ADD COLUMN IF NOT EXISTS origins VARCHAR(8)[],
    ADD COLUMN IF NOT EXISTS destinations VARCHAR(8)[],
    ADD COLUMN IF NOT EXISTS origins_name VARCHAR(128)[],
    ADD COLUMN IF NOT EXISTS destinations_name VARCHAR(128)[],
    ADD COLUMN IF NOT EXISTS stand_types TEXT,
    ADD COLUMN IF NOT EXISTS is_vip BOOLEAN[],
    ADD COLUMN IF NOT EXISTS start_boarding_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS end_boarding_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS codt TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS off_blocks_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS passengers_ready_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS boarding_permission_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS inbound_abnormal BOOLEAN,
    ADD COLUMN IF NOT EXISTS outbound_abnormal BOOLEAN,
    ADD COLUMN IF NOT EXISTS inbound_abnormal_reason TEXT,
    ADD COLUMN IF NOT EXISTS outbound_abnormal_reason TEXT,
    ADD COLUMN IF NOT EXISTS custom_fields JSONB DEFAULT '{}'::jsonb;

DO $$
BEGIN
    IF to_regclass('public.archived_flights') IS NOT NULL THEN
        EXECUTE '
            ALTER TABLE archived_flights
                ADD COLUMN IF NOT EXISTS flight_no_inbound VARCHAR(16),
                ADD COLUMN IF NOT EXISTS flight_no_outbound VARCHAR(16),
                ADD COLUMN IF NOT EXISTS missions SMALLINT[],
                ADD COLUMN IF NOT EXISTS flight_types SMALLINT[],
                ADD COLUMN IF NOT EXISTS origins VARCHAR(8)[],
                ADD COLUMN IF NOT EXISTS destinations VARCHAR(8)[],
                ADD COLUMN IF NOT EXISTS origins_name VARCHAR(128)[],
                ADD COLUMN IF NOT EXISTS destinations_name VARCHAR(128)[],
                ADD COLUMN IF NOT EXISTS stand_types TEXT,
                ADD COLUMN IF NOT EXISTS is_vip BOOLEAN[],
                ADD COLUMN IF NOT EXISTS start_boarding_time TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS end_boarding_time TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS codt TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS off_blocks_time TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS passengers_ready_time TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS boarding_permission_time TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS inbound_abnormal BOOLEAN,
                ADD COLUMN IF NOT EXISTS outbound_abnormal BOOLEAN,
                ADD COLUMN IF NOT EXISTS inbound_abnormal_reason TEXT,
                ADD COLUMN IF NOT EXISTS outbound_abnormal_reason TEXT,
                ADD COLUMN IF NOT EXISTS custom_fields JSONB DEFAULT ''{}''::jsonb
        ';
    END IF;
END $$;

-- =====================================================
-- 1) New tables
-- =====================================================

CREATE TABLE IF NOT EXISTS flight_legs (
    leg_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    leg_type VARCHAR(16) NOT NULL,
    flight_no VARCHAR(16) NOT NULL,
    flight_type VARCHAR(16) NOT NULL,
    mission SMALLINT,
    origin_code VARCHAR(8),
    destination_code VARCHAR(8),
    origin_name VARCHAR(128),
    destination_name VARCHAR(128),
    is_vip BOOLEAN NOT NULL DEFAULT FALSE,
    stand_type VARCHAR(64),
    scheduled_time TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_flight_legs_leg_type CHECK (leg_type IN ('inbound', 'outbound')),
    CONSTRAINT chk_flight_legs_flight_type CHECK (flight_type IN ('domestic', 'intl', 'region')),
    CONSTRAINT uq_flight_legs_flight_leg UNIQUE (flight_id, leg_type)
);

CREATE INDEX IF NOT EXISTS idx_flight_legs_flight_id
    ON flight_legs(flight_id);

CREATE INDEX IF NOT EXISTS idx_flight_legs_flight_no
    ON flight_legs(flight_no);

CREATE INDEX IF NOT EXISTS idx_flight_legs_leg_type_scheduled
    ON flight_legs(leg_type, scheduled_time DESC);

ALTER TABLE flight_legs
    ADD COLUMN IF NOT EXISTS origin_code VARCHAR(8),
    ADD COLUMN IF NOT EXISTS destination_code VARCHAR(8),
    ADD COLUMN IF NOT EXISTS origin_name VARCHAR(128),
    ADD COLUMN IF NOT EXISTS destination_name VARCHAR(128);

CREATE TABLE IF NOT EXISTS flight_dispatch_timeline_events (
    timeline_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    milestone_code VARCHAR(64) NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    leg_type VARCHAR(16),
    recorded_by VARCHAR(128),
    source VARCHAR(64) NOT NULL DEFAULT 'manual',
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_timeline_leg_type CHECK (leg_type IS NULL OR leg_type IN ('inbound', 'outbound'))
);

CREATE INDEX IF NOT EXISTS idx_flight_dispatch_timeline_flight_occurred
    ON flight_dispatch_timeline_events(flight_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_flight_dispatch_timeline_milestone_occurred
    ON flight_dispatch_timeline_events(milestone_code, occurred_at DESC);

CREATE TABLE IF NOT EXISTS flight_custom_field_archive (
    archive_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    field_key VARCHAR(128) NOT NULL,
    field_value_json JSONB,
    migrated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flight_custom_field_archive_flight_id
    ON flight_custom_field_archive(flight_id);

CREATE INDEX IF NOT EXISTS idx_flight_custom_field_archive_field_key
    ON flight_custom_field_archive(field_key);

-- =====================================================
-- 2) Backfill legs (active + archived)
-- =====================================================

INSERT INTO flight_legs (
    leg_id,
    flight_id,
    leg_type,
    flight_no,
    flight_type,
    mission,
    origin_code,
    destination_code,
    origin_name,
    destination_name,
    is_vip,
    stand_type,
    scheduled_time,
    created_at,
    updated_at
)
SELECT
    SUBSTRING(MD5(f.flight_id || ':inbound'), 1, 26) AS leg_id,
    f.flight_id,
    'inbound' AS leg_type,
    f.flight_no_inbound AS flight_no,
    CASE COALESCE((f.flight_types)[1], 0)
        WHEN 1 THEN 'intl'
        WHEN 2 THEN 'region'
        ELSE 'domestic'
    END AS flight_type,
    CASE
        WHEN f.missions IS NOT NULL AND array_length(f.missions, 1) >= 1 AND (f.missions)[1] IS NOT NULL
            THEN (f.missions)[1]
        ELSE NULL
    END AS mission,
    CASE WHEN f.origins IS NOT NULL AND array_length(f.origins, 1) >= 1 THEN (f.origins)[1] ELSE NULL END AS origin_code,
    CASE WHEN f.destinations IS NOT NULL AND array_length(f.destinations, 1) >= 1 THEN (f.destinations)[1] ELSE NULL END AS destination_code,
    CASE WHEN f.origins_name IS NOT NULL AND array_length(f.origins_name, 1) >= 1 THEN (f.origins_name)[1] ELSE NULL END AS origin_name,
    CASE WHEN f.destinations_name IS NOT NULL AND array_length(f.destinations_name, 1) >= 1 THEN (f.destinations_name)[1] ELSE NULL END AS destination_name,
    COALESCE((f.is_vip)[1], FALSE) AS is_vip,
    NULLIF(BTRIM(SPLIT_PART(COALESCE(f.stand_types, ''), ',', 1)), '') AS stand_type,
    f.scheduled_arrival AS scheduled_time,
    COALESCE(f.created_at, CURRENT_TIMESTAMP) AS created_at,
    COALESCE(f.updated_at, CURRENT_TIMESTAMP) AS updated_at
FROM flights f
WHERE f.flight_no_inbound IS NOT NULL
ON CONFLICT (flight_id, leg_type) DO UPDATE SET
    flight_no = EXCLUDED.flight_no,
    flight_type = EXCLUDED.flight_type,
    mission = EXCLUDED.mission,
    origin_code = EXCLUDED.origin_code,
    destination_code = EXCLUDED.destination_code,
    origin_name = EXCLUDED.origin_name,
    destination_name = EXCLUDED.destination_name,
    is_vip = EXCLUDED.is_vip,
    stand_type = EXCLUDED.stand_type,
    scheduled_time = EXCLUDED.scheduled_time,
    updated_at = CURRENT_TIMESTAMP;

INSERT INTO flight_legs (
    leg_id,
    flight_id,
    leg_type,
    flight_no,
    flight_type,
    mission,
    origin_code,
    destination_code,
    origin_name,
    destination_name,
    is_vip,
    stand_type,
    scheduled_time,
    created_at,
    updated_at
)
SELECT
    SUBSTRING(MD5(f.flight_id || ':outbound'), 1, 26) AS leg_id,
    f.flight_id,
    'outbound' AS leg_type,
    f.flight_no_outbound AS flight_no,
    CASE COALESCE((f.flight_types)[2], 0)
        WHEN 1 THEN 'intl'
        WHEN 2 THEN 'region'
        ELSE 'domestic'
    END AS flight_type,
    CASE
        WHEN f.missions IS NOT NULL AND array_length(f.missions, 1) >= 2 AND (f.missions)[2] IS NOT NULL
            THEN (f.missions)[2]
        ELSE NULL
    END AS mission,
    CASE WHEN f.origins IS NOT NULL AND array_length(f.origins, 1) >= 1 THEN (f.origins)[1] ELSE NULL END AS origin_code,
    CASE WHEN f.destinations IS NOT NULL AND array_length(f.destinations, 1) >= 1 THEN (f.destinations)[1] ELSE NULL END AS destination_code,
    CASE WHEN f.origins_name IS NOT NULL AND array_length(f.origins_name, 1) >= 1 THEN (f.origins_name)[1] ELSE NULL END AS origin_name,
    CASE WHEN f.destinations_name IS NOT NULL AND array_length(f.destinations_name, 1) >= 1 THEN (f.destinations_name)[1] ELSE NULL END AS destination_name,
    COALESCE((f.is_vip)[2], FALSE) AS is_vip,
    NULLIF(BTRIM(SPLIT_PART(COALESCE(f.stand_types, ''), ',', 2)), '') AS stand_type,
    f.scheduled_departure AS scheduled_time,
    COALESCE(f.created_at, CURRENT_TIMESTAMP) AS created_at,
    COALESCE(f.updated_at, CURRENT_TIMESTAMP) AS updated_at
FROM flights f
WHERE f.flight_no_outbound IS NOT NULL
ON CONFLICT (flight_id, leg_type) DO UPDATE SET
    flight_no = EXCLUDED.flight_no,
    flight_type = EXCLUDED.flight_type,
    mission = EXCLUDED.mission,
    origin_code = EXCLUDED.origin_code,
    destination_code = EXCLUDED.destination_code,
    origin_name = EXCLUDED.origin_name,
    destination_name = EXCLUDED.destination_name,
    is_vip = EXCLUDED.is_vip,
    stand_type = EXCLUDED.stand_type,
    scheduled_time = EXCLUDED.scheduled_time,
    updated_at = CURRENT_TIMESTAMP;

DO $$
BEGIN
    IF to_regclass('public.archived_flights') IS NOT NULL THEN
        INSERT INTO flight_legs (
            leg_id,
            flight_id,
            leg_type,
            flight_no,
            flight_type,
            mission,
            origin_code,
            destination_code,
            origin_name,
            destination_name,
            is_vip,
            stand_type,
            scheduled_time,
            created_at,
            updated_at
        )
        SELECT
            SUBSTRING(MD5(a.flight_id || ':inbound'), 1, 26) AS leg_id,
            a.flight_id,
            'inbound' AS leg_type,
            a.flight_no_inbound AS flight_no,
            CASE COALESCE((a.flight_types)[1], 0)
                WHEN 1 THEN 'intl'
                WHEN 2 THEN 'region'
                ELSE 'domestic'
            END AS flight_type,
            CASE
                WHEN a.missions IS NOT NULL AND array_length(a.missions, 1) >= 1 AND (a.missions)[1] IS NOT NULL
                    THEN (a.missions)[1]
                ELSE NULL
            END AS mission,
            CASE WHEN a.origins IS NOT NULL AND array_length(a.origins, 1) >= 1 THEN (a.origins)[1] ELSE NULL END AS origin_code,
            CASE WHEN a.destinations IS NOT NULL AND array_length(a.destinations, 1) >= 1 THEN (a.destinations)[1] ELSE NULL END AS destination_code,
            CASE WHEN a.origins_name IS NOT NULL AND array_length(a.origins_name, 1) >= 1 THEN (a.origins_name)[1] ELSE NULL END AS origin_name,
            CASE WHEN a.destinations_name IS NOT NULL AND array_length(a.destinations_name, 1) >= 1 THEN (a.destinations_name)[1] ELSE NULL END AS destination_name,
            COALESCE((a.is_vip)[1], FALSE) AS is_vip,
            NULLIF(BTRIM(SPLIT_PART(COALESCE(a.stand_types, ''), ',', 1)), '') AS stand_type,
            a.scheduled_arrival AS scheduled_time,
            COALESCE(a.created_at, CURRENT_TIMESTAMP) AS created_at,
            COALESCE(a.updated_at, CURRENT_TIMESTAMP) AS updated_at
        FROM archived_flights a
        WHERE a.flight_no_inbound IS NOT NULL
        ON CONFLICT (flight_id, leg_type) DO UPDATE SET
            flight_no = EXCLUDED.flight_no,
            flight_type = EXCLUDED.flight_type,
            mission = EXCLUDED.mission,
            origin_code = EXCLUDED.origin_code,
            destination_code = EXCLUDED.destination_code,
            origin_name = EXCLUDED.origin_name,
            destination_name = EXCLUDED.destination_name,
            is_vip = EXCLUDED.is_vip,
            stand_type = EXCLUDED.stand_type,
            scheduled_time = EXCLUDED.scheduled_time,
            updated_at = CURRENT_TIMESTAMP;

        INSERT INTO flight_legs (
            leg_id,
            flight_id,
            leg_type,
            flight_no,
            flight_type,
            mission,
            origin_code,
            destination_code,
            origin_name,
            destination_name,
            is_vip,
            stand_type,
            scheduled_time,
            created_at,
            updated_at
        )
        SELECT
            SUBSTRING(MD5(a.flight_id || ':outbound'), 1, 26) AS leg_id,
            a.flight_id,
            'outbound' AS leg_type,
            a.flight_no_outbound AS flight_no,
            CASE COALESCE((a.flight_types)[2], 0)
                WHEN 1 THEN 'intl'
                WHEN 2 THEN 'region'
                ELSE 'domestic'
            END AS flight_type,
            CASE
                WHEN a.missions IS NOT NULL AND array_length(a.missions, 1) >= 2 AND (a.missions)[2] IS NOT NULL
                    THEN (a.missions)[2]
                ELSE NULL
            END AS mission,
            CASE WHEN a.origins IS NOT NULL AND array_length(a.origins, 1) >= 1 THEN (a.origins)[1] ELSE NULL END AS origin_code,
            CASE WHEN a.destinations IS NOT NULL AND array_length(a.destinations, 1) >= 1 THEN (a.destinations)[1] ELSE NULL END AS destination_code,
            CASE WHEN a.origins_name IS NOT NULL AND array_length(a.origins_name, 1) >= 1 THEN (a.origins_name)[1] ELSE NULL END AS origin_name,
            CASE WHEN a.destinations_name IS NOT NULL AND array_length(a.destinations_name, 1) >= 1 THEN (a.destinations_name)[1] ELSE NULL END AS destination_name,
            COALESCE((a.is_vip)[2], FALSE) AS is_vip,
            NULLIF(BTRIM(SPLIT_PART(COALESCE(a.stand_types, ''), ',', 2)), '') AS stand_type,
            a.scheduled_departure AS scheduled_time,
            COALESCE(a.created_at, CURRENT_TIMESTAMP) AS created_at,
            COALESCE(a.updated_at, CURRENT_TIMESTAMP) AS updated_at
        FROM archived_flights a
        WHERE a.flight_no_outbound IS NOT NULL
        ON CONFLICT (flight_id, leg_type) DO UPDATE SET
            flight_no = EXCLUDED.flight_no,
            flight_type = EXCLUDED.flight_type,
            mission = EXCLUDED.mission,
            origin_code = EXCLUDED.origin_code,
            destination_code = EXCLUDED.destination_code,
            origin_name = EXCLUDED.origin_name,
            destination_name = EXCLUDED.destination_name,
            is_vip = EXCLUDED.is_vip,
            stand_type = EXCLUDED.stand_type,
            scheduled_time = EXCLUDED.scheduled_time,
            updated_at = CURRENT_TIMESTAMP;
    END IF;
END $$;

-- =====================================================
-- 3) Backfill timeline from legacy flattened milestone fields
-- =====================================================

WITH source_flights AS (
    SELECT
        'flights'::text AS source_table,
        f.flight_id,
        COALESCE(f.custom_fields, '{}'::jsonb) AS custom_fields,
        f.start_boarding_time,
        f.end_boarding_time,
        f.codt,
        f.wheel_chocks_time,
        f.cabin_door_open_time,
        f.deboarding_complete_time,
        f.cleaning_start_time,
        f.cleaning_end_time,
        f.cabin_door_close_time,
        f.cargo_door_close_time,
        f.loading_complete_time,
        f.off_blocks_time,
        f.passengers_ready_time,
        f.boarding_permission_time
    FROM flights f
),
expanded AS (
    SELECT
        sf.source_table,
        sf.flight_id,
        milestones.milestone_code,
        milestones.occurred_at,
        milestones.leg_type,
        sf.custom_fields
    FROM source_flights sf
    CROSS JOIN LATERAL (
        VALUES
            ('start_boarding_time', sf.start_boarding_time, 'outbound'),
            ('end_boarding_time', sf.end_boarding_time, 'outbound'),
            ('codt', sf.codt, 'outbound'),
            ('wheel_chocks_time', sf.wheel_chocks_time, 'inbound'),
            ('cabin_door_open_time', sf.cabin_door_open_time, 'inbound'),
            ('deboarding_complete_time', sf.deboarding_complete_time, 'inbound'),
            ('cleaning_start_time', sf.cleaning_start_time, 'inbound'),
            ('cleaning_end_time', sf.cleaning_end_time, 'outbound'),
            ('cabin_door_close_time', sf.cabin_door_close_time, 'outbound'),
            ('cargo_door_close_time', sf.cargo_door_close_time, 'outbound'),
            ('loading_complete_time', sf.loading_complete_time, 'outbound'),
            ('off_blocks_time', sf.off_blocks_time, 'outbound'),
            ('passengers_ready_time', sf.passengers_ready_time, 'outbound'),
            ('boarding_permission_time', sf.boarding_permission_time, 'outbound')
    ) AS milestones(milestone_code, occurred_at, leg_type)
    WHERE milestones.occurred_at IS NOT NULL
)
INSERT INTO flight_dispatch_timeline_events (
    timeline_id,
    flight_id,
    milestone_code,
    occurred_at,
    leg_type,
    recorded_by,
    source,
    payload,
    created_at
)
SELECT
    SUBSTRING(MD5(expanded.flight_id || ':' || expanded.milestone_code || ':' || expanded.occurred_at::text), 1, 26) AS timeline_id,
    expanded.flight_id,
    expanded.milestone_code,
    expanded.occurred_at,
    expanded.leg_type,
    NULLIF(BTRIM(expanded.custom_fields ->> (expanded.milestone_code || '_by')), '') AS recorded_by,
    'migration_backfill_v1' AS source,
    jsonb_build_object('source_table', expanded.source_table) AS payload,
    CURRENT_TIMESTAMP AS created_at
FROM expanded
ON CONFLICT (timeline_id) DO NOTHING;

DO $$
BEGIN
    IF to_regclass('public.archived_flights') IS NOT NULL THEN
        WITH source_flights AS (
            SELECT
                'archived_flights'::text AS source_table,
                f.flight_id,
                COALESCE(f.custom_fields, '{}'::jsonb) AS custom_fields,
                f.start_boarding_time,
                f.end_boarding_time,
                f.codt,
                f.wheel_chocks_time,
                f.cabin_door_open_time,
                f.deboarding_complete_time,
                f.cleaning_start_time,
                f.cleaning_end_time,
                f.cabin_door_close_time,
                f.cargo_door_close_time,
                f.loading_complete_time,
                f.off_blocks_time,
                f.passengers_ready_time,
                f.boarding_permission_time
            FROM archived_flights f
        ),
        expanded AS (
            SELECT
                sf.source_table,
                sf.flight_id,
                milestones.milestone_code,
                milestones.occurred_at,
                milestones.leg_type,
                sf.custom_fields
            FROM source_flights sf
            CROSS JOIN LATERAL (
                VALUES
                    ('start_boarding_time', sf.start_boarding_time, 'outbound'),
                    ('end_boarding_time', sf.end_boarding_time, 'outbound'),
                    ('codt', sf.codt, 'outbound'),
                    ('wheel_chocks_time', sf.wheel_chocks_time, 'inbound'),
                    ('cabin_door_open_time', sf.cabin_door_open_time, 'inbound'),
                    ('deboarding_complete_time', sf.deboarding_complete_time, 'inbound'),
                    ('cleaning_start_time', sf.cleaning_start_time, 'inbound'),
                    ('cleaning_end_time', sf.cleaning_end_time, 'outbound'),
                    ('cabin_door_close_time', sf.cabin_door_close_time, 'outbound'),
                    ('cargo_door_close_time', sf.cargo_door_close_time, 'outbound'),
                    ('loading_complete_time', sf.loading_complete_time, 'outbound'),
                    ('off_blocks_time', sf.off_blocks_time, 'outbound'),
                    ('passengers_ready_time', sf.passengers_ready_time, 'outbound'),
                    ('boarding_permission_time', sf.boarding_permission_time, 'outbound')
            ) AS milestones(milestone_code, occurred_at, leg_type)
            WHERE milestones.occurred_at IS NOT NULL
        )
        INSERT INTO flight_dispatch_timeline_events (
            timeline_id,
            flight_id,
            milestone_code,
            occurred_at,
            leg_type,
            recorded_by,
            source,
            payload,
            created_at
        )
        SELECT
            SUBSTRING(MD5(expanded.flight_id || ':' || expanded.milestone_code || ':' || expanded.occurred_at::text), 1, 26) AS timeline_id,
            expanded.flight_id,
            expanded.milestone_code,
            expanded.occurred_at,
            expanded.leg_type,
            NULLIF(BTRIM(expanded.custom_fields ->> (expanded.milestone_code || '_by')), '') AS recorded_by,
            'migration_backfill_v1' AS source,
            jsonb_build_object('source_table', expanded.source_table) AS payload,
            CURRENT_TIMESTAMP AS created_at
        FROM expanded
        ON CONFLICT (timeline_id) DO NOTHING;
    END IF;
END $$;

-- =====================================================
-- 4) Archive custom_fields
-- =====================================================

INSERT INTO flight_custom_field_archive (
    archive_id,
    flight_id,
    field_key,
    field_value_json,
    migrated_at
)
SELECT
    SUBSTRING(MD5(f.flight_id || ':' || kv.key), 1, 26) AS archive_id,
    f.flight_id,
    kv.key AS field_key,
    kv.value AS field_value_json,
    CURRENT_TIMESTAMP AS migrated_at
FROM flights f
CROSS JOIN LATERAL jsonb_each(COALESCE(f.custom_fields, '{}'::jsonb)) AS kv(key, value)
ON CONFLICT (archive_id) DO NOTHING;

DO $$
BEGIN
    IF to_regclass('public.archived_flights') IS NOT NULL THEN
        INSERT INTO flight_custom_field_archive (
            archive_id,
            flight_id,
            field_key,
            field_value_json,
            migrated_at
        )
        SELECT
            SUBSTRING(MD5(a.flight_id || ':' || kv.key), 1, 26) AS archive_id,
            a.flight_id,
            kv.key AS field_key,
            kv.value AS field_value_json,
            CURRENT_TIMESTAMP AS migrated_at
        FROM archived_flights a
        CROSS JOIN LATERAL jsonb_each(COALESCE(a.custom_fields, '{}'::jsonb)) AS kv(key, value)
        ON CONFLICT (archive_id) DO NOTHING;
    END IF;
END $$;

-- =====================================================
-- 5) Rebuild event stream + outbox to v2 seed events
-- =====================================================

DO $$
DECLARE
    legacy_suffix TEXT := to_char(CURRENT_DATE, 'YYYYMMDD');
    legacy_table_name TEXT := format('flight_state_changes_legacy_%s', legacy_suffix);
BEGIN
    IF to_regclass('public.flight_state_changes') IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM information_schema.columns
           WHERE table_schema = 'public'
             AND table_name = 'flight_state_changes'
             AND column_name = 'id'
       )
       AND to_regclass('public.' || legacy_table_name) IS NULL THEN
        EXECUTE format(
            'ALTER TABLE flight_state_changes RENAME TO flight_state_changes_legacy_%s',
            legacy_suffix
        );
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS flight_state_changes (
    id SERIAL PRIMARY KEY,
    change_id VARCHAR(26) NOT NULL UNIQUE,
    flight_id VARCHAR(26) NOT NULL,
    flight_number VARCHAR(7),
    change_type VARCHAR(255) NOT NULL,
    change_data JSONB NOT NULL,
    metadata JSONB,
    version INTEGER NOT NULL,
    occurred_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(flight_id, version)
);

CREATE INDEX IF NOT EXISTS idx_fsc_flight_id ON flight_state_changes(flight_id);
CREATE INDEX IF NOT EXISTS idx_fsc_occurred_at ON flight_state_changes(occurred_at);

DELETE FROM domain_event_outbox WHERE aggregate_type = 'flight';

INSERT INTO flight_state_changes (
    change_id,
    flight_id,
    flight_number,
    change_type,
    change_data,
    occurred_at,
    version
)
SELECT
    SUBSTRING(MD5(f.flight_id || ':created_v2'), 1, 26) AS change_id,
    f.flight_id,
    LEFT(COALESCE(f.flight_number, ''), 7) AS flight_number,
    'created_v2' AS change_type,
    jsonb_build_object(
        'type', 'created_v2',
        'occurred_at', COALESCE(f.created_at, CURRENT_TIMESTAMP),
        'data', jsonb_strip_nulls(
            jsonb_build_object(
                'flight_id', f.flight_id,
                'flight_number', f.flight_number,
                'airline_code', f.airline_code,
                'registration', f.registration,
                'status', f.status,
                'scheduled_departure', f.scheduled_departure,
                'scheduled_arrival', f.scheduled_arrival,
                'estimated_departure', f.estimated_departure,
                'estimated_arrival', f.estimated_arrival,
                'actual_departure', f.actual_departure,
                'actual_arrival', f.actual_arrival,
                'stand', f.stand,
                'gate', f.gate,
                'terminal', f.terminal,
                'position', f.position,
                'baggage_carousel', f.baggage_carousel,
                'aircraft_type_detail', f.aircraft_type_detail,
                'is_quick_turnaround', f.is_quick_turnaround,
                'has_boarding_restriction', f.has_boarding_restriction,
                'is_commercial_signed', f.is_commercial_signed,
                'flight_remarks', f.flight_remarks,
                'load_planning_remarks', f.load_planning_remarks,
                'aircraft_maintenance_remarks', f.aircraft_maintenance_remarks,
                'aircraft_check_remarks', f.aircraft_check_remarks,
                'inbound_leg', (
                    SELECT to_jsonb(l)
                    FROM flight_legs l
                    WHERE l.flight_id = f.flight_id AND l.leg_type = 'inbound'
                    ORDER BY l.updated_at DESC NULLS LAST, l.created_at DESC NULLS LAST
                    LIMIT 1
                ),
                'outbound_leg', (
                    SELECT to_jsonb(l)
                    FROM flight_legs l
                    WHERE l.flight_id = f.flight_id AND l.leg_type = 'outbound'
                    ORDER BY l.updated_at DESC NULLS LAST, l.created_at DESC NULLS LAST
                    LIMIT 1
                )
            )
        )
    ) AS change_data,
    COALESCE(f.created_at, CURRENT_TIMESTAMP) AS occurred_at,
    1 AS version
FROM flights f
ON CONFLICT (change_id) DO NOTHING;

INSERT INTO domain_event_outbox (
    event_id,
    aggregate_type,
    aggregate_id,
    event_type,
    payload,
    occurred_at,
    source_change_id
)
SELECT
    SUBSTRING(MD5(fsc.change_id || ':outbox'), 1, 26) AS event_id,
    'flight' AS aggregate_type,
    fsc.flight_id AS aggregate_id,
    'flight.created_v2' AS event_type,
    fsc.change_data AS payload,
    fsc.occurred_at,
    fsc.change_id AS source_change_id
FROM flight_state_changes fsc
WHERE fsc.change_type = 'created_v2'
ON CONFLICT (source_change_id) DO NOTHING;

-- =====================================================
-- 6) AI/KPI read models cut over to anomaly-domain semantics
-- =====================================================

DROP VIEW IF EXISTS ai_query.v_daily_kpi;
DROP MATERIALIZED VIEW IF EXISTS mv_daily_flight_kpi;

CREATE MATERIALIZED VIEW mv_daily_flight_kpi AS
WITH base AS (
    SELECT
        DATE(f.scheduled_departure AT TIME ZONE 'Asia/Shanghai') AS flight_date,
        f.flight_id,
        f.scheduled_departure,
        f.scheduled_arrival,
        f.estimated_departure,
        f.estimated_arrival,
        f.actual_departure,
        f.actual_arrival
    FROM flights f
    WHERE f.scheduled_departure IS NOT NULL
),
open_anomalies AS (
    SELECT DISTINCT a.flight_id
    FROM anomalies a
    WHERE a.status IN ('open', 'acknowledged')
)
SELECT
    b.flight_date,
    COUNT(*) AS total_flights,
    COUNT(*) FILTER (
        WHERE b.actual_departure IS NOT NULL AND b.actual_arrival IS NOT NULL
    ) AS completed_flights,
    AVG(EXTRACT(EPOCH FROM (b.actual_departure - b.actual_arrival)) / 60)
        FILTER (
            WHERE b.actual_departure IS NOT NULL AND b.actual_arrival IS NOT NULL
        ) AS avg_turnaround_minutes,
    PERCENTILE_CONT(0.9) WITHIN GROUP (
        ORDER BY EXTRACT(EPOCH FROM (b.actual_departure - b.actual_arrival)) / 60
    ) FILTER (
        WHERE b.actual_departure IS NOT NULL AND b.actual_arrival IS NOT NULL
    ) AS p90_turnaround_minutes,
    COUNT(*) FILTER (
        WHERE b.actual_departure <= b.scheduled_departure + INTERVAL '15 minutes'
    )::FLOAT
        / NULLIF(COUNT(*) FILTER (WHERE b.actual_departure IS NOT NULL), 0)
        AS on_time_departure_rate,
    COUNT(*) FILTER (
        WHERE b.actual_arrival <= b.scheduled_arrival + INTERVAL '15 minutes'
    )::FLOAT
        / NULLIF(COUNT(*) FILTER (WHERE b.actual_arrival IS NOT NULL), 0)
        AS on_time_arrival_rate,
    COUNT(*) FILTER (WHERE oa.flight_id IS NOT NULL)::FLOAT
        / NULLIF(COUNT(*), 0)
        AS abnormal_ratio
FROM base b
LEFT JOIN open_anomalies oa ON oa.flight_id = b.flight_id
GROUP BY b.flight_date;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_daily_flight_kpi_date
    ON mv_daily_flight_kpi(flight_date);

DROP VIEW IF EXISTS ai_query.v_ops_overview;
DROP VIEW IF EXISTS ai_query.v_flights;

CREATE VIEW ai_query.v_flights AS
WITH open_anomaly AS (
    SELECT
        a.flight_id,
        COUNT(*) AS open_anomaly_count
    FROM anomalies a
    WHERE a.status IN ('open', 'acknowledged')
    GROUP BY a.flight_id
)
SELECT
    f.flight_id,
    f.flight_number,
    f.airline_code,
    f.status,
    f.scheduled_departure,
    f.estimated_departure,
    f.actual_departure,
    f.scheduled_arrival,
    f.estimated_arrival,
    f.actual_arrival,
    f.execution_date,
    f.workspace_date,
    f.stand,
    f.gate,
    f.terminal,
    COALESCE(oa.open_anomaly_count, 0) AS open_anomaly_count,
    (COALESCE(oa.open_anomaly_count, 0) > 0) AS has_open_anomaly,
    inbound_leg.leg_json AS inbound_leg_json,
    outbound_leg.leg_json AS outbound_leg_json,
    CASE
        WHEN f.estimated_departure IS NOT NULL AND f.scheduled_departure IS NOT NULL THEN
            ROUND(EXTRACT(EPOCH FROM (f.estimated_departure - f.scheduled_departure)) / 60.0, 2)
        ELSE NULL
    END AS delay_minutes,
    f.created_at,
    f.updated_at
FROM public.flights AS f
LEFT JOIN open_anomaly oa ON oa.flight_id = f.flight_id
LEFT JOIN LATERAL (
    SELECT to_jsonb(l) AS leg_json
    FROM flight_legs l
    WHERE l.flight_id = f.flight_id AND l.leg_type = 'inbound'
    ORDER BY l.updated_at DESC NULLS LAST, l.created_at DESC NULLS LAST
    LIMIT 1
) inbound_leg ON TRUE
LEFT JOIN LATERAL (
    SELECT to_jsonb(l) AS leg_json
    FROM flight_legs l
    WHERE l.flight_id = f.flight_id AND l.leg_type = 'outbound'
    ORDER BY l.updated_at DESC NULLS LAST, l.created_at DESC NULLS LAST
    LIMIT 1
) outbound_leg ON TRUE;

CREATE OR REPLACE VIEW ai_query.v_daily_kpi AS
SELECT
    k.flight_date,
    k.total_flights,
    k.completed_flights,
    k.avg_turnaround_minutes,
    k.p90_turnaround_minutes,
    k.on_time_departure_rate,
    k.on_time_arrival_rate,
    k.abnormal_ratio
FROM public.mv_daily_flight_kpi AS k;

CREATE VIEW ai_query.v_ops_overview AS
SELECT
    (SELECT COUNT(*) FROM ai_query.v_flights) AS flights_total,
    (
        SELECT COUNT(*)
        FROM ai_query.v_flights
        WHERE status NOT IN (7, 8, 9)
    ) AS flights_active,
    (
        SELECT COUNT(*)
        FROM ai_query.v_anomalies
        WHERE status = 'open'
    ) AS anomalies_open,
    (
        SELECT COUNT(*)
        FROM ai_query.v_todos
        WHERE is_deleted = FALSE AND status IN ('待办', '进行中')
    ) AS todos_open,
    CURRENT_TIMESTAMP AS snapshot_at;

-- =====================================================
-- 7) Drop legacy columns from flights / archived_flights
-- =====================================================

ALTER TABLE flights
    DROP COLUMN IF EXISTS flight_no_inbound,
    DROP COLUMN IF EXISTS flight_no_outbound,
    DROP COLUMN IF EXISTS missions,
    DROP COLUMN IF EXISTS flight_types,
    DROP COLUMN IF EXISTS origins,
    DROP COLUMN IF EXISTS destinations,
    DROP COLUMN IF EXISTS origins_name,
    DROP COLUMN IF EXISTS destinations_name,
    DROP COLUMN IF EXISTS stand_types,
    DROP COLUMN IF EXISTS is_vip,
    DROP COLUMN IF EXISTS start_boarding_time,
    DROP COLUMN IF EXISTS end_boarding_time,
    DROP COLUMN IF EXISTS codt,
    DROP COLUMN IF EXISTS off_blocks_time,
    DROP COLUMN IF EXISTS passengers_ready_time,
    DROP COLUMN IF EXISTS boarding_permission_time,
    DROP COLUMN IF EXISTS inbound_abnormal,
    DROP COLUMN IF EXISTS outbound_abnormal,
    DROP COLUMN IF EXISTS inbound_abnormal_reason,
    DROP COLUMN IF EXISTS outbound_abnormal_reason,
    DROP COLUMN IF EXISTS custom_fields;

DO $$
BEGIN
    IF to_regclass('public.archived_flights') IS NOT NULL THEN
        EXECUTE '
            ALTER TABLE archived_flights
                DROP COLUMN IF EXISTS flight_no_inbound,
                DROP COLUMN IF EXISTS flight_no_outbound,
                DROP COLUMN IF EXISTS missions,
                DROP COLUMN IF EXISTS flight_types,
                DROP COLUMN IF EXISTS origins,
                DROP COLUMN IF EXISTS destinations,
                DROP COLUMN IF EXISTS origins_name,
                DROP COLUMN IF EXISTS destinations_name,
                DROP COLUMN IF EXISTS stand_types,
                DROP COLUMN IF EXISTS is_vip,
                DROP COLUMN IF EXISTS start_boarding_time,
                DROP COLUMN IF EXISTS end_boarding_time,
                DROP COLUMN IF EXISTS codt,
                DROP COLUMN IF EXISTS off_blocks_time,
                DROP COLUMN IF EXISTS passengers_ready_time,
                DROP COLUMN IF EXISTS boarding_permission_time,
                DROP COLUMN IF EXISTS inbound_abnormal,
                DROP COLUMN IF EXISTS outbound_abnormal,
                DROP COLUMN IF EXISTS inbound_abnormal_reason,
                DROP COLUMN IF EXISTS outbound_abnormal_reason,
                DROP COLUMN IF EXISTS custom_fields
        ';
    END IF;
END $$;

DO $$
DECLARE
    legacy_table_name TEXT := format('public.flight_state_changes_legacy_%s', to_char(CURRENT_DATE, 'YYYYMMDD'));
BEGIN
    IF to_regclass(legacy_table_name) IS NOT NULL THEN
        EXECUTE format('DROP TABLE %s', legacy_table_name);
    END IF;
END $$;



