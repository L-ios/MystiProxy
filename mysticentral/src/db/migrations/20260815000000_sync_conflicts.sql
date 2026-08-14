-- Persistent sync conflict queue
-- One row per conflicting config; upsert overwrites on repeat detection.

CREATE TABLE IF NOT EXISTS sync_conflicts (
    config_id UUID PRIMARY KEY,
    local_version JSONB NOT NULL,
    central_version JSONB NOT NULL,
    detected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);
