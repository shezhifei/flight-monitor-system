-- Create domain event outbox table for reliable event relay


CREATE TABLE IF NOT EXISTS domain_event_outbox (
    event_id VARCHAR(26) PRIMARY KEY,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(26) NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at TIMESTAMPTZ,
    publish_attempts INTEGER NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error TEXT,
    source_change_id VARCHAR(26) NOT NULL,
    CONSTRAINT uq_domain_event_outbox_source_change UNIQUE (source_change_id)
);

CREATE INDEX IF NOT EXISTS idx_domain_event_outbox_pending
    ON domain_event_outbox(next_retry_at, occurred_at)
    WHERE published_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_domain_event_outbox_aggregate
    ON domain_event_outbox(aggregate_type, aggregate_id, occurred_at DESC);

