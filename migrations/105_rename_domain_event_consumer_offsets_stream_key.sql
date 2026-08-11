-- Rename domain_event_consumer_offsets.stream_key to topic
-- Aligns column naming with MQ topic semantics (ADR-0003 known gap fix).
-- PostgreSQL RENAME COLUMN preserves primary key and index constraints.

ALTER TABLE domain_event_consumer_offsets
    RENAME COLUMN stream_key TO topic;
