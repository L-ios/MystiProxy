# Task Breakdown: HTTP Mock Management System

**Branch**: `001-mock-management` | **Date**: 2026-03-01
**Plan**: [plan.md](./plan.md) | **Spec**: [spec.md](./spec.md)

## Task Execution Principles

> **核心原则：先做代码的核心骨架，再做细节实现**

1. **Framework-First**: 先定义 traits/interfaces，再实现具体逻辑
2. **Vertical Slice**: 每个任务应该是端到端可验证的
3. **Core Before Details**: 核心流程优先，边缘情况后补
4. **Abstract After Repeat**: 代码重复 3 次后再抽象

---

## Phase 1: Foundation (核心骨架)

### 1.1 项目脚手架 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-001 | 创建 `mysticentral` crate 基础结构 | P0 | - | `cargo build -p mysticentral` 成功 |
| T-002 | 配置 workspace Cargo.toml 依赖 | P0 | T-001 | 所有依赖版本锁定，编译通过 |
| T-003 | 创建基础配置模块 `config.rs` | P0 | T-002 | 支持从环境变量/YAML加载配置 |
| T-004 | 定义统一错误类型 `error.rs` | P0 | T-002 | 使用 thiserror 定义 ApiError |
| T-005 | 创建 `lib.rs` 导出公共模块 | P0 | T-003, T-004 | 模块结构清晰可见 |

### 1.2 数据库核心 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-006 | 设计 PostgreSQL 数据库 schema | P0 | T-001 | SQL migration 文件就绪 |
| T-007 | 创建数据库连接池模块 `db/pool.rs` | P0 | T-006 | PgPool 初始化成功 |
| T-008 | 编写初始 migration 脚本 | P0 | T-006 | `sqlx migrate run` 成功 |
| T-009 | 定义 Repository trait (核心抽象) | P0 | T-007 | MockRepository trait 定义完成 |

```rust
// T-009 产出: 核心抽象
#[async_trait]
pub trait MockRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>>;
    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>>;
    async fn save(&self, config: &MockConfiguration) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}
```

### 1.3 核心数据模型 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-010 | 定义 `MockConfiguration` 模型 | P0 | T-006 | 包含所有字段，实现 Serialize/Deserialize |
| T-011 | 定义 `MatchingRules` 和 `ResponseConfig` | P0 | T-010 | JSON 序列化正确 |
| T-012 | 定义 `VersionVector` 冲突检测结构 | P0 | T-010 | 支持向量时钟比较 |
| T-013 | 定义 `Environment` 和 `MystiProxyInstance` 模型 | P1 | T-010 | 基础字段完整 |
| T-014 | 实现 `content_hash` 计算方法 | P1 | T-010 | SHA-256 hash 正确 |

### 1.4 API 框架骨架 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-015 | 创建 Axum Router 骨架 | P0 | T-005 | 服务启动，健康检查通过 |
| T-016 | 定义 API 路由结构 (空处理器) | P0 | T-015 | 所有路由注册，返回 501 |
| T-017 | 实现 CORS 和中间件配置 | P1 | T-015 | 跨域请求正常 |
| T-018 | 创建 InMemoryMockRepository (测试用) | P0 | T-009 | 单元测试通过 |

```rust
// T-016 产出: API 骨架
let app = Router::new()
    .route("/api/v1/mocks", get(list_mocks).post(create_mock))
    .route("/api/v1/mocks/:id", get(get_mock).put(update_mock).delete(delete_mock))
    .route("/api/v1/environments", get(list_environments).post(create_environment))
    .route("/api/v1/instances", get(list_instances))
    .route("/api/v1/sync/pull", post(sync_pull))
    .route("/api/v1/sync/push", post(sync_push));
```

---

## Phase 2: Core Features (核心功能)

### 2.1 Mock CRUD 核心流程 [CORE - 端到端]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-019 | 实现 PostgresMockRepository | P0 | T-009, T-008 | 数据库 CRUD 操作成功 |
| T-020 | 创建 MockService 业务逻辑层 | P0 | T-019 | 包含验证、版本向量更新 |
| T-021 | 实现 `create_mock` handler | P0 | T-020 | POST /api/v1/mocks 返回 201 |
| T-022 | 实现 `list_mocks` handler | P0 | T-020 | GET /api/v1/mocks 返回列表 |
| T-023 | 实现 `get_mock` handler | P0 | T-020 | GET /api/v1/mocks/:id 返回详情 |
| T-024 | 实现 `update_mock` handler | P0 | T-020 | PUT /api/v1/mocks/:id 更新成功 |
| T-025 | 实现 `delete_mock` handler | P0 | T-020 | DELETE /api/v1/mocks/:id 删除成功 |
| T-026 | 编写 Mock CRUD 集成测试 | P0 | T-021-T-025 | 所有测试通过 |

### 2.2 环境管理 [细节]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-027 | 定义 EnvironmentRepository trait | P1 | T-009 | trait 定义完成 |
| T-028 | 实现 PostgresEnvironmentRepository | P1 | T-027 | 数据库操作成功 |
| T-029 | 创建 EnvironmentService | P1 | T-028 | 业务逻辑完整 |
| T-030 | 实现环境 CRUD handlers | P1 | T-029 | API 端点工作正常 |

### 2.3 前端核心 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-031 | 初始化 React + Vite 项目 | P0 | - | `npm run dev` 启动成功 |
| T-032 | 配置 Ant Design 和主题 | P1 | T-031 | 组件样式正确 |
| T-033 | 创建 API 客户端 `api/client.ts` | P0 | T-031 | 封装 fetch，支持错误处理 |
| T-034 | 创建 Mock API hooks `api/mocks.ts` | P0 | T-033 | useQuery/useMutation 封装 |
| T-035 | 创建 MockList 页面组件 | P0 | T-034 | 显示 mock 列表 |
| T-036 | 创建 MockEditor 表单组件 | P0 | T-034 | 创建/编辑 mock |
| T-037 | 实现基础布局和路由 | P1 | T-035 | 页面导航正常 |

---

## Phase 3: Sync & Distributed (同步核心)

### 3.1 同步协议核心 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-038 | 定义 SyncMessage 协议枚举 | P0 | T-010 | 消息类型完整 |
| T-039 | 实现 VersionVector 比较算法 | P0 | T-012 | 冲突检测正确 |
| T-040 | 创建 SyncService 核心逻辑 | P0 | T-038, T-039 | pull/push 逻辑完整 |
| T-041 | 实现 `sync_pull` handler | P0 | T-040 | 增量同步返回正确数据 |
| T-042 | 实现 `sync_push` handler | P0 | T-040 | 接收并处理本地变更 |

```rust
// T-038 产出: 同步协议
pub enum SyncMessage {
    ConfigUpdate { config: MockConfiguration },
    ConfigDelete { id: Uuid },
    SyncRequest { since: DateTime<Utc>, checksums: HashMap<Uuid, String> },
    SyncResponse { configs: Vec<MockConfiguration>, deleted: Vec<Uuid> },
    ConflictDetected { config_id: Uuid, local: MockConfiguration, central: MockConfiguration },
}
```

### 3.2 WebSocket 实时推送 [细节]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-043 | 实现 WebSocket 升级处理器 | P1 | T-040 | WS 连接建立成功 |
| T-044 | 实现配置变更广播 | P1 | T-043 | 变更推送到所有连接实例 |
| T-045 | 实现心跳和重连机制 | P1 | T-043 | 断线重连正常 |

### 3.3 冲突检测与解决 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-046 | 实现 ConflictService 检测逻辑 | P0 | T-039 | 并发修改检测正确 |
| T-047 | 创建冲突存储和查询 | P0 | T-046 | 冲突记录持久化 |
| T-048 | 实现 `resolve_conflict` handler | P0 | T-046 | 支持三种解决策略 |
| T-049 | 前端冲突解决 UI 组件 | P1 | T-048 | Diff 显示和选择交互 |

### 3.4 MystiProxy 本地管理 [CORE]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-050 | 创建 SQLite 数据库模块 | P0 | - | 嵌入式数据库初始化 |
| T-051 | 实现 LocalMockRepository (SQLite) | P0 | T-050 | 本地 CRUD 成功 |
| T-052 | 创建本地管理配置模块 | P0 | T-050 | 配置加载正确 |
| T-053 | 实现本地 API handlers | P0 | T-051 | 与 Central API 兼容 |
| T-054 | 实现配置文件到数据库加载 | P0 | T-051 | YAML/JSON 导入成功 |
| T-055 | 实现同步客户端 `sync.rs` | P0 | T-041, T-042 | pull/push 调用成功 |
| T-056 | 实现离线队列和重试 | P1 | T-055 | 断网缓存，联网同步 |
| T-057 | 集成本地管理到 MystiProxy | P0 | T-053, T-055 | feature flag 启用 |

---

## Phase 4: Advanced Features (高级功能)

### 4.1 分析监控 [细节]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-058 | 创建 AnalyticsRecord 模型 | P2 | T-006 | 数据结构完整 |
| T-059 | 实现 AnalyticsService | P2 | T-058 | 时间序列查询正确 |
| T-060 | 实现分析 API handlers | P2 | T-059 | 返回聚合数据 |
| T-061 | 前端分析仪表盘页面 | P2 | T-060 | 图表展示正确 |

### 4.2 团队协作 [细节]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-062 | 实现 User 和 Team 模型 | P2 | T-006 | 用户/团队数据结构 |
| T-063 | 实现 JWT 认证中间件 | P2 | T-062 | Token 验证正确 |
| T-064 | 实现 RBAC 权限检查 | P2 | T-063 | 角色权限控制 |
| T-065 | 前端登录和用户管理页面 | P2 | T-063 | 认证流程完整 |

### 4.3 导入导出 [细节]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-066 | 实现 YAML/JSON 导出功能 | P2 | T-020 | 导出文件格式正确 |
| T-067 | 实现 YAML/JSON 导入功能 | P2 | T-020 | 导入解析正确 |
| T-068 | 前端导入导出 UI | P2 | T-066, T-067 | 文件上传下载正常 |

### 4.4 测试与文档 [收尾]

| ID | Task | Priority | Dependencies | Acceptance Criteria |
|----|------|----------|--------------|---------------------|
| T-069 | 编写 E2E 测试 (Playwright) | P1 | T-057 | 关键路径测试通过 |
| T-070 | 生成 OpenAPI 文档 | P1 | T-026 | Swagger UI 可访问 |
| T-071 | 编写部署文档 | P2 | T-057 | Docker 部署指南 |
| T-072 | 性能测试和优化 | P2 | T-057 | 满足性能指标 |

---

## Task Dependencies Graph

```
Phase 1 (Foundation)
T-001 ─→ T-002 ─→ T-003 ─→ T-005 ─→ T-015 ─→ T-016
    │        └─→ T-004 ────────┘
    └─→ T-006 ─→ T-007 ─→ T-009 ─→ T-018
              └─→ T-008
              └─→ T-010 ─→ T-011
                       └─→ T-012
                       └─→ T-013 ─→ T-014

Phase 2 (Core Features)
T-009 + T-008 ─→ T-019 ─→ T-020 ─→ T-021~T-025 ─→ T-026
                                    ↓
T-031 ─→ T-033 ─→ T-034 ─→ T-035 ─→ T-036
    └─→ T-032            └─→ T-037

Phase 3 (Sync)
T-010 + T-012 ─→ T-038 ─→ T-040 ─→ T-041, T-042
            └─→ T-039 ────────┘
T-040 ─→ T-046 ─→ T-047 ─→ T-048 ─→ T-049
T-050 ─→ T-051 ─→ T-053 ─→ T-057
    └─→ T-052       ↑
                    └─→ T-055 ←─ T-041, T-042
                           └─→ T-056

Phase 4 (Advanced)
T-020 ─→ T-058 ─→ T-059 ─→ T-060 ─→ T-061
T-062 ─→ T-063 ─→ T-064 ─→ T-065
T-020 ─→ T-066, T-067 ─→ T-068
T-057 ─→ T-069, T-070, T-071, T-072
```

---

## Progress Tracking

| Phase | Total Tasks | Completed | In Progress | Blocked |
|-------|-------------|-----------|-------------|---------|
| Phase 1: Foundation | 18 | 0 | 0 | 0 |
| Phase 2: Core Features | 17 | 0 | 0 | 0 |
| Phase 3: Sync & Distributed | 20 | 0 | 0 | 0 |
| Phase 4: Advanced | 15 | 0 | 0 | 0 |
| **Total** | **70** | **0** | **0** | **0** |

---

## Critical Path

```
T-001 → T-006 → T-009 → T-019 → T-020 → T-021 → T-026
                                     ↓
                          T-038 → T-040 → T-041
                                     ↓
                          T-050 → T-051 → T-053 → T-057
```

**关键路径任务 (P0)**:
- T-001, T-006, T-009, T-019, T-020, T-021, T-026 (Mock CRUD)
- T-038, T-040, T-041 (Sync Protocol)
- T-050, T-051, T-053, T-057 (Local Management)

---

## First Sprint Tasks (Week 1)

**目标**: 完成核心骨架，实现第一个端到端流程

| Day | Tasks | Deliverable |
|-----|-------|-------------|
| Day 1-2 | T-001 ~ T-009 | 项目骨架 + Repository trait |
| Day 3-4 | T-010 ~ T-018 | 数据模型 + API 框架 |
| Day 5 | T-019 ~ T-021 | 第一个 API: POST /mocks |

**验收标准**: 
```bash
# Day 5 结束时能够运行:
curl -X POST http://localhost:8080/api/v1/mocks \
  -H "Content-Type: application/json" \
  -d '{"name": "test", "path": "/test", "method": "GET", ...}'
# 返回 201 Created
```
