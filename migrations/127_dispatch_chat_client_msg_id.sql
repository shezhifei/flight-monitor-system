-- 127_dispatch_chat_client_msg_id.sql
-- Make chat sends idempotent: the client stamps each optimistic message with a
-- `client_msg_id`, and a retried POST (network hiccup, double tap, offline
-- replay) resolves to the message already stored instead of inserting a
-- duplicate row and fanning out a second SSE frame.
-- Style: idempotent (safe to re-run), NO foreign keys (per migration 120
-- policy which dropped all FKs and relies on application-level integrity).

BEGIN;

ALTER TABLE dispatch_chat_messages
    ADD COLUMN IF NOT EXISTS client_msg_id VARCHAR(64);

COMMENT ON COLUMN dispatch_chat_messages.client_msg_id IS
    'Client-generated idempotency key, unique per group. NULL for messages the '
    'server originated (system/dispatch notices) and for rows written before 127.';

-- Partial unique index: pre-127 rows and server-originated messages keep
-- client_msg_id NULL and are exempt, so this can be added without a backfill.
CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_chat_messages_group_client_msg
    ON dispatch_chat_messages (group_id, client_msg_id)
    WHERE client_msg_id IS NOT NULL;

COMMIT;
