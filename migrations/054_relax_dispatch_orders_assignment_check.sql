
ALTER TABLE dispatch_orders
    DROP CONSTRAINT IF EXISTS dispatch_orders_check;

ALTER TABLE dispatch_orders
    ADD CONSTRAINT dispatch_orders_check CHECK (
        (
            assignee_type = 'team'
            AND individual_user_id IS NULL
            AND (
                team_id IS NOT NULL
                OR status = 'pending'
                OR COALESCE(
                    jsonb_array_length(COALESCE(task_crew->'members', '[]'::jsonb)),
                    0
                ) > 0
            )
        )
        OR (
            assignee_type = 'individual'
            AND team_id IS NULL
            AND (
                individual_user_id IS NOT NULL
                OR status = 'pending'
            )
        )
    );

