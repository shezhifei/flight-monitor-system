
ALTER TABLE IF EXISTS department_step_requirement_versions
    RENAME TO department_task_type_requirement_versions;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.table_constraints
        WHERE table_name = 'department_task_type_requirement_versions'
          AND constraint_name = 'uq_department_step_requirement_version'
    ) THEN
        ALTER TABLE department_task_type_requirement_versions
            RENAME CONSTRAINT uq_department_step_requirement_version
            TO uq_department_task_type_requirement_version;
    END IF;
END $$;

ALTER INDEX IF EXISTS idx_department_step_requirement_versions_lookup
    RENAME TO idx_department_task_type_requirement_versions_lookup;

ALTER INDEX IF EXISTS uq_department_step_requirement_published
    RENAME TO uq_department_task_type_requirement_published;

