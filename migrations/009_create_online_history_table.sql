-- 在线历史记录表
-- 用于记录用户登录/登出历史

CREATE TABLE IF NOT EXISTS online_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(50) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id VARCHAR(100) NOT NULL,
    login_time TIMESTAMP WITH TIME ZONE NOT NULL,
    logout_time TIMESTAMP WITH TIME ZONE,
    duration_seconds INTEGER,
    ip_address INET,
    device_info VARCHAR(200),
    forced_logout BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 索引优化
CREATE INDEX IF NOT EXISTS idx_online_history_user_login 
    ON online_history(user_id, login_time DESC);
CREATE INDEX IF NOT EXISTS idx_online_history_session 
    ON online_history(session_id);
CREATE INDEX IF NOT EXISTS idx_online_history_login_time 
    ON online_history(login_time DESC);

-- 注释
COMMENT ON TABLE online_history IS '用户在线历史记录';
COMMENT ON COLUMN online_history.duration_seconds IS '在线时长（秒）';
COMMENT ON COLUMN online_history.forced_logout IS '是否被强制下线';
