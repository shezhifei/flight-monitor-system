-- Add deprecation marker for dispatch chat groups


ALTER TABLE dispatch_chat_groups
    ADD COLUMN IF NOT EXISTS deprecated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deprecation_reason VARCHAR(64);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_deprecated_at
    ON dispatch_chat_groups(deprecated_at);

COMMENT ON COLUMN dispatch_chat_groups.deprecated_at IS '群聊弃用时间';
COMMENT ON COLUMN dispatch_chat_groups.deprecation_reason IS '群聊弃用原因';

