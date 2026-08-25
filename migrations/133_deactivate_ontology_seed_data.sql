-- Deactivate ontology seed data - PR #本体两层改造
-- This migration sets is_active = false for all aip_ontology_objects rows
-- After this migration, load_active_schema() will return None and fall back to code-based schema (build_flight_ops_v1_schema)
-- Rollback is simple: UPDATE aip_ontology_objects SET is_active = TRUE;

SET TRANSACTION READ WRITE;

-- Deactivate all seed objects in aip_ontology_objects
-- These were inserted by migrations/073 and setup_postgresql.sql
UPDATE aip_ontology_objects
SET is_active = FALSE,
    updated_at = CURRENT_TIMESTAMP
WHERE is_active = TRUE;

COMMENT ON TABLE aip_ontology_objects IS 'Ontology 对象类型定义 [已停用种子数据] - AI 治理不再使用此表作为 schema 真相源，改用代码 schema + overlay';
COMMENT ON TABLE aip_ontology_actions IS 'Ontology 动作定义 [已停用种子数据] - AI 治理不再使用此表作为 schema 真相源，改用代码 schema + overlay';
COMMENT ON TABLE aip_functions IS 'AIP 函数注册表 [已停用种子数据] - 不再由 seed data 驱动 schema';
COMMENT ON TABLE aip_tool_mappings IS '工具映射配置 [已停用种子数据] - 不再由 seed data 驱动 schema';
COMMENT ON TABLE aip_constraints IS '业务约束定义 [已停用种子数据] - 不再由 seed data 驱动 schema';

-- Note: This is NOT a destructive migration. 
-- The tables remain intact but are no longer used as schema source.
-- Overlay tables (aip_ontology_customization_* if added later) can still be used for risk/approval configuration.
-- Rollback: UPDATE aip_ontology_objects SET is_active = TRUE WHERE is_active = FALSE;
