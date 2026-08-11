-- Create domain event consumer idempotency and offset tables


CREATE TABLE IF NOT EXISTS domain_event_processed (
    event_id VARCHAR(64) PRIMARY KEY,
    source_change_id VARCHAR(64),
    event_type VARCHAR(128) NOT NULL,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(64) NOT NULL,
    success BOOLEAN NOT NULL DEFAULT FALSE,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_domain_event_processed_success_attempt
    ON domain_event_processed(success, last_attempt_at DESC);

CREATE INDEX IF NOT EXISTS idx_domain_event_processed_source_change
    ON domain_event_processed(source_change_id);

CREATE TABLE IF NOT EXISTS domain_event_consumer_offsets (
    consumer_group VARCHAR(128) NOT NULL,
    consumer_name VARCHAR(128) NOT NULL,
    stream_key VARCHAR(128) NOT NULL,
    last_message_id VARCHAR(64) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (consumer_group, consumer_name, stream_key)
);

CREATE INDEX IF NOT EXISTS idx_domain_event_consumer_offsets_updated_at
    ON domain_event_consumer_offsets(updated_at DESC);

