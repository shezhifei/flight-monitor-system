-- Migration 005: Create Audit Logs Table
-- Purpose: Store structured audit events for business entity changes

CREATE TABLE IF NOT EXISTS system_audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type VARCHAR(50) NOT NULL,
    entity_id VARCHAR(100) NOT NULL,
    action VARCHAR(50) NOT NULL,
    changes JSONB,
    user_id VARCHAR(100),
    trace_id VARCHAR(100),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_entity ON system_audit_logs(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_audit_time ON system_audit_logs(created_at);

COMMENT ON TABLE system_audit_logs IS '系统审计日志，记录实体变更历史';

-- Record migration

