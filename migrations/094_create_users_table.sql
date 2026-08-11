-- Migration 094: Create users table (idempotent)
-- Description: Originally created the users table here, but since migration 007+
--              already reference users(id) via FK constraints, the table is now
--              created in migration 000 to ensure fresh deployments work.
--              This migration is kept for backward compatibility with existing
--              databases that ran 000 but not 094. The CREATE TABLE IF NOT EXISTS
--              is a safe no-op in both cases.


CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(26) PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    email VARCHAR(128) UNIQUE,
    password_hash VARCHAR(256),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    verification_token VARCHAR(128),
    verification_token_expires TIMESTAMPTZ,
    verified_at TIMESTAMPTZ,
    password_reset_token VARCHAR(128),
    password_reset_token_expires TIMESTAMPTZ,
    password_changed_at TIMESTAMPTZ,
    last_login_at TIMESTAMPTZ,
    department VARCHAR(64),
    department_id VARCHAR(26),
    job_level SMALLINT DEFAULT 1,
    job_title VARCHAR(64),
    permission_version INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_department_id ON users(department_id) WHERE department_id IS NOT NULL;

