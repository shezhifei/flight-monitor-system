-- 120: Drop ALL foreign key constraints in the public schema.
--
-- Background: referential integrity is now enforced at the application layer
-- (see docs/plans/2026-08-12-remove-foreign-keys-spec.md). Audit requirements
-- forbid physical deletion of business data, so ON DELETE CASCADE/SET NULL/
-- RESTRICT semantics are no longer needed at the database level.
--
-- This migration dynamically enumerates pg_constraint (contype = 'f') instead
-- of listing 136 constraint names, so it converges to "no foreign keys"
-- regardless of how the schema was created (migrations vs. manual setup).
-- Idempotent: safe to re-run.

DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT conname, conrelid::regclass::text AS tbl
        FROM pg_constraint
        WHERE contype = 'f'
          AND connamespace = 'public'::regnamespace
    LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', r.tbl, r.conname);
    END LOOP;
END
$$;

-- Self-check: no foreign key may remain after this migration.
DO $$
DECLARE
    remaining INT;
BEGIN
    SELECT count(*) INTO remaining
    FROM pg_constraint
    WHERE contype = 'f'
      AND connamespace = 'public'::regnamespace;

    IF remaining > 0 THEN
        RAISE EXCEPTION 'migration 120 failed: % foreign key constraint(s) still present', remaining;
    END IF;
END
$$;
