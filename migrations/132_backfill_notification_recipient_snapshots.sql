-- Backfill recipient snapshot columns on existing notifications.
--
-- Migration 100 added the snapshot columns with the intent that the
-- recipient's identity is captured at send time, but the insert path never
-- populated them (always NULL), so receipt projections fell back to the
-- '未知账号' sentinel. The insert path now writes the snapshot; this
-- migration repairs rows created before that fix. Rows whose recipient no
-- longer exists in `users` keep a NULL snapshot (read paths fall back to
-- the sentinel, which is the correct historical answer for a deleted
-- account).

UPDATE notifications n
SET recipient_username_snapshot = u.username,
    recipient_display_name_snapshot = u.display_name,
    recipient_department_snapshot = u.department,
    recipient_job_title_snapshot = u.job_title
FROM users u
WHERE u.id = n.user_id
  AND n.recipient_username_snapshot IS NULL;
