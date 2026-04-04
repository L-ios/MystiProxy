# Implementation Plan: HTTP Mock Management System

**Branch**: `001-mock-management` | **Date**: 2026-03-01 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-mock-management/spec.md`

## Summary

实现一个双层架构的 HTTP Mock 管理系统：
- **中心管理系统**：集中式 Web 服务，提供全局配置管理、团队协作、监控分析
- **MystiProxy 本地管理**：每个代理实例的嵌入式管理能力，支持离线操作和双向同步

技术栈：Rust (Axum) + React (TypeScript) + PostgreSQL + SQLite

## Technical Context

**Language/Version**: Rust 1.75+ (Edition 2021), TypeScript 5.x
**Primary Dependencies**: 
- Backend: axum, tokio, sqlx, serde, tracing, tower
- Frontend: React 18, Ant Design 5, React Query, Zustand
**Storage**: PostgreSQL (Central), SQLite (MystiProxy local)
**Testing**: cargo test, pytest (integration), Playwright (E2E)
**Target Platform**: Linux server (Central), Cross-platform (MystiProxy)
**Project Type**: Web service + Embedded module
**Performance Goals**: <100ms API response, 100 concurrent users, 99.9% uptime
**Constraints**: Offline-capable, Cross-platform compatible, Minimal resource usage
**Scale/Scope**: 1000+ mock configs, 100+ instances, 10k requests/day

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| Rust Edition 2021 | ✅ Pass | Using existing project standard |
| Zero-cost abstractions | ✅ Pass | Axum + Tokio async runtime |
| Memory safety | ✅ Pass | No unsafe code planned |
| Error handling | ✅ Pass | thiserror + anyhow pattern |
| Cross-platform | ✅ Pass | Conditional compilation for platform-specific code |
| Test coverage | ⚠️ Partial | Need integration tests for sync protocol |

## Project Structure

### Documentation (this feature)

```text
specs/001-mock-management/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   └── openapi.yaml     # API contract
└── tasks.md             # Phase 2 output (created by /speckit.tasks)
```

### Source Code (repository root)

```text
mysticentral/                    # New crate: Central Management System
├── Cargo.toml
├── src/
│   ├── main.rs                  # Entry point
│   ├── lib.rs                   # Library exports
│   ├── config.rs                # Configuration
│   ├── error.rs                 # Error types
│   ├── db/
│   │   ├── mod.rs
│   │   ├── pool.rs              # Connection pool
│   │   └── migrations/          # SQL migrations
│   ├── models/
│   │   ├── mod.rs
│   │   ├── user.rs
│   │   ├── team.rs
│   │   ├── mock_config.rs
│   │   ├── environment.rs
│   │   ├── instance.rs
│   │   └── sync_record.rs
│   ├── services/
│   │   ├── mod.rs
│   │   ├── mock_service.rs      # Mock CRUD logic
│   │   ├── sync_service.rs      # Sync protocol logic
│   │   ├── conflict_service.rs  # Conflict detection/resolution
│   │   └── analytics_service.rs # Analytics aggregation
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── mocks.rs             # Mock endpoints
│   │   ├── environments.rs      # Environment endpoints
│   │   ├── instances.rs         # Instance management
│   │   ├── sync.rs              # Sync endpoints
│   │   └── analytics.rs         # Analytics endpoints
│   ├── ws/
│   │   ├── mod.rs
│   │   └── sync_ws.rs           # WebSocket sync handler
│   └── auth/
│       ├── mod.rs
│       └── jwt.rs               # JWT authentication
└── tests/
    ├── integration/
    └── fixtures/

mystiproxy-local/                # New module in existing crate
├── src/
│   ├── management/
│   │   ├── mod.rs
│   │   ├── config.rs            # Local management config
│   │   ├── db.rs                # SQLite operations
│   │   ├── sync.rs              # Sync client
│   │   ├── handlers.rs          # Local API handlers
│   │   └── ws_client.rs         # WebSocket client
│   └── ...
└── tests/

frontend/                        # New directory: React frontend
├── package.json
├── tsconfig.json
├── vite.config.ts
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── api/
│   │   ├── client.ts            # API client
│   │   ├── mocks.ts             # Mock API
│   │   └── sync.ts              # Sync API
│   ├── components/
│   │   ├── Layout/
│   │   ├── MockEditor/
│   │   ├── MockList/
│   │   ├── ConflictResolver/
│   │   └── SyncStatus/
│   ├── pages/
│   │   ├── Dashboard/
│   │   ├── Mocks/
│   │   ├── Environments/
│   │   ├── Instances/
│   │   └── Analytics/
│   ├── stores/
│   │   ├── mockStore.ts
│   │   └── uiStore.ts
│   └── types/
│       └── api.ts
└── tests/
    └── e2e/
```

**Structure Decision**: 采用多 crate 结构：
- `mysticentral`: 独立的中心管理系统服务
- `mystiproxy-local`: MystiProxy 内嵌的本地管理模块
- `frontend`: React 前端应用

## Implementation Strategy

### Phase Overview

```
Phase 1: Foundation (Week 1-2)
├── 1.1 Project scaffolding
├── 1.2 Database schema & migrations
├── 1.3 Core data models
└── 1.4 Basic API framework

Phase 2: Core Features (Week 3-4)
├── 2.1 Mock CRUD operations
├── 2.2 Environment management
├── 2.3 Basic frontend
└── 2.4 API documentation

Phase 3: Sync & Distributed (Week 5-6)
├── 3.1 Sync protocol implementation
├── 3.2 WebSocket communication
├── 3.3 Conflict detection
└── 3.4 Conflict resolution UI

Phase 4: Advanced Features (Week 7-8)
├── 4.1 Analytics & monitoring
├── 4.2 Team collaboration
├── 4.3 Import/Export
└── 4.4 Testing & documentation
```

### Code Implementation Approach

#### 1. Framework-First Development

**Principle**: Build the skeleton before filling in details

```
Step 1: Define traits/interfaces
        trait MockRepository { ... }
        trait SyncProtocol { ... }
        
Step 2: Create mock implementations
        struct InMemoryMockRepository { ... }
        
Step 3: Wire up the framework
        Router::new().route("/mocks", ...)
        
Step 4: Implement real logic
        struct PostgresMockRepository { ... }
        
Step 5: Replace mocks with real implementations
```

#### 2. Abstraction Strategy

**When to abstract**:
- Same code appears 3+ times → Extract to function
- Same pattern across modules → Extract to trait
- Cross-cutting concerns → Use middleware/decorator

**Abstraction layers**:

```rust
// Layer 1: Data access (Repository trait)
#[async_trait]
pub trait MockRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>>;
    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>>;
    async fn save(&self, config: &MockConfiguration) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}

// Layer 2: Business logic (Service layer)
pub struct MockService<R: MockRepository> {
    repo: Arc<R>,
    event_bus: EventBus,
}

impl<R: MockRepository> MockService<R> {
    pub async fn create(&self, req: MockCreateRequest) -> Result<MockConfiguration> {
        // Validation, business rules, event publishing
    }
}

// Layer 3: HTTP handlers (Handler layer)
pub async fn create_mock(
    State(service): State<Arc<MockService<PostgresMockRepository>>>,
    Json(req): Json<MockCreateRequest>,
) -> Result<Json<MockConfiguration>, ApiError> {
    let config = service.create(req).await?;
    Ok(Json(config))
}
```

#### 3. Incremental Development Pattern

**Pattern**: Vertical slice first, then horizontal expansion

```
Slice 1: Mock CRUD (end-to-end)
├── Database table
├── Repository implementation
├── Service layer
├── HTTP handlers
├── Frontend page
└── Integration tests

Slice 2: Environment management
├── Same layers, reuse patterns from Slice 1
└── Identify common code → Abstract

Slice 3: Sync protocol
├── New WebSocket layer
├── Reuse existing services
└── Add sync-specific logic
```

#### 4. Testing Strategy

```
Unit Tests (per module)
├── Repository tests with test database
├── Service tests with mock repository
└── Handler tests with TestServer

Integration Tests (per feature)
├── API contract tests
├── Sync protocol tests
└── Conflict resolution tests

E2E Tests (critical paths)
├── Mock creation → Sync → Conflict → Resolution
├── Offline mode → Reconnect → Sync
└── Import → Export → Verify
```

### Detailed Implementation Steps

#### Phase 1: Foundation

**1.1 Project Scaffolding**

```bash
# Create new crate for central management
cargo new --name mysticentral mysticentral

# Add to workspace Cargo.toml
[workspace]
members = ["mysticentral", "mystiproxy", "mystictl"]

# Add dependencies
cd mysticentral
cargo add axum tokio sqlx serde --features ...
```

**1.2 Database Schema**

```sql
-- migrations/001_initial_schema.sql
CREATE TABLE mock_configurations (
    id UUID PRIMARY KEY,
    name VARCHAR(256) NOT NULL,
    path VARCHAR(1024) NOT NULL,
    method VARCHAR(16) NOT NULL,
    matching_rules JSONB NOT NULL,
    response_config JSONB NOT NULL,
    version_vector JSONB NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX idx_mock_path_method ON mock_configurations(path, method);
```

**1.3 Core Data Models**

```rust
// models/mock_config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockConfiguration {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub method: String,
    pub matching_rules: MatchingRules,
    pub response_config: ResponseConfig,
    pub version_vector: VersionVector,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Implement From<SqlRow> for database mapping
impl From<sqlx::postgres::PgRow> for MockConfiguration { ... }
```

**1.4 Basic API Framework**

```rust
// main.rs
#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let db_pool = create_pool(&config.database).await?;
    
    let app = Router::new()
        .route("/api/v1/mocks", get(list_mocks).post(create_mock))
        .route("/api/v1/mocks/:id", get(get_mock).put(update_mock).delete(delete_mock))
        .layer(Extension(db_pool));
    
    axum::Server::bind(&config.server.addr)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}
```

#### Phase 2: Core Features

**2.1 Mock CRUD Operations**

Follow the Repository → Service → Handler pattern:

```rust
// Step 1: Repository trait + implementation
// Step 2: Service with business logic
// Step 3: HTTP handlers
// Step 4: Frontend components
```

**2.2 Frontend Development**

```typescript
// Start with API client
export const mockApi = {
  list: (params: MockFilter) => 
    fetchJSON<MockListResponse>('/api/v1/mocks?' + new URLSearchParams(params)),
  create: (data: MockCreateRequest) =>
    fetchJSON<MockConfiguration>('/api/v1/mocks', { method: 'POST', body: JSON.stringify(data) }),
  // ...
};

// Then React components
export function MockList() {
  const { data } = useQuery(['mocks'], () => mockApi.list({}));
  return <Table dataSource={data?.data} ... />;
}
```

#### Phase 3: Sync & Distributed

**3.1 Sync Protocol Implementation**

```rust
// sync/protocol.rs
pub enum SyncMessage {
    ConfigUpdate { config: MockConfiguration },
    ConfigDelete { id: Uuid },
    SyncRequest { since: DateTime<Utc> },
    SyncResponse { configs: Vec<MockConfiguration> },
    ConflictDetected { local: MockConfiguration, central: MockConfiguration },
}

// WebSocket handler
pub async fn sync_websocket(
    ws: WebSocketUpgrade,
    State(sync_service): State<Arc<SyncService>>,
) -> Response {
    ws.on_upgrade(|socket| handle_sync(socket, sync_service))
}
```

**3.2 Conflict Detection**

```rust
// conflict_service.rs
pub fn detect_conflict(local: &MockConfiguration, central: &MockConfiguration) -> Option<Conflict> {
    // Compare version vectors
    let local_ahead = local.version_vector.dominate(&central.version_vector);
    let central_ahead = central.version_vector.dominate(&local.version_vector);
    
    if local_ahead && central_ahead {
        // Concurrent modification - conflict!
        Some(Conflict { local: local.clone(), central: central.clone() })
    } else {
        None
    }
}
```

#### Phase 4: Advanced Features

**4.1 Analytics**

```rust
// Analytics aggregation with time-series data
pub struct AnalyticsService {
    db: PgPool,
}

impl AnalyticsService {
    pub async fn get_timeseries(&self, params: AnalyticsQuery) -> Result<TimeSeries> {
        sqlx::query_as!(
            AnalyticsPoint,
            r#"
            SELECT 
                date_trunc($1, timestamp) as time,
                COUNT(*) as request_count,
                AVG(response_time_ms) as avg_response_time
            FROM analytics_records
            WHERE mock_id = $2 AND timestamp BETWEEN $3 AND $4
            GROUP BY time
            ORDER BY time
            "#,
            params.interval, params.mock_id, params.start, params.end
        )
        .fetch_all(&self.db)
        .await
    }
}
```

### Refactoring Checkpoints

**After each phase, review for**:

1. **Code duplication**: Extract common patterns
2. **Complexity**: Simplify over-engineered parts
3. **Performance**: Profile and optimize bottlenecks
4. **Test coverage**: Add missing tests

**Refactoring triggers**:

| Signal | Action |
|--------|--------|
| Same code 3+ times | Extract to shared function |
| Function > 50 lines | Split into smaller functions |
| Struct > 10 fields | Consider splitting |
| Deep nesting (>3) | Extract to helper function |
| Many parameters | Use builder pattern or config struct |

## Complexity Tracking

> No constitution violations requiring justification.

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Sync protocol complexity | Start with simple polling, add WebSocket incrementally |
| Cross-platform SQLite | Use sqlx which handles platform differences |
| Frontend scope creep | Use Ant Design components, minimize custom UI |
| Performance at scale | Add caching layer, database indexing |

## Dependencies

### Rust Crates (mysticentral)

```toml
[dependencies]
axum = { version = "0.7", features = ["ws", "macros"] }
axum-extra = { version = "0.9", features = ["typed-header"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "sqlite", "uuid", "chrono", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
jsonwebtoken = "9"
bcrypt = "0.15"
```

### NPM Packages (frontend)

```json
{
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "react-router-dom": "^6.22.0",
    "antd": "^5.14.0",
    "@ant-design/icons": "^5.3.0",
    "@tanstack/react-query": "^5.24.0",
    "zustand": "^4.5.0",
    "axios": "^1.6.0",
    "dayjs": "^1.11.0"
  },
  "devDependencies": {
    "typescript": "^5.3.0",
    "vite": "^5.1.0",
    "@types/react": "^18.2.0",
    "vitest": "^1.3.0",
    "@playwright/test": "^1.41.0"
  }
}
```

## Next Steps

1. Run `/speckit.tasks` to generate detailed task breakdown
2. Begin Phase 1 implementation
3. Set up CI/CD pipeline for new crates
