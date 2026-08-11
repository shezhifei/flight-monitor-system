ALTER TABLE flight_sync_runs
    ADD COLUMN IF NOT EXISTS official_record_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS registration_enriched_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS registration_ambiguous_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS registration_missing_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS stitched_turnaround_count INTEGER NOT NULL DEFAULT 0;
