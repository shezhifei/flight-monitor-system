-- Backfill anomaly -> todo links from event-sourced todo creation events.
WITH candidate_links AS (
    SELECT DISTINCT ON (change_data #>> '{data,data,source_id}')
        (change_data #>> '{data,data,source_id}') AS anomaly_id,
        todo_id
    FROM todo_state_changes
    WHERE change_type = 'todo_created'
      AND (change_data #>> '{data,data,source_type}') = 'anomaly_detection'
      AND COALESCE(change_data #>> '{data,data,source_id}', '') <> ''
    ORDER BY (change_data #>> '{data,data,source_id}'), occurred_at DESC
)
UPDATE anomalies AS a
SET linked_todo_id = c.todo_id,
    updated_at = NOW()
FROM candidate_links AS c
WHERE a.anomaly_id = c.anomaly_id
  AND COALESCE(a.linked_todo_id, '') = '';
