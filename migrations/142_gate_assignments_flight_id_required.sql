-- PR #本体两层改造 PR3 - gate_assignments.flight_id 收敛必填
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md
-- （口分配主体改为 flight_id 必填 + 迁移章节第 5 条）
--
-- 要点：
-- 1. 回填（迁移章节第 5 条「active 行尽量回填」）：
--    flight_id 为空的历史行，按「同机号 + 时段重叠」匹配 flights 表回填：
--      - 机号相等（flights.registration = gate_assignments.registration）
--      - 时段命中：有效起飞时刻（actual > estimated > scheduled 取首个非空）
--        落在占用窗口 [starts_at, ends_at] 内，或有效到达时刻落在窗口内
--        （登机口占用对应航班在站时段，用航班到达/起飞与窗口做包含判断，
--        而非航班 [到,离] 区间与窗口重叠——出港航班的到达时刻在目的站，
--        与本站口占用窗口天然不重叠）。
--      - 多个候选取「有效时刻距 starts_at 最近」的航班（DISTINCT ON 保一行）。
--    回填不限于 active 行：released/expired 历史行同样尝试回填，
--    使下面的 NOT NULL 收敛覆盖全表。
-- 2. 补不上的行（计划 :573「补不上的标 released」）：
--    仍为空的 active 行置 status='released'。ends_at 保持原值不改：
--    若强行补 NOW()，对未来时段窗口会违反 chk_gate_assignment_time
--    (ends_at > starts_at)；且 released 行已被所有 active 查询排除，
--    窗口不再参与生效判断。
-- 3. NOT NULL 收敛：
--    全表 flight_id 无空行时执行 ALTER COLUMN ... SET NOT NULL；
--    若仍有补不上的行（其 flight_id 为空），保留可空并 RAISE WARNING，
--    由应用层 gate（AllocateGateRequest.flight_id 必填 + service 校验）兜底，
--    待历史数据清理后再补收敛迁移。
-- 4. 无新增外键：120 之后的新迁移禁止新增 FOREIGN KEY / REFERENCES
--    （guard: tests/tools/test_no_new_foreign_keys.py），flight_id 引用
--    完整性由应用层保证。

SET TRANSACTION READ WRITE;

-- ============================================================================
-- 1. 回填：同机号 + 时段重叠匹配 flights
-- ============================================================================

UPDATE gate_assignments ga
SET flight_id = m.flight_id,
    updated_at = NOW()
FROM (
    SELECT DISTINCT ON (ga2.id)
        ga2.id AS assignment_id,
        f.flight_id AS flight_id
    FROM gate_assignments ga2
    JOIN flights f ON f.registration = ga2.registration
    CROSS JOIN LATERAL (
        SELECT
            COALESCE(f.actual_arrival, f.estimated_arrival, f.scheduled_arrival) AS arr_at,
            COALESCE(f.actual_departure, f.estimated_departure, f.scheduled_departure) AS dep_at
    ) t
    WHERE ga2.flight_id IS NULL
      AND (
        (t.dep_at IS NOT NULL AND t.dep_at >= ga2.starts_at AND t.dep_at <= ga2.ends_at)
        OR (t.arr_at IS NOT NULL AND t.arr_at >= ga2.starts_at AND t.arr_at <= ga2.ends_at)
      )
    ORDER BY ga2.id,
        ABS(EXTRACT(EPOCH FROM (COALESCE(t.dep_at, t.arr_at) - ga2.starts_at))) ASC
) m
WHERE ga.id = m.assignment_id;

-- ============================================================================
-- 2. 补不上的 active 行：标 released（ends_at 保持原值，见文件头说明）
-- ============================================================================

UPDATE gate_assignments
SET status = 'released',
    updated_at = NOW()
WHERE flight_id IS NULL
  AND status = 'active';

-- ============================================================================
-- 3. NOT NULL 收敛（全表清干净才执行；否则保留可空并告警）
-- ============================================================================

DO $$
DECLARE
    remaining INTEGER;
BEGIN
    SELECT COUNT(*) INTO remaining FROM gate_assignments WHERE flight_id IS NULL;
    IF remaining = 0 THEN
        ALTER TABLE gate_assignments ALTER COLUMN flight_id SET NOT NULL;
    ELSE
        RAISE WARNING 'gate_assignments: % 行 flight_id 仍为空（已置 released），本次保留列可空；清理后需补收敛迁移', remaining;
    END IF;
END $$;

COMMENT ON COLUMN gate_assignments.flight_id IS
    '主体：航班（PR3 起必填）。registration 为机号投影；历史补不上的行已置 released';
