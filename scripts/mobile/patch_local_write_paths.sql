-- Local-only schema patches that unblock mobile write-path acceptance.
-- These are NOT numbered migrations and must not ship as a backend change
-- on the Android rebuild branch (plan §7: 后端零改动).
--
-- 1) safety checklist records: backend writes Uuid v4 (36) into varchar(26)
-- 2) notifications.updated_at: repository ON CONFLICT SET updated_at = NOW()
--    but the live table never had that column.

ALTER TABLE dispatch_safety_checklist_records
    ALTER COLUMN record_id TYPE varchar(36);

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
