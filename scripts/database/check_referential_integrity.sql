-- ============================================================================
-- 参考完整性巡检脚本（外键移除后的应用层兜底巡检）
--
-- 背景：迁移 120 移除了全部外键约束，引用完整性改由应用层逻辑保证
-- （见 docs/plans/2026-08-12-remove-foreign-keys-spec.md §3.3）。
-- 审计要求软删除（迁移 121），因此理论上：
--   1. 不应出现孤儿行（子行指向不存在的父行）；
--   2. 不应出现"软删引用"（父行软删之后仍新建指向它的子行）。
--
-- 用法：psql -v ON_ERROR_STOP=1 -d <db> -f scripts/database/check_referential_integrity.sql
-- 结果：违规明细输出到 ri_violations；存在违规则 RAISE EXCEPTION（非零退出）。
-- 纳入 nightly 巡检（.github/workflows/nightly.yml）。
-- ============================================================================

CREATE TEMP TABLE IF NOT EXISTS ri_violations (
    check_name TEXT NOT NULL,
    detail     TEXT NOT NULL
);

-- ----------------------------------------------------------------------------
-- 第一类：孤儿行（子行外键列非空，但父表中完全不存在对应行）
-- ----------------------------------------------------------------------------

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.dispatch_orders.flight_id', o.id
FROM dispatch_orders o
LEFT JOIN flights f ON f.flight_id = o.flight_id
WHERE o.flight_id IS NOT NULL AND f.flight_id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.dispatch_orders.individual_user_id', o.id
FROM dispatch_orders o
LEFT JOIN users u ON u.id = o.individual_user_id
WHERE o.individual_user_id IS NOT NULL AND u.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.dispatch_order_members.source_team_id', m.id
FROM dispatch_order_members m
LEFT JOIN teams t ON t.id = m.source_team_id
WHERE m.source_team_id IS NOT NULL AND t.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.dispatch_orders.driver_user_id', o.id
FROM dispatch_orders o
LEFT JOIN users u ON u.id = o.driver_user_id
WHERE o.driver_user_id IS NOT NULL AND u.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.flight_legs.flight_id', l.leg_id
FROM flight_legs l
LEFT JOIN flights f ON f.flight_id = l.flight_id
WHERE f.flight_id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.flight_business_cases.flight_id', b.case_id
FROM flight_business_cases b
LEFT JOIN flights f ON f.flight_id = b.flight_id
WHERE f.flight_id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.anomalies.flight_id', a.anomaly_id
FROM anomalies a
LEFT JOIN flights f ON f.flight_id = a.flight_id
WHERE f.flight_id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.user_roles.user_id', ur.user_id || '->' || ur.role_id
FROM user_roles ur
LEFT JOIN users u ON u.id = ur.user_id
WHERE u.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.user_roles.role_id', ur.user_id || '->' || ur.role_id
FROM user_roles ur
LEFT JOIN roles r ON r.id = ur.role_id
WHERE r.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.role_permissions.role_id', rp.role_id || '->' || rp.permission_id
FROM role_permissions rp
LEFT JOIN roles r ON r.id = rp.role_id
WHERE r.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.role_permissions.permission_id', rp.role_id || '->' || rp.permission_id
FROM role_permissions rp
LEFT JOIN permissions p ON p.id = rp.permission_id
WHERE p.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.ai_entity_mcp_bindings.entity_id', b.binding_id
FROM ai_entity_mcp_bindings b
LEFT JOIN ai_entities e ON e.id = b.entity_id
WHERE e.id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.ai_entity_mcp_bindings.server_id', b.binding_id
FROM ai_entity_mcp_bindings b
LEFT JOIN ai_mcp_servers s ON s.server_id = b.server_id
WHERE s.server_id IS NULL;

INSERT INTO ri_violations (check_name, detail)
SELECT 'orphan.ai_entity_skill_bindings.entity_id', b.binding_id
FROM ai_entity_skill_bindings b
LEFT JOIN ai_entities e ON e.id = b.entity_id
WHERE e.id IS NULL;

-- ----------------------------------------------------------------------------
-- 第二类：软删引用（父行软删之后新建的子行，created_at > 父行 deleted_at）
-- 说明：父行软删之前已存在的子行属于合法历史数据，不算违规。
-- ----------------------------------------------------------------------------

INSERT INTO ri_violations (check_name, detail)
SELECT 'soft-deleted-ref.dispatch_orders.flight_id', o.id
FROM dispatch_orders o
JOIN flights f ON f.flight_id = o.flight_id
WHERE f.deleted_at IS NOT NULL AND o.created_at > f.deleted_at;

INSERT INTO ri_violations (check_name, detail)
SELECT 'soft-deleted-ref.anomalies.flight_id', a.anomaly_id
FROM anomalies a
JOIN flights f ON f.flight_id = a.flight_id
WHERE f.deleted_at IS NOT NULL AND a.created_at > f.deleted_at;

INSERT INTO ri_violations (check_name, detail)
SELECT 'soft-deleted-ref.flight_business_cases.flight_id', b.case_id
FROM flight_business_cases b
JOIN flights f ON f.flight_id = b.flight_id
WHERE f.deleted_at IS NOT NULL AND b.created_at > f.deleted_at;

INSERT INTO ri_violations (check_name, detail)
SELECT 'soft-deleted-ref.flight_legs.flight_id', l.leg_id
FROM flight_legs l
JOIN flights f ON f.flight_id = l.flight_id
WHERE f.deleted_at IS NOT NULL AND l.created_at > f.deleted_at;

-- ----------------------------------------------------------------------------
-- 汇总输出与失败判定
-- ----------------------------------------------------------------------------

SELECT check_name, count(*) AS violation_count
FROM ri_violations
GROUP BY check_name
ORDER BY check_name;

DO $$
DECLARE
    total INT;
BEGIN
    SELECT count(*) INTO total FROM ri_violations;
    IF total > 0 THEN
        RAISE EXCEPTION '参考完整性巡检失败：发现 % 条违规（明细见上方 ri_violations 输出）', total;
    END IF;
    RAISE NOTICE '参考完整性巡检通过：无孤儿行、无软删引用违规';
END
$$;
