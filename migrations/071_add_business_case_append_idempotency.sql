ALTER TABLE flight_business_case_appends
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

WITH duplicate_client_actions AS (
    SELECT
        append_id,
        ROW_NUMBER() OVER (
            PARTITION BY case_id, client_action_id
            ORDER BY appended_at, append_id
        ) AS duplicate_rank
    FROM flight_business_case_appends
    WHERE client_action_id IS NOT NULL
      AND NULLIF(client_action_id, '') IS NOT NULL
)
UPDATE flight_business_case_appends AS appends
SET metadata = jsonb_set(
        appends.metadata,
        '{duplicate_client_action_id}',
        to_jsonb(appends.client_action_id),
        true
    ),
    client_action_id = NULL
FROM duplicate_client_actions AS duplicates
WHERE appends.append_id = duplicates.append_id
  AND duplicates.duplicate_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_fbc_appends_case_client_action
    ON flight_business_case_appends(case_id, client_action_id)
    WHERE client_action_id IS NOT NULL;


