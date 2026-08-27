-- PR #本体两层改造 PR2 - teams/equipment 挂 department_id，terminal 列废弃
-- As specified in docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md
-- （《组织：科室/班组/设备/人员》+ 迁移章节第 6 条）
--
-- 要点：
-- 1. teams.department_id：班组直接挂科室（不再经 team_types 间接归属）。
-- 2. 从旧 team_types.department_id 回填（team_types 主键为 id）。
-- 3. 回填后仍为空的历史班组（无 team_type）不强行 NOT NULL：
--    department_id 保持可空，由写路径（创建必填 + 科室边界校验）兜底。
-- 4. equipment.department_id：设备同样必挂科室（计划 :180/:197）。
--    历史设备无回填来源（equipment_types 无 department_id），保持 NULL，
--    创建必填，存量行的科室边界校验仅 admin 可改。
-- 5. teams.terminal / equipment.terminal：废弃但保留列不 DROP。
--    派工生成链路（find_available_for_dispatch 等 SQL 过滤）仍引用该列，
--    引用面大，故保留列供历史查询，写路径自本迁移起停止写入。
-- 6. teams.team_type_id：保留列供读取兼容（TeamResponse 仍带出历史值、
--    派工匹配链路 PR5 前仍按 team_type 过滤），写路径自本迁移起停止写入。
--
-- 无 FK：引用完整性在应用层保证（guard: tests/tools/test_no_new_foreign_keys.py，
-- 120 之后的新迁移禁止新增 FOREIGN KEY / REFERENCES）。

SET TRANSACTION READ WRITE;

-- ============================================================================
-- 1. teams.department_id
-- ============================================================================

ALTER TABLE teams ADD COLUMN IF NOT EXISTS department_id VARCHAR(26);

CREATE INDEX IF NOT EXISTS idx_teams_department ON teams(department_id);

-- 从旧 team_types.department_id 回填（team_types 主键为 id，不是 team_type_id）
UPDATE teams t
SET department_id = tt.department_id
FROM team_types tt
WHERE t.department_id IS NULL
  AND t.team_type_id IS NOT NULL
  AND tt.id = t.team_type_id;

COMMENT ON COLUMN teams.department_id IS
    '所属科室 id（departments.id，应用层引用完整性）；创建必填，历史无类型班组可为空';

-- ============================================================================
-- 2. equipment.department_id
-- ============================================================================

ALTER TABLE equipment ADD COLUMN IF NOT EXISTS department_id VARCHAR(26);

CREATE INDEX IF NOT EXISTS idx_equipment_department ON equipment(department_id);

COMMENT ON COLUMN equipment.department_id IS
    '所属科室 id（departments.id，应用层引用完整性）；创建必填，历史设备可为空（无回填来源）';

-- ============================================================================
-- 3. 废弃列标记（保留列、停止写入，见文件头说明）
-- ============================================================================

COMMENT ON COLUMN teams.terminal IS
    '已废弃（PR2）：班组无常驻楼字段。保留列仅供历史查询/派工旧链路过滤，写路径已停止写入';
COMMENT ON COLUMN teams.team_type_id IS
    '已废弃写路径（PR2）：班组类型降为只读历史。保留列供读取兼容，写路径已停止写入';
COMMENT ON COLUMN equipment.terminal IS
    '已废弃（PR2）：设备无常驻楼字段。保留列仅供历史查询/派工旧链路过滤，写路径已停止写入';

-- ============================================================================
-- Rollback notes
-- ============================================================================

/*
ALTER TABLE teams DROP COLUMN IF EXISTS department_id;
ALTER TABLE equipment DROP COLUMN IF EXISTS department_id;
-- terminal / team_type_id 列本身未删除，无需回滚；回填数据不可逆，需要时从
-- team_types.department_id 重新推导。
*/
