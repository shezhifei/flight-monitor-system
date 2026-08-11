-- Backfill missing flight seed events for write-side replay.


ALTER TABLE flights
    ADD COLUMN IF NOT EXISTS flight_no_inbound VARCHAR(16),
    ADD COLUMN IF NOT EXISTS flight_no_outbound VARCHAR(16),
    ADD COLUMN IF NOT EXISTS flight_types SMALLINT[],
    ADD COLUMN IF NOT EXISTS origins VARCHAR(8)[],
    ADD COLUMN IF NOT EXISTS destinations VARCHAR(8)[],
    ADD COLUMN IF NOT EXISTS origins_name VARCHAR(128)[],
    ADD COLUMN IF NOT EXISTS destinations_name VARCHAR(128)[],
    ADD COLUMN IF NOT EXISTS is_vip BOOLEAN[],
    ADD COLUMN IF NOT EXISTS inbound_abnormal BOOLEAN,
    ADD COLUMN IF NOT EXISTS outbound_abnormal BOOLEAN,
    ADD COLUMN IF NOT EXISTS inbound_abnormal_reason TEXT,
    ADD COLUMN IF NOT EXISTS outbound_abnormal_reason TEXT,
    ADD COLUMN IF NOT EXISTS custom_fields JSONB DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS start_boarding_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS end_boarding_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS codt TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS off_blocks_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS passengers_ready_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS boarding_permission_time TIMESTAMPTZ;

WITH missing_flights AS (
    SELECT f.*
    FROM flights f
    LEFT JOIN (
        SELECT DISTINCT flight_id
        FROM flight_state_changes
    ) sc ON sc.flight_id = f.flight_id
    WHERE sc.flight_id IS NULL
),
seed_payloads AS (
    SELECT
        mf.flight_id,
        COALESCE(mf.flight_number, mf.flight_no_outbound, mf.flight_no_inbound) AS flight_number,
        COALESCE(mf.created_at, NOW()) AS occurred_at,
        jsonb_build_object(
            'type', 'flight_created',
            'occurred_at', TO_CHAR((COALESCE(mf.created_at, NOW()) AT TIME ZONE 'UTC'), 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
            'data', jsonb_build_object(
                'data',
                jsonb_strip_nulls(
                    jsonb_build_object(
                        'flight_id', mf.flight_id,
                        'flight_no_inbound', mf.flight_no_inbound,
                        'flight_no_outbound', mf.flight_no_outbound,
                        'flight_number', mf.flight_number,
                        'status', mf.status,
                        'airline_code', mf.airline_code,
                        'registration', mf.registration,
                        'flight_types', TO_JSONB(mf.flight_types),
                        'origins', TO_JSONB(mf.origins),
                        'destinations', TO_JSONB(mf.destinations),
                        'origins_name', TO_JSONB(mf.origins_name),
                        'destinations_name', TO_JSONB(mf.destinations_name),
                        'aircraft_type_detail', mf.aircraft_type_detail,
                        'scheduled_departure', mf.scheduled_departure,
                        'scheduled_arrival', mf.scheduled_arrival,
                        'estimated_departure', mf.estimated_departure,
                        'estimated_arrival', mf.estimated_arrival,
                        'actual_departure', mf.actual_departure,
                        'actual_arrival', mf.actual_arrival,
                        'stand', mf.stand,
                        'gate', mf.gate
                    )
                    || jsonb_build_object(
                        'terminal', mf.terminal,
                        'position', mf.position,
                        'baggage_carousel', mf.baggage_carousel,
                        'is_vip', TO_JSONB(mf.is_vip),
                        'has_boarding_restriction', mf.has_boarding_restriction,
                        'is_quick_turnaround', mf.is_quick_turnaround,
                        'is_commercial_signed', mf.is_commercial_signed,
                        'inbound_abnormal', mf.inbound_abnormal,
                        'outbound_abnormal', mf.outbound_abnormal,
                        'inbound_abnormal_reason', mf.inbound_abnormal_reason,
                        'outbound_abnormal_reason', mf.outbound_abnormal_reason,
                        'custom_fields', COALESCE(mf.custom_fields, '{}'::jsonb),
                        'start_boarding_time', mf.start_boarding_time,
                        'end_boarding_time', mf.end_boarding_time,
                        'codt', mf.codt,
                        'wheel_chocks_time', mf.wheel_chocks_time,
                        'cabin_door_open_time', mf.cabin_door_open_time,
                        'deboarding_complete_time', mf.deboarding_complete_time,
                        'cleaning_start_time', mf.cleaning_start_time,
                        'cleaning_end_time', mf.cleaning_end_time,
                        'cabin_door_close_time', mf.cabin_door_close_time
                    )
                    || jsonb_build_object(
                        'cargo_door_close_time', mf.cargo_door_close_time,
                        'loading_complete_time', mf.loading_complete_time,
                        'off_blocks_time', mf.off_blocks_time,
                        'passengers_ready_time', mf.passengers_ready_time,
                        'boarding_permission_time', mf.boarding_permission_time,
                        'flight_remarks', mf.flight_remarks,
                        'load_planning_remarks', mf.load_planning_remarks,
                        'aircraft_maintenance_remarks', mf.aircraft_maintenance_remarks,
                        'aircraft_check_remarks', mf.aircraft_check_remarks
                    )
                )
            )
        ) AS change_data
    FROM missing_flights mf
)
INSERT INTO flight_state_changes (
    change_id,
    flight_id,
    flight_number,
    change_type,
    change_data,
    metadata,
    version,
    occurred_at
)
SELECT
    LEFT(MD5(sp.flight_id || ':' || CLOCK_TIMESTAMP()::text), 26) AS change_id,
    sp.flight_id,
    sp.flight_number,
    'flight_created' AS change_type,
    sp.change_data,
    jsonb_build_object(
        'source', 'migration_028_backfill_missing_flight_seed_events',
        'reason', 'missing_event_stream'
    ) AS metadata,
    1 AS version,
    sp.occurred_at
FROM seed_payloads sp
ON CONFLICT (flight_id, version) DO NOTHING;



