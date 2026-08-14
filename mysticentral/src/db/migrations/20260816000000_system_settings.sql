-- Single-row system settings table
CREATE TABLE IF NOT EXISTS system_settings (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    central_url TEXT NOT NULL DEFAULT '',
    sync_interval_secs INTEGER NOT NULL DEFAULT 30,
    log_level TEXT NOT NULL DEFAULT 'info',
    max_request_history INTEGER NOT NULL DEFAULT 1000,
    default_environment TEXT,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

INSERT INTO system_settings (id) VALUES (TRUE) ON CONFLICT DO NOTHING;
