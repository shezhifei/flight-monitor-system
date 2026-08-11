-- Runtime schema guard for host development startup.
--
-- setup_postgresql.sql bootstraps legacy base tables while sqlx owns numbered
-- migrations. This guard verifies the runtime contracts that are easy to miss
-- when an existing database has stale migration metadata.

DO $$
DECLARE
    missing_items TEXT[];
BEGIN
    WITH required_relations(kind, name) AS (
        VALUES
            ('table', 'business_case_types'),
            ('table', 'flight_business_cases'),
            ('table', 'ai_copilot_business_case_batches'),
            ('table', 'ai_action_proposals'),
            ('table', 'ai_entities'),
            ('table', 'notifications'),
            ('index', 'idx_business_case_types_ai_enabled'),
            ('index', 'idx_flight_business_cases_copilot_batch_action')
    ),
    required_columns(table_name, column_name) AS (
        VALUES
            ('business_case_types', 'ai_extraction_config'),
            ('business_case_types', 'case_properties'),
            ('ai_copilot_business_case_batches', 'batch_id'),
            ('ai_copilot_business_case_batches', 'notification_groups'),
            ('ai_copilot_business_case_batches', 'commit_error'),
            ('ai_copilot_business_case_batches', 'committed_at'),
            ('ai_copilot_business_case_batches', 'workflow_dispatch_status'),
            ('ai_copilot_business_case_batches', 'workflow_dispatch_request'),
            ('ai_copilot_business_case_batches', 'workflow_dispatch_error'),
            ('ai_copilot_business_case_batches', 'workflow_dispatch_attempts'),
            ('ai_copilot_business_case_batches', 'workflow_dispatched_at'),
            ('ai_copilot_business_case_batches', 'workflow_dispatch_next_retry_at'),
            ('ai_copilot_business_case_batches', 'commit_request'),
            ('ai_copilot_business_case_batches', 'created_action_case_ids'),
            ('ai_copilot_business_case_batches', 'commit_started_at'),
            ('ai_copilot_business_case_batches', 'commit_attempts'),
            ('ai_copilot_business_case_batches', 'commit_next_recovery_at'),
            ('ai_action_proposals', 'proposal_id'),
            ('ai_action_proposals', 'job_id'),
            ('ai_action_proposals', 'run_id'),
            ('ai_action_proposals', 'ontology_version'),
            ('ai_action_proposals', 'object_type'),
            ('ai_action_proposals', 'object_id'),
            ('ai_action_proposals', 'action_name'),
            ('ai_action_proposals', 'arguments'),
            ('ai_action_proposals', 'status'),
            ('ai_action_proposals', 'metadata'),
            ('ai_entities', 'config_version'),
            ('ai_entities', 'config_revision'),
            ('notifications', 'recipient_username_snapshot'),
            ('notifications', 'recipient_display_name_snapshot'),
            ('notifications', 'recipient_department_snapshot'),
            ('notifications', 'recipient_job_title_snapshot')
    ),
    missing_relations AS (
        SELECT format('%s:%s', kind, name) AS item
        FROM required_relations
        WHERE to_regclass(format('public.%I', name)) IS NULL
    ),
    missing_columns AS (
        SELECT format('column:%s.%s', rc.table_name, rc.column_name) AS item
        FROM required_columns rc
        LEFT JOIN information_schema.columns c
            ON c.table_schema = 'public'
           AND c.table_name = rc.table_name
           AND c.column_name = rc.column_name
        WHERE c.column_name IS NULL
    )
    SELECT array_agg(item ORDER BY item)
    INTO missing_items
    FROM (
        SELECT item FROM missing_relations
        UNION ALL
        SELECT item FROM missing_columns
    ) AS missing;

    IF COALESCE(array_length(missing_items, 1), 0) > 0 THEN
        RAISE EXCEPTION 'runtime schema verification failed; missing: %',
            array_to_string(missing_items, ', ');
    END IF;
END $$;
