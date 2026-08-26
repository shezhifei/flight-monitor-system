-- PR7 岗位/个人账号分离：交接班单绑定岗位账号
-- docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md §交接班
--
-- 交接班从「from/to 任意用户」改为：席位专属，`from`/`to` 必须是个人账号，
-- 且该单归属某个岗位（席）。create/submit：当前占席个人；complete：核接班人密码
-- 后调同一 OccupySeat。
--
-- 120 已删除所有 FK；本迁移只加列 + 索引，不加 FOREIGN KEY / REFERENCES
-- （guard：tests/tools/test_no_new_foreign_keys.py）。引用完整性由应用层保证。

SET TRANSACTION READ WRITE;

ALTER TABLE shift_handovers
    ADD COLUMN IF NOT EXISTS position_user_id VARCHAR(26);

CREATE INDEX IF NOT EXISTS idx_shift_handovers_position_status
    ON shift_handovers(position_user_id, status, shift_date DESC)
    WHERE position_user_id IS NOT NULL;

COMMENT ON COLUMN shift_handovers.position_user_id IS
    '该交接单所属岗位（席）账号 id；创建时由当前占席个人写入，complete 调 OccupySeat 核密后切占用';

-- ============================================================================
-- 回滚
-- ============================================================================
/*
ALTER TABLE shift_handovers DROP COLUMN IF EXISTS position_user_id;
*/