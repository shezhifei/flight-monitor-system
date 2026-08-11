ALTER TABLE flight_dispatch_timeline_events
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

CREATE UNIQUE INDEX IF NOT EXISTS uq_flight_dispatch_timeline_client_action
    ON flight_dispatch_timeline_events(flight_id, client_action_id)
    WHERE client_action_id IS NOT NULL;


