-- F1: monitor rows are a persisted read model.  Merging and breaking a
-- turnaround must not physically delete a row because row_id is a stable UI
-- key and the project forbids destructive deletes in application SQL.
ALTER TABLE flight_monitor_rows
    ADD COLUMN IF NOT EXISTS is_active BOOLEAN NOT NULL DEFAULT TRUE;

CREATE INDEX IF NOT EXISTS idx_flight_monitor_rows_active_workspace_sort
    ON flight_monitor_rows (is_active, workspace_date, sort_time DESC, row_id);
