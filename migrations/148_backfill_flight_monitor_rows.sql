-- Initial F1 projection from the current aggregate flights + legs model.
-- Subsequent write paths must upsert the row in the same UnitOfWork.
INSERT INTO flight_monitor_rows (
    row_id, kind, inbound_flight_id, outbound_flight_id,
    inbound_flight_no, outbound_flight_no,
    inbound_scheduled_at, outbound_scheduled_at,
    inbound_is_vip, outbound_is_vip, registration, aircraft_type,
    stand_code, gate_code, terminal_code, baggage_carousel_code,
    status, workspace_date, sort_time, version, updated_at
)
SELECT
    f.flight_id,
    CASE WHEN i.flight_id IS NOT NULL AND o.flight_id IS NOT NULL THEN 'turnaround' ELSE 'single' END,
    CASE WHEN i.flight_id IS NOT NULL THEN f.flight_id END,
    CASE WHEN o.flight_id IS NOT NULL THEN f.flight_id END,
    i.flight_no, o.flight_no, i.scheduled_time, o.scheduled_time,
    COALESCE(i.is_vip, FALSE), COALESCE(o.is_vip, FALSE), f.registration, f.aircraft_type_detail,
    f.stand, f.gate, f.terminal, f.baggage_carousel,
    f.status::text, f.workspace_date,
    COALESCE(i.scheduled_time, o.scheduled_time, f.scheduled_departure, f.scheduled_arrival),
    f.version::integer, f.updated_at
FROM flights f
LEFT JOIN LATERAL (
    SELECT fl.flight_id, fl.flight_no, fl.scheduled_time, fl.is_vip
    FROM flight_legs fl WHERE fl.flight_id = f.flight_id AND fl.leg_type = 'inbound' AND fl.deleted_at IS NULL LIMIT 1
) i ON TRUE
LEFT JOIN LATERAL (
    SELECT fl.flight_id, fl.flight_no, fl.scheduled_time, fl.is_vip
    FROM flight_legs fl WHERE fl.flight_id = f.flight_id AND fl.leg_type = 'outbound' AND fl.deleted_at IS NULL LIMIT 1
) o ON TRUE
WHERE f.deleted_at IS NULL
ON CONFLICT (row_id) DO NOTHING;
