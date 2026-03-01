-- Initial schema for MystiCentral
-- Creates core tables for mock management system

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- User table for team collaboration (Phase 4)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(64) UNIQUE NOT NULL,
    email VARCHAR(256) UNIQUE NOT NULL,
    password_hash VARCHAR(256) NOT NULL,
    role VARCHAR(16) NOT NULL DEFAULT 'viewer',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_user_username ON users(username);
CREATE INDEX idx_user_email ON users(email);

-- Team table
CREATE TABLE teams (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(128) UNIQUE NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Team member junction table
CREATE TABLE team_members (
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(16) NOT NULL DEFAULT 'viewer',
    joined_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, user_id)
);

-- Environment table
CREATE TABLE environments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(64) NOT NULL,
    description TEXT,
    endpoints JSONB NOT NULL DEFAULT '{}',
    is_template BOOLEAN NOT NULL DEFAULT FALSE,
    template_id UUID REFERENCES environments(id),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Mock configuration table (core entity)
CREATE TABLE mock_configurations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(256) NOT NULL,
    path VARCHAR(1024) NOT NULL,
    method VARCHAR(16) NOT NULL,
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
    environment_id UUID REFERENCES environments(id) ON DELETE SET NULL,
    matching_rules JSONB NOT NULL,
    response_config JSONB NOT NULL,
    state_config JSONB,
    source VARCHAR(16) NOT NULL DEFAULT 'central',
    version_vector JSONB NOT NULL DEFAULT '{}',
    content_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_by UUID REFERENCES users(id)
);

-- Indexes for mock_configurations
CREATE INDEX idx_mock_path_method ON mock_configurations(path, method);
CREATE INDEX idx_mock_team ON mock_configurations(team_id);
CREATE INDEX idx_mock_environment ON mock_configurations(environment_id);
CREATE INDEX idx_mock_content_hash ON mock_configurations(content_hash);
CREATE INDEX idx_mock_source ON mock_configurations(source);

-- MystiProxy instance table
CREATE TABLE mystiproxy_instances (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(128) NOT NULL,
    endpoint_url VARCHAR(512) NOT NULL,
    api_key_hash VARCHAR(256),
    sync_status VARCHAR(16) NOT NULL DEFAULT 'disconnected',
    last_sync_at TIMESTAMP WITH TIME ZONE,
    config_checksum VARCHAR(64),
    registered_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_heartbeat TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_instance_status ON mystiproxy_instances(sync_status);
CREATE INDEX idx_instance_heartbeat ON mystiproxy_instances(last_heartbeat);

-- Sync record table
CREATE TABLE sync_records (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    config_id UUID NOT NULL REFERENCES mock_configurations(id) ON DELETE CASCADE,
    instance_id UUID NOT NULL REFERENCES mystiproxy_instances(id) ON DELETE CASCADE,
    operation_type VARCHAR(16) NOT NULL,
    source VARCHAR(16) NOT NULL,
    conflict_status VARCHAR(16) NOT NULL DEFAULT 'none',
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    payload_snapshot JSONB
);

CREATE INDEX idx_sync_config ON sync_records(config_id);
CREATE INDEX idx_sync_instance ON sync_records(instance_id);
CREATE INDEX idx_sync_timestamp ON sync_records(timestamp);

-- Analytics record table
CREATE TABLE analytics_records (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    mock_id UUID NOT NULL REFERENCES mock_configurations(id) ON DELETE CASCADE,
    instance_id UUID NOT NULL REFERENCES mystiproxy_instances(id) ON DELETE CASCADE,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    request_details JSONB NOT NULL,
    response_time_ms INTEGER NOT NULL,
    status_code INTEGER NOT NULL,
    error_info TEXT
);

CREATE INDEX idx_analytics_mock ON analytics_records(mock_id);
CREATE INDEX idx_analytics_timestamp ON analytics_records(timestamp);
CREATE INDEX idx_analytics_status ON analytics_records(status_code);

-- Updated_at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Apply updated_at triggers
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_teams_updated_at BEFORE UPDATE ON teams
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_environments_updated_at BEFORE UPDATE ON environments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_mock_configurations_updated_at BEFORE UPDATE ON mock_configurations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
