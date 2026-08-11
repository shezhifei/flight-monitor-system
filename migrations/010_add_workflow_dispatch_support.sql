-- 为派工系统增加流程编排支持


ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS process_instance_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS process_task_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS workflow_context JSONB DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS workflow_status VARCHAR(20) DEFAULT 'pending_assignment',
    ADD COLUMN IF NOT EXISTS source VARCHAR(20) DEFAULT 'system',
    ADD COLUMN IF NOT EXISTS recommended_assignees JSONB,
    ADD COLUMN IF NOT EXISTS recommendation_score NUMERIC(5, 2),
    ADD COLUMN IF NOT EXISTS supervisor_notified BOOLEAN DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS supervisor_notified_at TIMESTAMP WITH TIME ZONE,
    ADD COLUMN IF NOT EXISTS assignment_deadline TIMESTAMP WITH TIME ZONE;

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_process_instance
    ON dispatch_orders(process_instance_id);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_source
    ON dispatch_orders(source);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_workflow_status
    ON dispatch_orders(workflow_status);

CREATE TABLE IF NOT EXISTS workflow_dispatch_mappings (
    mapping_id VARCHAR(26) PRIMARY KEY,
    process_instance_id VARCHAR(64) NOT NULL,
    process_definition_key VARCHAR(100) NOT NULL,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE SET NULL,
    business_key VARCHAR(100),
    flight_id VARCHAR(26),
    context_variables JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_workflow_dispatch_mapping_process
    ON workflow_dispatch_mappings(process_instance_id);

CREATE INDEX IF NOT EXISTS idx_workflow_dispatch_mapping_dispatch
    ON workflow_dispatch_mappings(dispatch_order_id);

CREATE INDEX IF NOT EXISTS idx_workflow_dispatch_mapping_flight
    ON workflow_dispatch_mappings(flight_id);

