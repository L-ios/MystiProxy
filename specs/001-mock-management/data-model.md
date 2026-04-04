# Data Model: HTTP Mock Management System

**Date**: 2026-03-01
**Feature**: 001-mock-management

## Entity Relationship Diagram

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│      User        │     │      Team        │     │   TeamMember     │
├──────────────────┤     ├──────────────────┤     ├──────────────────┤
│ id (PK)          │     │ id (PK)          │     │ team_id (FK)     │
│ username         │     │ name             │     │ user_id (FK)     │
│ email            │     │ description      │     │ role             │
│ password_hash    │     │ created_at       │     │ joined_at        │
│ role             │     │ updated_at       │     └──────────────────┘
│ created_at       │     └──────────────────┘
│ updated_at       │              │
└──────────────────┘              │
         │                        │
         │                        ▼
         │              ┌──────────────────┐
         │              │ MockConfiguration│
         │              ├──────────────────┤
         │              │ id (PK)          │
         │              │ name             │
         │              │ path             │
         │              │ method           │
         │              │ team_id (FK)     │
         │              │ environment_id   │
         │              │ matching_rules   │
         │              │ response_config  │
         │              │ state_config     │
         │              │ source           │
         │              │ version_vector   │
         │              │ content_hash     │
         │              │ created_at       │
         │              │ updated_at       │
         │              │ created_by (FK)  │
         │              └──────────────────┘
         │                        │
         │                        │
         ▼                        ▼
┌──────────────────┐     ┌──────────────────┐
│   Environment    │     │   SyncRecord     │
├──────────────────┤     ├──────────────────┤
│ id (PK)          │     │ id (PK)          │
│ name             │     │ config_id (FK)   │
│ description      │     │ instance_id (FK) │
│ endpoints        │     │ operation_type   │
│ is_template      │     │ source           │
│ template_id (FK) │     │ conflict_status  │
│ created_at       │     │ timestamp        │
│ updated_at       │     │ payload_snapshot │
└──────────────────┘     └──────────────────┘
         │
         │
         ▼
┌──────────────────┐     ┌──────────────────┐
│ MystiProxyInstance│    │  AnalyticsRecord │
├──────────────────┤     ├──────────────────┤
│ id (PK)          │     │ id (PK)          │
│ name             │     │ mock_id (FK)     │
│ endpoint_url     │     │ instance_id (FK) │
│ api_key_hash     │     │ timestamp        │
│ sync_status      │     │ request_details  │
│ last_sync_at     │     │ response_time_ms │
│ config_checksum  │     │ status_code      │
│ registered_at    │     │ error_info       │
│ last_heartbeat   │     └──────────────────┘
└──────────────────┘
```

## Core Entities

### User

Represents a system user with authentication credentials and role-based permissions.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| username | VARCHAR(64) | UNIQUE, NOT NULL | Login username |
| email | VARCHAR(256) | UNIQUE, NOT NULL | Email address |
| password_hash | VARCHAR(256) | NOT NULL | Bcrypt hashed password |
| role | ENUM | NOT NULL | 'admin', 'editor', 'viewer' |
| created_at | TIMESTAMP | NOT NULL | Creation timestamp |
| updated_at | TIMESTAMP | NOT NULL | Last update timestamp |

**Indexes**:
- `idx_user_username` on (username)
- `idx_user_email` on (email)

### Team

Represents a group of users with shared access to mock configurations.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| name | VARCHAR(128) | UNIQUE, NOT NULL | Team name |
| description | TEXT | NULLABLE | Team description |
| created_at | TIMESTAMP | NOT NULL | Creation timestamp |
| updated_at | TIMESTAMP | NOT NULL | Last update timestamp |

### TeamMember

Junction table for User-Team many-to-many relationship.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| team_id | UUID | FK, NOT NULL | Reference to Team |
| user_id | UUID | FK, NOT NULL | Reference to User |
| role | ENUM | NOT NULL | 'owner', 'editor', 'viewer' |
| joined_at | TIMESTAMP | NOT NULL | Join timestamp |

**Primary Key**: (team_id, user_id)

### MockConfiguration

Defines a mock endpoint including matching criteria, response template, and metadata.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| name | VARCHAR(256) | NOT NULL | Human-readable name |
| path | VARCHAR(1024) | NOT NULL | URL path pattern |
| method | VARCHAR(16) | NOT NULL | HTTP method(s) |
| team_id | UUID | FK, NULLABLE | Owner team |
| environment_id | UUID | FK, NULLABLE | Environment |
| matching_rules | JSONB | NOT NULL | Headers, body, query matching |
| response_config | JSONB | NOT NULL | Response template, status, headers |
| state_config | JSONB | NULLABLE | State machine configuration |
| source | ENUM | NOT NULL | 'central', 'local' |
| version_vector | JSONB | NOT NULL | Vector clock for sync |
| content_hash | VARCHAR(64) | NOT NULL | SHA-256 of content |
| created_at | TIMESTAMP | NOT NULL | Creation timestamp |
| updated_at | TIMESTAMP | NOT NULL | Last update timestamp |
| created_by | UUID | FK, NOT NULL | Creator user |

**Indexes**:
- `idx_mock_path_method` on (path, method)
- `idx_mock_team` on (team_id)
- `idx_mock_environment` on (environment_id)
- `idx_mock_content_hash` on (content_hash)

### Environment

Represents a deployment environment (dev, test, staging, production).

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| name | VARCHAR(64) | NOT NULL | Environment name |
| description | TEXT | NULLABLE | Description |
| endpoints | JSONB | NOT NULL | Environment-specific endpoints |
| is_template | BOOLEAN | NOT NULL | Is this a template? |
| template_id | UUID | FK, NULLABLE | Parent template |
| created_at | TIMESTAMP | NOT NULL | Creation timestamp |
| updated_at | TIMESTAMP | NOT NULL | Last update timestamp |

### MystiProxyInstance

Represents a connected MystiProxy proxy server.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| name | VARCHAR(128) | NOT NULL | Instance name |
| endpoint_url | VARCHAR(512) | NOT NULL | Management endpoint URL |
| api_key_hash | VARCHAR(256) | NULLABLE | API key for authentication |
| sync_status | ENUM | NOT NULL | 'connected', 'disconnected', 'syncing', 'conflict' |
| last_sync_at | TIMESTAMP | NULLABLE | Last sync timestamp |
| config_checksum | VARCHAR(64) | NULLABLE | Current config checksum |
| registered_at | TIMESTAMP | NOT NULL | Registration timestamp |
| last_heartbeat | TIMESTAMP | NULLABLE | Last heartbeat |

**Indexes**:
- `idx_instance_status` on (sync_status)
- `idx_instance_heartbeat` on (last_heartbeat)

### SyncRecord

Tracks synchronization events between MystiProxy and Central.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| config_id | UUID | FK, NOT NULL | Affected configuration |
| instance_id | UUID | FK, NOT NULL | MystiProxy instance |
| operation_type | ENUM | NOT NULL | 'create', 'update', 'delete' |
| source | ENUM | NOT NULL | 'central', 'local' |
| conflict_status | ENUM | NOT NULL | 'none', 'detected', 'resolved' |
| timestamp | TIMESTAMP | NOT NULL | Event timestamp |
| payload_snapshot | JSONB | NULLABLE | Configuration snapshot |

**Indexes**:
- `idx_sync_config` on (config_id)
- `idx_sync_instance` on (instance_id)
- `idx_sync_timestamp` on (timestamp)

### AnalyticsRecord

Captures usage data for mock endpoints.

| Field | Type | Constraints | Description |
|-------|------|-------------|-------------|
| id | UUID | PK, NOT NULL | Unique identifier |
| mock_id | UUID | FK, NOT NULL | Mock configuration |
| instance_id | UUID | FK, NOT NULL | MystiProxy instance |
| timestamp | TIMESTAMP | NOT NULL | Request timestamp |
| request_details | JSONB | NOT NULL | Method, path, headers, body hash |
| response_time_ms | INTEGER | NOT NULL | Response time in milliseconds |
| status_code | INTEGER | NOT NULL | HTTP status code |
| error_info | TEXT | NULLABLE | Error message if failed |

**Indexes**:
- `idx_analytics_mock` on (mock_id)
- `idx_analytics_timestamp` on (timestamp)
- `idx_analytics_status` on (status_code)

## JSON Schema Definitions

### MatchingRules

```json
{
  "type": "object",
  "properties": {
    "path_pattern": { "type": "string" },
    "path_pattern_type": { "enum": ["exact", "prefix", "regex"] },
    "headers": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "value": { "type": "string" },
          "match_type": { "enum": ["exact", "regex", "exists"] }
        }
      }
    },
    "query_params": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "value": { "type": "string" },
          "match_type": { "enum": ["exact", "regex", "exists"] }
        }
      }
    },
    "body": {
      "type": "object",
      "properties": {
        "json_path": { "type": "string" },
        "value": { "type": "string" },
        "match_type": { "enum": ["exact", "regex", "json_path"] }
      }
    }
  }
}
```

### ResponseConfig

```json
{
  "type": "object",
  "properties": {
    "status": { "type": "integer" },
    "headers": {
      "type": "object",
      "additionalProperties": { "type": "string" }
    },
    "body": {
      "type": "object",
      "properties": {
        "type": { "enum": ["static", "template", "file", "script"] },
        "content": { "type": "string" },
        "template_vars": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "name": { "type": "string" },
              "source": { "enum": ["path", "query", "header", "body"] },
              "path": { "type": "string" }
            }
          }
        }
      }
    },
    "delay_ms": { "type": "integer" }
  }
}
```

### StateConfig

```json
{
  "type": "object",
  "properties": {
    "initial_state": { "type": "string" },
    "transitions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "from_state": { "type": "string" },
          "to_state": { "type": "string" },
          "trigger": {
            "type": "object",
            "properties": {
              "type": { "enum": ["request", "body", "header"] },
              "condition": { "type": "string" }
            }
          },
          "response": { "$ref": "#/definitions/ResponseConfig" }
        }
      }
    }
  }
}
```

### VersionVector

```json
{
  "type": "object",
  "additionalProperties": { "type": "integer" },
  "description": "Map of instance_id -> counter for conflict detection"
}
```

## State Transitions

### MockConfiguration Lifecycle

```
┌─────────┐    create    ┌─────────┐    update    ┌─────────┐
│  None   │ ───────────> │  Active │ <──────────> │ Modified│
└─────────┘              └─────────┘              └─────────┘
                              │                        │
                              │ delete                 │ sync
                              ▼                        ▼
                         ┌─────────┐              ┌─────────┐
                         │ Deleted │              │ Syncing │
                         └─────────┘              └─────────┘
                                                        │
                                                        │ conflict
                                                        ▼
                                                   ┌──────────┐
                                                   │ Conflict │
                                                   └──────────┘
                                                        │
                                                        │ resolve
                                                        ▼
                                                   ┌─────────┐
                                                   │ Resolved│
                                                   └─────────┘
```

### MystiProxyInstance Sync Status

```
┌─────────────┐    register    ┌─────────────┐
│   Unknown   │ ─────────────> │ Disconnected│
└─────────────┘                └─────────────┘
                                     │
                                     │ connect
                                     ▼
                               ┌─────────────┐
                               │  Connected  │<──────┐
                               └─────────────┘       │
                                     │               │
                                     │ sync          │ sync_complete
                                     ▼               │
                               ┌─────────────┐       │
                               │   Syncing   │───────┘
                               └─────────────┘
                                     │
                                     │ conflict_detected
                                     ▼
                               ┌─────────────┐
                               │  Conflict   │
                               └─────────────┘
                                     │
                                     │ conflict_resolved
                                     ▼
                               ┌─────────────┐
                               │  Connected  │
                               └─────────────┘
```

## Validation Rules

### MockConfiguration

1. `path` must start with `/`
2. `method` must be valid HTTP method or `*`
3. `matching_rules.path_pattern` must be valid regex if `path_pattern_type` is `regex`
4. `response_config.status` must be valid HTTP status code (100-599)
5. `response_config.delay_ms` must be >= 0 and <= 300000 (5 minutes)
6. `version_vector` must contain at least one entry after creation

### User

1. `username` must be 3-64 characters, alphanumeric + underscore
2. `email` must be valid email format
3. `password_hash` must be valid bcrypt hash

### MystiProxyInstance

1. `name` must be unique per team
2. `endpoint_url` must be valid URL
3. `last_heartbeat` must be within 5 minutes for `sync_status` to be 'connected'
