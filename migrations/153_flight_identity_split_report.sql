-- F3 preflight: produce a deterministic migration report before splitting
-- aggregate flights into directional flights. This migration intentionally
-- does not mutate flight identity; the apply step must consume the report in
-- an explicit deployment window after all reference remaps are verified.
CREATE TABLE IF NOT EXISTS flight_identity_split_report (
    old_flight_id VARCHAR(26) PRIMARY KEY,
    leg_count INTEGER NOT NULL,
    inbound_leg_id VARCHAR(26),
    outbound_leg_id VARCHAR(26),
    action VARCHAR(32) NOT NULL,
    error_reason TEXT,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

TRUNCATE TABLE flight_identity_split_report;

INSERT INTO flight_identity_split_report (
    old_flight_id, leg_count, inbound_leg_id, outbound_leg_id, action, error_reason
)
SELECT
    f.flight_id,
    COUNT(fl.leg_id)::INTEGER,
    MAX(fl.leg_id) FILTER (WHERE fl.leg_type = 'inbound'),
    MAX(fl.leg_id) FILTER (WHERE fl.leg_type = 'outbound'),
    CASE COUNT(fl.leg_id)
        WHEN 0 THEN 'fail'
        WHEN 1 THEN 'retain_single'
        WHEN 2 THEN 'split_turnaround'
        ELSE 'fail'
    END,
    CASE
        WHEN COUNT(fl.leg_id) = 0 THEN 'flight has no active flight_leg; refusing silent deletion'
        WHEN COUNT(fl.leg_id) > 2 THEN 'flight has more than two active flight_legs'
        WHEN COUNT(*) FILTER (WHERE fl.leg_type = 'inbound') > 1 THEN 'duplicate inbound legs'
        WHEN COUNT(*) FILTER (WHERE fl.leg_type = 'outbound') > 1 THEN 'duplicate outbound legs'
        ELSE NULL
    END
FROM flights f
LEFT JOIN flight_legs fl
    ON fl.flight_id = f.flight_id
   AND fl.deleted_at IS NULL
WHERE f.deleted_at IS NULL
GROUP BY f.flight_id;

DO $$
DECLARE
    invalid_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO invalid_count
    FROM flight_identity_split_report
    WHERE action = 'fail';
    IF invalid_count > 0 THEN
        RAISE EXCEPTION
            'F3 flight identity preflight failed: % invalid flight rows; inspect flight_identity_split_report',
            invalid_count;
    END IF;
END $$;

COMMENT ON TABLE flight_identity_split_report IS
    'F3 航班身份拆分预检报告；任何 0 leg/重复 leg/超过两 leg 必须先修复，禁止静默删除';
