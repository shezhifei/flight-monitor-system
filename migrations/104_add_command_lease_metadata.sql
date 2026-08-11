-- Phase 4: Command Queue + Hardened Worker Leases + Multi-worker
-- Adds lease-hardening metadata to ai_runtime_commands so multiple
-- Python workers can compete safely via FOR UPDATE SKIP LOCKED while
-- preserving per-run ownership after a worker leases start_run.

ALTER TABLE ai_runtime_commands
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 3,
    ADD COLUMN IF NOT EXISTS last_heartbeat_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS run_owner_lock TEXT;

-- Speed up recovery scans for expired leases. Partial index only
-- covers rows that can actually expire (status = 'leased').
CREATE INDEX IF NOT EXISTS idx_ai_commands_lease_expiry
    ON ai_runtime_commands(lease_expires_at)
    WHERE status = 'leased';
