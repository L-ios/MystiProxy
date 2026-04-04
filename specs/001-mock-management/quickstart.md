# Quick Start: HTTP Mock Management System

This guide helps you quickly set up and use the HTTP Mock Management System.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Central Management                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Frontend   │  │  Backend    │  │     PostgreSQL      │  │
│  │  (React)    │  │  (Axum)     │  │                     │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
         │                    │
         │ HTTP/WebSocket     │ REST API
         ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                    MystiProxy Instance                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  Local Web  │  │  Proxy      │  │     SQLite          │  │
│  │  (Embedded) │  │  Engine     │  │     (Embedded)      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Prerequisites

- Rust 1.75+ (for backend)
- Node.js 18+ (for frontend)
- PostgreSQL 15+ (for central management)
- Docker (optional, for containerized deployment)

## Quick Setup

### 1. Central Management Setup

```bash
# Clone and build
cd mystiproxy
cargo build --release -p mysticentral

# Setup database
sqlx database create
sqlx migrate run

# Start central server
./target/release/mysticentral --config central.yaml
```

### 2. MystiProxy with Local Management

```bash
# Build MystiProxy with management feature
cargo build --release --features local-management

# Start MystiProxy
./target/release/mystiproxy --config proxy.yaml
```

### 3. Access Dashboards

- Central Management: http://localhost:8080
- MystiProxy Local: http://localhost:9090

## Basic Usage

### Creating a Mock via API

```bash
# Create a simple mock
curl -X POST http://localhost:8080/api/v1/mocks \
  -H "Content-Type: application/json" \
  -d '{
    "name": "User API Mock",
    "path": "/api/users/{id}",
    "method": "GET",
    "matching_rules": {
      "path_pattern_type": "exact"
    },
    "response_config": {
      "status": 200,
      "headers": {
        "Content-Type": "application/json"
      },
      "body": {
        "type": "template",
        "content": "{\"id\": \"{{request.path.id}}\", \"name\": \"User {{request.path.id}}\"}"
      }
    }
  }'
```

### Testing the Mock

```bash
# Test the mock endpoint
curl http://localhost:9090/api/users/123
# Response: {"id": "123", "name": "User 123"}
```

### Sync Configuration

```bash
# Check sync status
curl http://localhost:9090/api/v1/sync/status

# Force sync with central
curl -X POST http://localhost:9090/api/v1/sync/pull
```

## Configuration Files

### Central Management (central.yaml)

```yaml
server:
  host: 0.0.0.0
  port: 8080

database:
  url: postgresql://user:pass@localhost/mysticentral
  max_connections: 10

auth:
  jwt_secret: your-secret-key
  token_expiry: 24h

frontend:
  serve: true
  path: ./frontend/dist
```

### MystiProxy with Local Management (proxy.yaml)

```yaml
server:
  listen: tcp://0.0.0.0:9090

proxy:
  target: tcp://127.0.0.1:8081

management:
  enabled: true
  central_url: http://localhost:8080
  instance_name: local-dev
  api_key: your-api-key
  sync_interval: 30s

database:
  path: ./mystiproxy.db
```

## Common Workflows

### 1. Development Workflow

1. Start MystiProxy locally
2. Create mocks via local dashboard (http://localhost:9090)
3. Test your application against mocks
4. Changes automatically sync to central when connected

### 2. Team Collaboration

1. Team lead creates mocks in central
2. Push configurations to team environments
3. Team members pull latest configs
4. Resolve conflicts through dashboard

### 3. CI/CD Integration

```yaml
# GitHub Actions example
- name: Sync Mock Configs
  run: |
    curl -X POST $CENTRAL_URL/api/v1/instances/$INSTANCE_ID/push \
      -H "Authorization: Bearer $API_TOKEN" \
      -H "Content-Type: application/json" \
      -d '{"config_ids": ["all"]}'
```

## Troubleshooting

### Sync Issues

```bash
# Check connection to central
curl http://localhost:9090/api/v1/sync/health

# View sync logs
tail -f /var/log/mystiproxy/sync.log

# Reset local database
rm mystiproxy.db && ./mystiproxy --config proxy.yaml
```

### Conflict Resolution

1. Open local dashboard
2. Navigate to Conflicts section
3. Choose resolution strategy:
   - Keep Local: Your changes override central
   - Keep Central: Discard local changes
   - Merge: Manually combine changes

## Next Steps

- Read the [API Documentation](./contracts/openapi.yaml)
- Explore [Data Model](./data-model.md)
- Review [Research Decisions](./research.md)
