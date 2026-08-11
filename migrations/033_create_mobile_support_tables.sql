-- Create mobile workbench support tables (device registrations and uploads)


CREATE TABLE IF NOT EXISTS mobile_device_registrations (
    device_id VARCHAR(64) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform VARCHAR(32) NOT NULL DEFAULT 'android',
    push_channel VARCHAR(32) NOT NULL DEFAULT 'none',
    push_token TEXT,
    app_version VARCHAR(64),
    os_version VARCHAR(64),
    device_model VARCHAR(128),
    manufacturer VARCHAR(64),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT chk_mobile_device_push_channel
        CHECK (push_channel IN ('none', 'fcm', 'hms', 'xiaomi', 'oppo', 'vivo', 'wecom'))
);

CREATE INDEX IF NOT EXISTS idx_mobile_devices_user_active_heartbeat
    ON mobile_device_registrations(user_id, is_active, last_heartbeat_at DESC);

CREATE INDEX IF NOT EXISTS idx_mobile_devices_push_channel_active
    ON mobile_device_registrations(push_channel, is_active, last_heartbeat_at DESC);

CREATE INDEX IF NOT EXISTS idx_mobile_devices_push_token
    ON mobile_device_registrations(push_token)
    WHERE push_token IS NOT NULL;

CREATE TABLE IF NOT EXISTS mobile_upload_assets (
    upload_id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    storage_key VARCHAR(255) NOT NULL UNIQUE,
    original_filename VARCHAR(255) NOT NULL,
    content_type VARCHAR(128),
    file_size BIGINT NOT NULL DEFAULT 0,
    checksum_sha256 VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT chk_mobile_upload_size_non_negative
        CHECK (file_size >= 0)
);

CREATE INDEX IF NOT EXISTS idx_mobile_upload_assets_user_created
    ON mobile_upload_assets(user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_mobile_upload_assets_checksum
    ON mobile_upload_assets(checksum_sha256)
    WHERE checksum_sha256 IS NOT NULL;

