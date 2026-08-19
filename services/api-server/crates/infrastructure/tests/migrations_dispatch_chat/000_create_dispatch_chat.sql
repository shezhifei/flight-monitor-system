-- Minimal schema for dispatch chat repository tests.
--
-- Mirrors migrations 029 / 035 / 038 / 127 for the tables these tests touch.
-- Foreign keys to flights/users are dropped on purpose: the tests need a group
-- without a flights row, and a member whose users row is missing.
--
-- `seq_no` MUST stay a table-global BIGSERIAL. The unread-count regression these
-- tests guard only reproduces when one group's sequence numbers are advanced by
-- another group's traffic.

CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(26) PRIMARY KEY,
    username VARCHAR(100),
    display_name VARCHAR(100),
    department VARCHAR(100),
    job_title VARCHAR(100),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS dispatch_chat_groups (
    group_id VARCHAR(26) PRIMARY KEY,
    channel_type VARCHAR(32) NOT NULL DEFAULT 'system_flight_dispatch',
    flight_id VARCHAR(26) NOT NULL,
    group_name VARCHAR(120) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    read_only BOOLEAN NOT NULL DEFAULT FALSE,
    archive_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    deprecated_at TIMESTAMPTZ,
    deprecation_reason VARCHAR(64),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_chat_groups_channel_flight UNIQUE (channel_type, flight_id),
    CONSTRAINT chk_dispatch_chat_group_status CHECK (status IN ('active', 'archived'))
);

CREATE TABLE IF NOT EXISTS dispatch_chat_group_members (
    id VARCHAR(26) PRIMARY KEY,
    group_id VARCHAR(26) NOT NULL,
    user_id VARCHAR(26) NOT NULL,
    is_assignee BOOLEAN NOT NULL DEFAULT FALSE,
    is_dispatcher BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    left_at TIMESTAMPTZ,
    last_read_seq BIGINT NOT NULL DEFAULT 0,
    last_read_at TIMESTAMPTZ,
    CONSTRAINT uq_dispatch_chat_group_member UNIQUE (group_id, user_id),
    CONSTRAINT chk_dispatch_chat_group_member_role CHECK (is_assignee OR is_dispatcher)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_group_active
    ON dispatch_chat_group_members(group_id, is_active);

CREATE TABLE IF NOT EXISTS dispatch_chat_messages (
    message_id VARCHAR(26) PRIMARY KEY,
    seq_no BIGSERIAL UNIQUE,
    group_id VARCHAR(26) NOT NULL,
    sender_user_id VARCHAR(26),
    dispatch_order_id VARCHAR(26),
    event_id VARCHAR(26),
    message_type VARCHAR(16) NOT NULL DEFAULT 'text',
    content TEXT NOT NULL,
    is_at_all BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    client_msg_id VARCHAR(64),
    CONSTRAINT chk_dispatch_chat_message_type CHECK (message_type IN ('text', 'system')),
    CONSTRAINT chk_dispatch_chat_message_content_len CHECK (char_length(trim(content)) BETWEEN 1 AND 2000)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_group_seq_desc
    ON dispatch_chat_messages(group_id, seq_no DESC);

CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_chat_messages_group_client_msg
    ON dispatch_chat_messages (group_id, client_msg_id)
    WHERE client_msg_id IS NOT NULL;
