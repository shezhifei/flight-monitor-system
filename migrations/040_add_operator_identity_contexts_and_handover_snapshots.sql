-- Add operator identity contexts and shift handover operator snapshots


ALTER TABLE users
    ADD COLUMN IF NOT EXISTS display_name VARCHAR(100);

COMMENT ON COLUMN users.display_name IS '账号默认展示姓名';

CREATE INDEX IF NOT EXISTS idx_users_display_name
    ON users(display_name)
    WHERE display_name IS NOT NULL;

CREATE TABLE IF NOT EXISTS operator_identity_contexts (
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    context_type VARCHAR(32) NOT NULL,
    context_id VARCHAR(128) NOT NULL,
    operator_name VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, context_type, context_id),
    CONSTRAINT chk_operator_identity_context_type
        CHECK (context_type IN ('mobile_device', 'web_client'))
);

CREATE INDEX IF NOT EXISTS idx_operator_identity_contexts_scope
    ON operator_identity_contexts(context_type, context_id, updated_at DESC);

ALTER TABLE shift_handovers
    ADD COLUMN IF NOT EXISTS from_operator_name VARCHAR(100),
    ADD COLUMN IF NOT EXISTS from_operator_job_title VARCHAR(100),
    ADD COLUMN IF NOT EXISTS to_operator_name VARCHAR(100),
    ADD COLUMN IF NOT EXISTS to_operator_job_title VARCHAR(100);

COMMENT ON COLUMN shift_handovers.from_operator_name IS '交班方姓名快照';
COMMENT ON COLUMN shift_handovers.from_operator_job_title IS '交班方职务快照';
COMMENT ON COLUMN shift_handovers.to_operator_name IS '接班方姓名快照';
COMMENT ON COLUMN shift_handovers.to_operator_job_title IS '接班方职务快照';

CREATE SCHEMA IF NOT EXISTS ai_query;

DROP VIEW IF EXISTS ai_query.v_shift_handovers;

CREATE VIEW ai_query.v_shift_handovers AS
SELECT
    sh.handover_id,
    sh.shift_date,
    sh.shift_code,
    sh.from_user_id,
    sh.to_user_id,
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



