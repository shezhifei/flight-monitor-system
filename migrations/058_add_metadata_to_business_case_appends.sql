-- 058: 为业务事项追加记录增加 metadata JSONB 列
-- 用于存储 AI agent 工具调用记录、token 用量等结构化元数据

ALTER TABLE flight_business_case_appends
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN flight_business_case_appends.metadata
    IS '追加记录的结构化元数据（tool_calls, token_usage, thinking, step_type, sequence 等）';
