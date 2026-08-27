-- 143: 派工去班组化（本体两层改造 PR5）
--
-- 语义：班组（Team）只是在岗名册，不再是工单指派对象；人员按槽挂到工单
-- （dispatch_order_members.slot_code），司机资质由设备槽的 requires_driver/资质表达。
--
-- 变更：
--   1. dispatch_orders 删除 assignee_type / team_id / driver_team_id 列
--      （含对 teams 的旧 FK，随列一并删除）。individual_user_id / driver_user_id 保留。
--      driver_type 列保留但只允许 'individual'/NULL。
--   2. equipment_types 删除 driver_team_type_id 列（FK 到 team_types 随列删除）；
--      requires_driver 布尔列自 007 起已存在，无需新增。
--   3. ai_query.v_dispatch_orders 视图去掉已删列（DROP+CREATE，CREATE OR REPLACE
--      不能减少视图列）。
--
-- 取舍：直接 DROP 而非「注释废弃+停写」——assignee_type 是 NOT NULL 无默认列，
-- 保留它反而要求 INSERT 继续写死值；DROP COLUMN 会级联删掉旧 FK，符合 120 后
-- 不新增 FK 的方向。历史数据中的 team_id 引用随之丢弃（名单语义已由
-- dispatch_order_members.source_team_id 承担）。

BEGIN;

DROP VIEW IF EXISTS ai_query.v_dispatch_orders;

ALTER TABLE dispatch_orders
    DROP COLUMN IF EXISTS assignee_type,
    DROP COLUMN IF EXISTS team_id,
    DROP COLUMN IF EXISTS driver_team_id;

ALTER TABLE equipment_types
    DROP COLUMN IF EXISTS driver_team_type_id;

CREATE VIEW ai_query.v_dispatch_orders AS
SELECT
    d.id AS dispatch_order_id,
    d.flight_id,
    d.task_type,
    d.stand_id,
    d.individual_user_id,
    d.status,
    d.dispatch_type,
    d.workflow_status,
    d.source,
    d.recommendation_score,
    d.planned_start_time,
    d.planned_end_time,
    d.actual_start_time,
    d.actual_end_time,
    d.assignment_deadline,
    d.dispatched_at,
    d.created_at,
    d.updated_at
FROM public.dispatch_orders AS d;

COMMIT;
