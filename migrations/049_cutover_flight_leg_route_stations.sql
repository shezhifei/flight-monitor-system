
ALTER TABLE flight_legs
    ADD COLUMN IF NOT EXISTS origin_stations JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS destination_stations JSONB NOT NULL DEFAULT '[]'::jsonb;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'flight_legs'
          AND column_name = 'origin_code'
    ) THEN
        UPDATE flight_legs
        SET origin_stations = CASE
                WHEN leg_type = 'inbound' AND NULLIF(BTRIM(COALESCE(origin_code, '')), '') IS NOT NULL
                    THEN jsonb_build_array(
                        jsonb_build_object(
                            'code', UPPER(BTRIM(origin_code)),
                            'name', NULLIF(BTRIM(COALESCE(origin_name, '')), '')
                        )
                    )
                ELSE origin_stations
            END,
            destination_stations = CASE
                WHEN leg_type = 'outbound' AND NULLIF(BTRIM(COALESCE(destination_code, '')), '') IS NOT NULL
                    THEN jsonb_build_array(
                        jsonb_build_object(
                            'code', UPPER(BTRIM(destination_code)),
                            'name', NULLIF(BTRIM(COALESCE(destination_name, '')), '')
                        )
                    )
                ELSE destination_stations
            END;

        ALTER TABLE flight_legs
            DROP COLUMN IF EXISTS origin_code,
            DROP COLUMN IF EXISTS destination_code,
            DROP COLUMN IF EXISTS origin_name,
            DROP COLUMN IF EXISTS destination_name;
    END IF;
END $$;

