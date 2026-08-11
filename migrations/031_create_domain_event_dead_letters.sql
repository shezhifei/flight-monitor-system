-- Create dead-letter table for domain event subscriber failures.


CREATE TABLE IF NOT EXISTS domain_event_dead_letters (
    event_id VARCHAR(64) PRIMARY KEY,
    source_change_id VARCHAR(64),
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(64) NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    stream_message_id VARCHAR(64),
    retry_count INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    dead_lettered_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_domain_event_dead_letters_type_time
    ON domain_event_dead_letters(event_type, dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_domain_event_dead_letters_aggregate
    ON domain_event_dead_letters(aggregate_type, aggregate_id, dead_lettered_at DESC);

