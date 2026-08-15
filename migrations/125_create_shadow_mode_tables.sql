-- 125_create_shadow_mode_tables.sql
-- Shadow Mode (Phase 1) infrastructure: operator roster, operator feedback,
-- and AI-vs-human discrepancy tracking.
-- Style: idempotent (safe to re-run), NO foreign keys (per migration 120
-- policy which dropped all FKs and relies on application-level integrity).

BEGIN;

-- ============================================================
-- 1) shadow_mode_operators — volunteer roster for shadow runs
-- ============================================================
CREATE TABLE IF NOT EXISTS shadow_mode_operators (
    id              BIGSERIAL PRIMARY KEY,
    user_id         BIGINT NOT NULL,
    display_name    VARCHAR(120) NOT NULL,
    department      VARCHAR(120),
    shift_pattern   VARCHAR(60),
    expertise_level VARCHAR(30) NOT NULL DEFAULT 'intermediate'
                    CHECK (expertise_level IN ('junior', 'intermediate', 'senior')),
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    enrolled_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    opted_out_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_shadow_mode_operators_user
    ON shadow_mode_operators (user_id);

-- ============================================================
-- 2) operator_feedback — human answers in the double-run loop
-- ============================================================
CREATE TABLE IF NOT EXISTS operator_feedback (
    id                  BIGSERIAL PRIMARY KEY,
    session_id          UUID NOT NULL,
    operator_id         BIGINT NOT NULL,
    question_text       TEXT NOT NULL,
    human_answer        TEXT NOT NULL,
    answer_duration_ms  INTEGER,
    confidence_self     SMALLINT CHECK (confidence_self BETWEEN 1 AND 5),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_operator_feedback_session
    ON operator_feedback (session_id);
CREATE INDEX IF NOT EXISTS idx_operator_feedback_created
    ON operator_feedback (created_at DESC);

-- ============================================================
-- 3) shadow_mode_discrepancies — AI vs human comparison results
-- ============================================================
CREATE TABLE IF NOT EXISTS shadow_mode_discrepancies (
    id                      BIGSERIAL PRIMARY KEY,
    session_id              UUID NOT NULL,
    feedback_id             BIGINT,
    question_text           TEXT NOT NULL,
    ai_answer               TEXT NOT NULL,
    human_answer            TEXT NOT NULL,
    ai_confidence           NUMERIC(4, 3),
    discrepancy_type        VARCHAR(40) NOT NULL
                            CHECK (discrepancy_type IN (
                                'missing_information',
                                'conflicting_data',
                                'over_confidence'
                            )),
    severity                VARCHAR(20) NOT NULL
                            CHECK (severity IN (
                                'critical', 'major', 'minor', 'informational'
                            )),
    resolved                BOOLEAN NOT NULL DEFAULT FALSE,
    resolution_note         TEXT,
    resolved_at             TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_shadow_discrepancies_session
    ON shadow_mode_discrepancies (session_id);
CREATE INDEX IF NOT EXISTS idx_shadow_discrepancies_severity_created
    ON shadow_mode_discrepancies (severity, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_shadow_discrepancies_unresolved
    ON shadow_mode_discrepancies (resolved, created_at DESC)
    WHERE NOT resolved;

COMMIT;

-- ============================================================
-- Rollback
-- ============================================================
-- BEGIN;
-- DROP INDEX IF EXISTS idx_shadow_discrepancies_unresolved;
-- DROP INDEX IF EXISTS idx_shadow_discrepancies_severity_created;
-- DROP INDEX IF EXISTS idx_shadow_discrepancies_session;
-- DROP INDEX IF EXISTS idx_operator_feedback_created;
-- DROP INDEX IF EXISTS idx_operator_feedback_session;
-- DROP INDEX IF EXISTS uq_shadow_mode_operators_user;
-- DROP TABLE IF EXISTS shadow_mode_discrepancies;
-- DROP TABLE IF EXISTS operator_feedback;
-- DROP TABLE IF EXISTS shadow_mode_operators;
-- COMMIT;