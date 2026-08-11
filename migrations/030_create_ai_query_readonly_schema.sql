-- Create ai_query schema and readonly views for AI SQL read tool


ALTER TABLE public.flights
    ADD COLUMN IF NOT EXISTS inbound_abnormal BOOLEAN,
    ADD COLUMN IF NOT EXISTS outbound_abnormal BOOLEAN;

CREATE SCHEMA IF NOT EXISTS ai_query;

DROP VIEW IF EXISTS ai_query.v_ops_overview;
DROP VIEW IF EXISTS ai_query.v_daily_kpi;
DROP VIEW IF EXISTS ai_query.v_online_history;
DROP VIEW IF EXISTS ai_query.v_notifications;
DROP VIEW IF EXISTS ai_query.v_shift_handovers;
DROP VIEW IF EXISTS ai_query.v_dispatch_alerts;
DROP VIEW IF EXISTS ai_query.v_dispatch_orders;
DROP VIEW IF EXISTS ai_query.v_todos;
DROP VIEW IF EXISTS ai_query.v_anomalies;
DROP VIEW IF EXISTS ai_query.v_flights;

CREATE OR REPLACE VIEW ai_query.v_flights AS
SELECT
    f.flight_id,
    f.flight_number,
    f.airline_code,
    f.status,
    f.scheduled_departure,
    f.estimated_departure,
    f.actual_departure,
    f.scheduled_arrival,
    f.estimated_arrival,
    f.actual_arrival,
    f.execution_date,
    f.workspace_date,
    f.stand,
    f.gate,
    f.terminal,
    f.inbound_abnormal,
    f.outbound_abnormal,
    CASE
        WHEN f.estimated_departure IS NOT NULL AND f.scheduled_departure IS NOT NULL THEN
            ROUND(EXTRACT(EPOCH FROM (f.estimated_departure - f.scheduled_departure)) / 60.0, 2)
        ELSE NULL
    END AS delay_minutes,
    f.created_at,
    f.updated_at
FROM public.flights AS f;

CREATE OR REPLACE VIEW ai_query.v_anomalies AS
SELECT
    a.anomaly_id,
    a.flight_id,
    a.anomaly_type,
    a.severity,
    a.status,
    a.title,
    a.description,
    a.detected_at,
    a.resolved_at,
    a.escalation_level,
    a.rule_id,
    a.context_data,
    a.created_at,
    a.updated_at
FROM public.anomalies AS a;

CREATE OR REPLACE VIEW ai_query.v_todos AS
SELECT
    t.todo_id,
    t.title,
    t.description,
    t.priority,
    t.status,
    t.category,
    t.due_date,
    t.assigned_to,
    t.progress,
    t.tags,
    t.created_by,
    t.updated_by,
    t.created_at,
    t.updated_at,
    t.is_deleted
FROM public.todos AS t;

CREATE OR REPLACE VIEW ai_query.v_dispatch_orders AS
SELECT
    d.id AS dispatch_order_id,
    d.flight_id,
    d.task_type,
    d.stand_id,
    d.assignee_type,
    d.team_id,
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

CREATE OR REPLACE VIEW ai_query.v_dispatch_alerts AS
SELECT
    da.id AS dispatch_alert_id,
    da.flight_id,
    da.task_type,
    da.alert_type,
    da.severity,
    da.message,
    da.is_resolved,
    da.resolved_at,
    da.resolved_by,
    da.created_at
FROM public.dispatch_alerts AS da;

CREATE OR REPLACE VIEW ai_query.v_shift_handovers AS
SELECT
    sh.handover_id,
    sh.shift_date,
    sh.shift_code,
    sh.from_user_id,
    sh.to_user_id,
    sh.status,
    sh.risk_level,
    sh.summary,
    sh.signed_at,
    sh.submitted_at,
    sh.created_at,
    sh.updated_at
FROM public.shift_handovers AS sh;

CREATE OR REPLACE VIEW ai_query.v_notifications AS
SELECT
    n.notification_id,
    n.user_id,
    n.title,
    n.body,
    n.category,
    n.severity,
    n.delivery_status,
    n.is_read,
    n.ack_status,
    n.related_entity_type,
    n.related_entity_id,
    n.created_at
FROM public.notifications AS n;

CREATE OR REPLACE VIEW ai_query.v_online_history AS
SELECT
    oh.id,
    oh.user_id,
    oh.session_id,
    oh.login_time,
    oh.logout_time,
    oh.duration_seconds,
    oh.forced_logout,
    oh.created_at
FROM public.online_history AS oh;

CREATE OR REPLACE VIEW ai_query.v_daily_kpi AS
SELECT
    k.flight_date,
    k.total_flights,
    k.completed_flights,
    k.avg_turnaround_minutes,
    k.p90_turnaround_minutes,
    k.on_time_departure_rate,
    k.on_time_arrival_rate,
    k.abnormal_ratio
FROM public.mv_daily_flight_kpi AS k;

CREATE OR REPLACE VIEW ai_query.v_ops_overview AS
SELECT
    (SELECT COUNT(*) FROM ai_query.v_flights) AS flights_total,
    (
        SELECT COUNT(*)
        FROM ai_query.v_flights
        WHERE status NOT IN (7, 8, 9)
    ) AS flights_active,
    (
        SELECT COUNT(*)
        FROM ai_query.v_anomalies
        WHERE status = 'open'
    ) AS anomalies_open,
    (
        SELECT COUNT(*)
        FROM ai_query.v_todos
        WHERE is_deleted = FALSE AND status IN ('待办', '进行中')
    ) AS todos_open,
    CURRENT_TIMESTAMP AS snapshot_at;

COMMENT ON VIEW ai_query.v_flights IS 'AI 只读航班视图（脱敏/字段裁剪入口）';
COMMENT ON VIEW ai_query.v_anomalies IS 'AI 只读异常视图';
COMMENT ON VIEW ai_query.v_todos IS 'AI 只读待办视图';
COMMENT ON VIEW ai_query.v_dispatch_orders IS 'AI 只读派工单视图';
COMMENT ON VIEW ai_query.v_dispatch_alerts IS 'AI 只读派工告警视图';
COMMENT ON VIEW ai_query.v_shift_handovers IS 'AI 只读交接班视图';
COMMENT ON VIEW ai_query.v_notifications IS 'AI 只读通知视图';
COMMENT ON VIEW ai_query.v_online_history IS 'AI 只读在线历史视图';
COMMENT ON VIEW ai_query.v_daily_kpi IS 'AI 只读 KPI 物化视图代理';
COMMENT ON VIEW ai_query.v_ops_overview IS 'AI 只读运营概览汇总视图';

