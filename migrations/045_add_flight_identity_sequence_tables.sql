CREATE TABLE IF NOT EXISTS flight_identity_bindings (
    identity_binding_id VARCHAR(26) PRIMARY KEY,
    vendor VARCHAR(64) NOT NULL,
    vendor_movement_id VARCHAR(128) NOT NULL,
    registration VARCHAR(32),
    official_natural_key VARCHAR(255) NOT NULL,
    flight_id VARCHAR(26) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_flight_identity_bindings_vendor_movement UNIQUE (vendor, vendor_movement_id)
);

CREATE INDEX IF NOT EXISTS idx_flight_identity_bindings_flight_id
    ON flight_identity_bindings(flight_id);

CREATE INDEX IF NOT EXISTS idx_flight_identity_bindings_registration
    ON flight_identity_bindings(registration, last_seen_at DESC)
    WHERE registration IS NOT NULL;

CREATE TABLE IF NOT EXISTS flight_aircraft_sequences (
    sequence_binding_id VARCHAR(26) PRIMARY KEY,
    sequence_key VARCHAR(512) NOT NULL UNIQUE,
    registration VARCHAR(32) NOT NULL,
    flight_id VARCHAR(26) NOT NULL,
    inbound_natural_key VARCHAR(255),
    outbound_natural_key VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flight_aircraft_sequences_registration
    ON flight_aircraft_sequences(registration, last_seen_at DESC);
