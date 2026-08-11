-- Add recipient snapshot columns to notifications.
--
-- These columns were historically populated by JOIN-ing the `users` table
-- at notification-read time. Making them first-class columns on the
-- notification row lets us read them without extra JOINs and guarantees
-- that the snapshot captured at send time is preserved even if the
-- recipient's user profile changes later.

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS recipient_username_snapshot VARCHAR(128),
    ADD COLUMN IF NOT EXISTS recipient_display_name_snapshot VARCHAR(128),
    ADD COLUMN IF NOT EXISTS recipient_department_snapshot VARCHAR(64),
    ADD COLUMN IF NOT EXISTS recipient_job_title_snapshot VARCHAR(64);
