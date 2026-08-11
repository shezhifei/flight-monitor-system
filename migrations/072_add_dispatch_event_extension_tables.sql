
-- 工单内容调整规则配置表
CREATE TABLE IF NOT EXISTS dispatch_order_adjustment_rules (
    id VARCHAR(26) PRIMARY KEY,
    adjuster_type VARCHAR(64) NOT NULL,
    name VARCHAR(120) NOT NULL DEFAULT '',
    description TEXT,
    event_patterns JSONB NOT NULL DEFAULT '[]'::jsonb,
    priority INT NOT NULL DEFAULT 100,
    config JSONB,
    conditions JSONB,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    department_id VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(100),
    
    CONSTRAINT fk_adjustment_rules_department 
        FOREIGN KEY (department_id) REFERENCES departments(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_adjustment_rules_type ON dispatch_order_adjustment_rules(adjuster_type);
CREATE INDEX IF NOT EXISTS idx_adjustment_rules_enabled ON dispatch_order_adjustment_rules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_adjustment_rules_department ON dispatch_order_adjustment_rules(department_id);

-- 事件驱动的工单生成规则表
CREATE TABLE IF NOT EXISTS event_driven_dispatch_generation_rules (
    id VARCHAR(26) PRIMARY KEY,
    generator_type VARCHAR(64) NOT NULL,
    name VARCHAR(120) NOT NULL DEFAULT '',
    description TEXT,
    event_patterns JSONB NOT NULL DEFAULT '[]'::jsonb,
    priority INT NOT NULL DEFAULT 100,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    conditions JSONB,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    department_id VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(100),
    
    CONSTRAINT fk_generation_rules_department 
        FOREIGN KEY (department_id) REFERENCES departments(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_generation_rules_type ON event_driven_dispatch_generation_rules(generator_type);
CREATE INDEX IF NOT EXISTS idx_generation_rules_enabled ON event_driven_dispatch_generation_rules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_generation_rules_department ON event_driven_dispatch_generation_rules(department_id);

-- 工单事件调整日志表
CREATE TABLE IF NOT EXISTS dispatch_order_adjustment_logs (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) NOT NULL,
    adjuster_id VARCHAR(64) NOT NULL,
    adjuster_type VARCHAR(64) NOT NULL,
    event_id VARCHAR(64),
    event_type VARCHAR(64) NOT NULL,
    action_type VARCHAR(32) NOT NULL,
    before_state JSONB,
    after_state JSONB,
    reason TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    CONSTRAINT fk_adjustment_logs_order 
        FOREIGN KEY (dispatch_order_id) REFERENCES dispatch_orders(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_adjustment_logs_order ON dispatch_order_adjustment_logs(dispatch_order_id);
CREATE INDEX IF NOT EXISTS idx_adjustment_logs_adjuster ON dispatch_order_adjustment_logs(adjuster_id);
CREATE INDEX IF NOT EXISTS idx_adjustment_logs_event ON dispatch_order_adjustment_logs(event_id);

-- 添加 source_type 新值支持（如果列已存在）
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'dispatch_orders' AND column_name = 'source_type'
    ) THEN
        -- 检查是否需要添加 EventGenerated 值
        -- 注意: PostgreSQL 的 ALTER TYPE ADD VALUE 需要在事务中执行
        -- 这里使用 CHECK 约束或默认值处理
        NULL;
    END IF;
END $$;

