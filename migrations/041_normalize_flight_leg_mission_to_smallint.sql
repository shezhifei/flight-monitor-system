
DO $$
DECLARE
    mission_data_type TEXT;
    invalid_count INTEGER := 0;
BEGIN
    SELECT data_type
    INTO mission_data_type
    FROM information_schema.columns
    WHERE table_schema = 'public'
      AND table_name = 'flight_legs'
      AND column_name = 'mission';

    IF mission_data_type IS NULL THEN
        RAISE EXCEPTION 'flight_legs.mission column does not exist';
    END IF;

    IF mission_data_type <> 'smallint' THEN
        UPDATE flight_legs
        SET mission = CASE
            WHEN mission IS NULL OR BTRIM(mission) = '' THEN NULL
            WHEN BTRIM(mission) ~ '^\d+$' THEN BTRIM(mission)
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'A/V' THEN '1'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'B/F' THEN '2'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'B/W' THEN '3'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'C/B' THEN '4'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'D/M' THEN '5'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'D/Y' THEN '6'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'F/J' THEN '7'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'H/G' THEN '8'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'H/Y' THEN '9'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'J/B' THEN '10'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'K/L' THEN '11'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'L/W' THEN '12'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'N/M' THEN '13'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'R/Z' THEN '14'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'S/F' THEN '15'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'U/H' THEN '16'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'VIP' THEN '17'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'X/L' THEN '18'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) IN ('O/F', '0/F') THEN '19'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) IN ('W/Z', 'W', 'Z') THEN '20'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'Z/P' THEN '21'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'Z/F' THEN '22'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'Y/Z' THEN '23'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'W/A' THEN '24'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'S/Q' THEN '25'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'H/F' THEN '26'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'X/X' THEN '27'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'OVERFLIGHT' THEN '28'
            WHEN UPPER(REPLACE(REPLACE(BTRIM(mission), '／', '/'), ' ', '_')) = 'TECH_STOP' THEN '31'
            ELSE mission
        END;

        SELECT COUNT(*)
        INTO invalid_count
        FROM flight_legs
        WHERE mission IS NOT NULL
          AND NOT (BTRIM(mission) ~ '^\d+$');

        IF invalid_count > 0 THEN
            RAISE EXCEPTION 'flight_legs.mission contains % unmapped legacy values', invalid_count;
        END IF;

        ALTER TABLE flight_legs
            ALTER COLUMN mission TYPE SMALLINT
            USING NULLIF(BTRIM(mission), '')::SMALLINT;
    END IF;
END $$;

ALTER TABLE flight_legs
    DROP CONSTRAINT IF EXISTS chk_flight_legs_mission;

ALTER TABLE flight_legs
    ADD CONSTRAINT chk_flight_legs_mission
    CHECK (mission IS NULL OR mission IN (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 31));



