-- linked_todo_id stores event-sourced Todo IDs; keep it as a soft reference.
DO $$
BEGIN
    IF to_regclass('public.anomalies') IS NULL THEN
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'fk_anomalies_todo_id'
          AND conrelid = 'anomalies'::regclass
    ) THEN
        ALTER TABLE anomalies DROP CONSTRAINT fk_anomalies_todo_id;
    END IF;
END
$$;
