CREATE TABLE IF NOT EXISTS ai_global_state (
    id VARCHAR(255) PRIMARY KEY,
    state JSONB NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);
-- Insert default overview record
INSERT INTO ai_global_state (id, state) VALUES ('overview', '{}') ON CONFLICT (id) DO NOTHING;
