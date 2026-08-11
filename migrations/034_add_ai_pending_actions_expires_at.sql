-- Ensure ai_pending_actions supports TTL-based expiration.


ALTER TABLE ai_pending_actions
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_expires_at
    ON ai_pending_actions(expires_at)
    WHERE status = 'pending' AND expires_at IS NOT NULL;

