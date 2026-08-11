
DROP INDEX IF EXISTS uq_dispatch_collab_events_source_record;

CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_source_record
    ON dispatch_collaboration_events(source_table, source_record_id)
    WHERE source_table IS NOT NULL AND source_record_id IS NOT NULL;

