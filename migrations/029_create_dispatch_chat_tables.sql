-- Create dispatch chat tables


CREATE TABLE IF NOT EXISTS dispatch_chat_groups (
    group_id VARCHAR(26) PRIMARY KEY,
    channel_type VARCHAR(32) NOT NULL DEFAULT 'system_flight_dispatch',
    flight_id VARCHAR(26) NOT NULL,
    group_name VARCHAR(120) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    read_only BOOLEAN NOT NULL DEFAULT FALSE,
    archive_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_chat_groups_channel_flight UNIQUE (channel_type, flight_id),
    CONSTRAINT fk_dispatch_chat_groups_flight FOREIGN KEY (flight_id) REFERENCES flights(flight_id) ON DELETE CASCADE,
    CONSTRAINT chk_dispatch_chat_group_status CHECK (status IN ('active', 'archived'))
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_flight_id
    ON dispatch_chat_groups(flight_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_status_read_only
    ON dispatch_chat_groups(status, read_only);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_archive_at
    ON dispatch_chat_groups(archive_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_updated_at_desc
    ON dispatch_chat_groups(updated_at DESC);

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
    CONSTRAINT fk_dispatch_chat_group_members_group FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE CASCADE,
    CONSTRAINT fk_dispatch_chat_group_members_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT chk_dispatch_chat_group_member_role CHECK (is_assignee OR is_dispatcher)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_user_active
    ON dispatch_chat_group_members(user_id, is_active);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_group_active
    ON dispatch_chat_group_members(group_id, is_active);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_group_read_seq
    ON dispatch_chat_group_members(group_id, last_read_seq);

CREATE TABLE IF NOT EXISTS dispatch_chat_messages (
    message_id VARCHAR(26) PRIMARY KEY,
    seq_no BIGSERIAL UNIQUE,
    group_id VARCHAR(26) NOT NULL,
    sender_user_id VARCHAR(26),
    message_type VARCHAR(16) NOT NULL DEFAULT 'text',
    content TEXT NOT NULL,
    is_at_all BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_dispatch_chat_messages_group FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE CASCADE,
    CONSTRAINT fk_dispatch_chat_messages_sender FOREIGN KEY (sender_user_id) REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT chk_dispatch_chat_message_type CHECK (message_type IN ('text', 'system')),
    CONSTRAINT chk_dispatch_chat_message_content_len CHECK (char_length(trim(content)) BETWEEN 1 AND 2000)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_group_seq_desc
    ON dispatch_chat_messages(group_id, seq_no DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_group_sent_desc
    ON dispatch_chat_messages(group_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_sender_sent_desc
    ON dispatch_chat_messages(sender_user_id, sent_at DESC);

COMMENT ON TABLE dispatch_chat_groups IS '按航班维度自动生成的保障协同群';
COMMENT ON TABLE dispatch_chat_group_members IS '群成员关系，含成员角色与已读游标';
COMMENT ON TABLE dispatch_chat_messages IS '群消息表，首版支持文本与系统消息';

