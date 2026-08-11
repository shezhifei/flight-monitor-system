-- workflow_receipt 已迁移到 FlightBusinessCase.workflow_receipt 字段，
-- 不再存在 context JSONB 中
UPDATE flight_business_cases
SET context = context - 'workflow_receipt'
WHERE context ? 'workflow_receipt';
