ALTER TABLE flight_business_cases
    ADD COLUMN IF NOT EXISTS visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS department_name_snapshot VARCHAR(100);

UPDATE flight_business_cases
SET visibility_scope = CASE
        WHEN COALESCE(NULLIF(BTRIM(department_id), ''), NULLIF(BTRIM(department_name_snapshot), '')) IS NOT NULL
            THEN 'DEPARTMENT'
        ELSE 'COMMON'
    END
WHERE visibility_scope IS NULL
   OR BTRIM(visibility_scope) = '';

ALTER TABLE archived_flight_business_cases
    ADD COLUMN IF NOT EXISTS visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS department_name_snapshot VARCHAR(100);

UPDATE archived_flight_business_cases
SET visibility_scope = CASE
        WHEN COALESCE(NULLIF(BTRIM(department_id), ''), NULLIF(BTRIM(department_name_snapshot), '')) IS NOT NULL
            THEN 'DEPARTMENT'
        ELSE 'COMMON'
    END
WHERE visibility_scope IS NULL
   OR BTRIM(visibility_scope) = '';

ALTER TABLE business_case_types
    ADD COLUMN IF NOT EXISTS visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS department_name_snapshot VARCHAR(100);

UPDATE business_case_types
SET visibility_scope = CASE
        WHEN COALESCE(NULLIF(BTRIM(department_id), ''), NULLIF(BTRIM(department_name_snapshot), '')) IS NOT NULL
            THEN 'DEPARTMENT'
        ELSE 'COMMON'
    END
WHERE visibility_scope IS NULL
   OR BTRIM(visibility_scope) = '';

CREATE INDEX IF NOT EXISTS idx_flight_business_cases_visibility_department
    ON flight_business_cases (visibility_scope, department_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_archived_business_cases_visibility_department
    ON archived_flight_business_cases (visibility_scope, department_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_business_case_types_visibility_department
    ON business_case_types (visibility_scope, department_id, is_active, created_at);


