-- PR7 岗位/个人账号分离：交接班只读视图补 position_user_id
-- docs/plans/2026-08-25-ontology-team-equipment-personnel-design.md §交接班
--
-- 迁移 139 已给表 `shift_handovers` 加 `position_user_id` 列；`ai_query.v_shift_handovers`
-- 是显式列清单（非 `SELECT *`），需补上该列，仓储 `find_by_id`/`find_all` 才能读到。
-- 视图不动 FK / 约束，仅加一列。

DROP VIEW IF EXISTS ai_query.v_shift_handovers;

CREATE VIEW ai_query.v_shift_handovers AS
SELECT
    sh.handover_id,
    sh.shift_date,
    sh.shift_code,
    sh.from_user_id,
    sh.to_user_id,
    sh.position_user_id,
    sh.from_operator_name,
    sh.from_operator_job_title,
    CASE
        WHEN COALESCE(NULLIF(BTRIM(sh.from_operator_name), ''), NULLIF(BTRIM(COALESCE(from_user.display_name, from_user.username)), '')) IS NULL
            THEN NULL
        ELSE CONCAT(
            COALESCE(NULLIF(BTRIM(sh.from_operator_name), ''), NULLIF(BTRIM(COALESCE(from_user.display_name, from_user.username)), '')),
            '-',
            COALESCE(
                NULLIF(BTRIM(sh.from_operator_job_title), ''),
                NULLIF(BTRIM(from_user.job_title), ''),
                NULLIF(BTRIM(from_role.role_name), ''),
                CASE WHEN COALESCE(from_user.is_admin, FALSE) THEN 'admin' ELSE '用户' END
            )
        )
    END AS from_operator_label,
    sh.to_operator_name,
    sh.to_operator_job_title,
    CASE
        WHEN COALESCE(NULLIF(BTRIM(sh.to_operator_name), ''), NULLIF(BTRIM(COALESCE(to_user.display_name, to_user.username)), '')) IS NULL
            THEN NULL
        ELSE CONCAT(
            COALESCE(NULLIF(BTRIM(sh.to_operator_name), ''), NULLIF(BTRIM(COALESCE(to_user.display_name, to_user.username)), '')),
            '-',
            COALESCE(
                NULLIF(BTRIM(sh.to_operator_job_title), ''),
                NULLIF(BTRIM(to_user.job_title), ''),
                NULLIF(BTRIM(to_role.role_name), ''),
                CASE WHEN COALESCE(to_user.is_admin, FALSE) THEN 'admin' ELSE '用户' END
            )
        )
    END AS to_operator_label,
    sh.status,
    sh.risk_level,
    sh.summary,
    sh.signed_at,
    sh.submitted_at,
    sh.created_at,
    sh.updated_at
FROM public.shift_handovers AS sh
LEFT JOIN public.users AS from_user ON from_user.id = sh.from_user_id
LEFT JOIN public.users AS to_user ON to_user.id = sh.to_user_id
LEFT JOIN LATERAL (
    SELECT r.name AS role_name
    FROM public.user_roles ur
    JOIN public.roles r ON r.id = ur.role_id
    WHERE ur.user_id = sh.from_user_id
    ORDER BY r.name ASC
    LIMIT 1
) AS from_role ON TRUE
LEFT JOIN LATERAL (
    SELECT r.name AS role_name
    FROM public.user_roles ur
    JOIN public.roles r ON r.id = ur.role_id
    WHERE ur.user_id = sh.to_user_id
    ORDER BY r.name ASC
    LIMIT 1
) AS to_role ON TRUE;

COMMENT ON VIEW ai_query.v_shift_handovers IS 'AI 只读交接班视图（含 position_user_id）';

-- ============================================================================
-- 回滚
-- ============================================================================
/*
DROP VIEW IF EXISTS ai_query.v_shift_handovers;
*/