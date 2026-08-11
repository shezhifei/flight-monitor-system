WITH duplicate_client_actions AS (
    SELECT
        id,
        details,
        ROW_NUMBER() OVER (
            PARTITION BY dispatch_order_id, action, details->>'client_action_id'
            ORDER BY created_at, id
        ) AS duplicate_rank
    FROM dispatch_order_logs
    WHERE details ? 'client_action_id'
      AND NULLIF(details->>'client_action_id', '') IS NOT NULL
)
UPDATE dispatch_order_logs AS logs
SET details = jsonb_set(
        logs.details - 'client_action_id',
        '{duplicate_client_action_id}',
        to_jsonb(logs.details->>'client_action_id'),
        true
    )
FROM duplicate_client_actions AS duplicates
WHERE logs.id = duplicates.id
  AND duplicates.duplicate_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_order_logs_client_action
    ON dispatch_order_logs(dispatch_order_id, action, (details->>'client_action_id'))
    WHERE details ? 'client_action_id'
      AND NULLIF(details->>'client_action_id', '') IS NOT NULL;


