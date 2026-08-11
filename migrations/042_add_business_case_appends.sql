ALTER TABLE flight_business_cases
    ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    ADD COLUMN IF NOT EXISTS stand VARCHAR(10),
    ADD COLUMN IF NOT EXISTS gate VARCHAR(10),
    ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS log TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[];

ALTER TABLE archived_flight_business_cases
    ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    ADD COLUMN IF NOT EXISTS stand VARCHAR(10),
    ADD COLUMN IF NOT EXISTS gate VARCHAR(10),
    ADD COLUMN IF NOT EXISTS finished_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS log TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[];

CREATE TABLE IF NOT EXISTS flight_business_case_appends (
    id SERIAL PRIMARY KEY,
    append_id VARCHAR(26) NOT NULL UNIQUE,
    case_id VARCHAR(26) NOT NULL,
    content TEXT NOT NULL,
    submitted_by VARCHAR(100) NOT NULL,
    submitted_operator_name VARCHAR(100),
    appended_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_business_case_append_case
        FOREIGN KEY (case_id) REFERENCES flight_business_cases(case_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_fbc_appends_case_id_time
    ON flight_business_case_appends(case_id, appended_at ASC);


