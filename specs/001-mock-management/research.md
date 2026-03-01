# Research: HTTP Mock Management System

**Date**: 2026-03-01
**Feature**: 001-mock-management

## Technology Decisions

### 1. Central Management System Backend

**Decision**: Rust + Axum web framework

**Rationale**:
- Consistent with existing MystiProxy codebase (Rust)
- Axum provides excellent async support with Tokio
- Type-safe routing and extractors
- Good ecosystem for JWT, CORS, WebSocket support

**Alternatives Considered**:
- Go + Gin: Would introduce new language to project
- Node.js + Express: Not suitable for high-performance requirements
- Python + FastAPI: Performance concerns for sync operations

### 2. Frontend for Management Dashboard

**Decision**: React + TypeScript + Ant Design

**Rationale**:
- Industry-standard for admin dashboards
- Rich component library (tables, forms, trees)
- Good TypeScript support
- Large ecosystem and community

**Alternatives Considered**:
- Vue.js: Less ecosystem for complex admin UIs
- Svelte: Smaller ecosystem, fewer component libraries
- Pure HTML/HTMX: Limited interactivity for complex operations

### 3. Database for Central Management

**Decision**: PostgreSQL with SQLx

**Rationale**:
- Robust relational database for complex queries
- SQLx provides compile-time query checking
- Good support for JSON columns (flexible mock config storage)
- Supports connection pooling

**Alternatives Considered**:
- MySQL: Less advanced JSON support
- SQLite: Not suitable for multi-instance deployment
- MongoDB: Less suitable for relational data (users, teams, permissions)

### 4. Embedded Database for MystiProxy Local

**Decision**: SQLite (via Rusqlite or SQLx)

**Rationale**:
- Zero-configuration embedded database
- File-based, portable across platforms
- Supports concurrent reads
- Same schema as central for consistency

**Alternatives Considered**:
- sled: Key-value only, no relational queries
- RocksDB: More complex setup, overkill for local use
- redb: Less mature, smaller ecosystem

### 5. Real-time Push Mechanism

**Decision**: WebSocket for push + HTTP long-polling fallback

**Rationale**:
- WebSocket provides real-time bidirectional communication
- Long-polling as fallback for restricted networks
- axum-extra provides WebSocket support
- Works well with Tokio async runtime

**Alternatives Considered**:
- Server-Sent Events (SSE): Unidirectional only
- gRPC streaming: More complex, overkill for this use case
- Webhook only: Requires MystiProxy to expose endpoint

### 6. Sync Protocol

**Decision**: Custom JSON-based protocol over WebSocket + REST

**Rationale**:
- Simple to implement and debug
- JSON is human-readable for troubleshooting
- REST for initial load and reconnection
- WebSocket for incremental updates

**Protocol Structure**:
```json
{
  "type": "config_update" | "config_delete" | "sync_request" | "sync_response" | "conflict_detected",
  "payload": { ... },
  "version": "semver",
  "timestamp": "ISO8601",
  "source": "central" | "local"
}
```

### 7. Conflict Detection Strategy

**Decision**: Vector clock + content hash

**Rationale**:
- Vector clock tracks causality
- Content hash detects actual changes
- Can detect true conflicts vs. false positives
- Supports manual resolution workflow

**Implementation**:
- Each config has `version_vector: HashMap<String, u64>`
- Each update increments local counter
- Sync compares vectors to detect concurrent modifications

### 8. API Contract Sharing

**Decision**: OpenAPI 3.0 specification shared between Central and MystiProxy

**Rationale**:
- Single source of truth for API contracts
- Code generation for both server and client
- Documentation auto-generated
- Type-safe clients

## Best Practices Research

### Rust Web Service Architecture

**Layered Architecture**:
```
Handler (HTTP) -> Service (Business Logic) -> Repository (Data Access) -> Database
```

**Key Patterns**:
- Dependency injection via trait objects
- Error handling with `thiserror` for domain errors
- Request validation with `validator` crate
- Structured logging with `tracing`

### Database Migration Strategy

**Decision**: sqlx migrations + versioned schema

**Approach**:
1. Schema version in `schema_migrations` table
2. Migration files in `migrations/` directory
3. Both Central and MystiProxy use same migration files
4. Backward-compatible migrations (additive first, then cleanup)

### Frontend State Management

**Decision**: React Query + Zustand

**Rationale**:
- React Query for server state (caching, refetching)
- Zustand for client state (UI state, filters)
- Minimal boilerplate
- Good TypeScript integration

## Security Considerations

### Authentication Flow

1. Central Management: OAuth2/JWT based
2. MystiProxy Local: API key or JWT
3. Central-Local communication: mTLS or API key

### Data Protection

- Sensitive data (API keys, passwords) encrypted at rest
- TLS 1.3 for all network communication
- Audit logging for all configuration changes

## Performance Optimization

### Caching Strategy

- In-memory cache for hot configurations (LRU)
- Cache invalidation on config update
- ETag-based conditional requests

### Database Optimization

- Index on frequently queried columns (path, method, environment)
- Connection pooling (deadpool-postgres)
- Prepared statements (sqlx)

## Deployment Architecture

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

## Open Questions Resolved

| Question | Resolution |
|----------|------------|
| Frontend framework? | React + TypeScript + Ant Design |
| Central database? | PostgreSQL with SQLx |
| Local database? | SQLite (embedded) |
| Push mechanism? | WebSocket + HTTP fallback |
| Conflict detection? | Vector clock + content hash |
| API contract? | OpenAPI 3.0 shared spec |
