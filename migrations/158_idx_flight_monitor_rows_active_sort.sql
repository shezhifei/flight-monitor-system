-- no-transaction
-- Hot-path list is `WHERE is_active ORDER BY sort_time DESC NULLS LAST, row_id`.
-- The composite (is_active, workspace_date, sort_time, row_id) index is weaker
-- when workspace_date is unconstrained.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_flight_monitor_rows_active_sort
    ON flight_monitor_rows (sort_time DESC, row_id)
    WHERE is_active;
