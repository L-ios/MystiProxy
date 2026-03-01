# MystiProxy Local Management Module

本地管理模块为 MystiProxy 提供了基于 SQLite 的嵌入式 Mock 配置管理功能，支持离线操作和与中央管理系统的同步。

## 功能特性

### 核心功能

- **SQLite 数据库**: 嵌入式数据库，无需外部依赖
- **REST API**: 完整的 HTTP API，与中央管理系统兼容
- **配置导入**: 支持 YAML/JSON 配置文件导入
- **版本控制**: 使用向量时钟进行冲突检测
- **内容哈希**: SHA-256 哈希用于增量同步

### 同步功能

- **双向同步**: 与中央管理系统的 Pull/Push 同步
- **离线队列**: 断网时缓存变更，联网后自动同步
- **指数退避**: 智能重试机制
- **冲突检测**: 基于向量时钟的冲突检测

## 架构设计

```text
┌─────────────────────────────────────────────────────────────┐
│                    Management Module                         │
├─────────────────────────────────────────────────────────────┤
│  handlers.rs    - HTTP API handlers (Axum compatible)       │
│  repository.rs  - MockRepository trait & SQLite impl        │
│  db.rs          - SQLite connection & migrations            │
│  config.rs      - Configuration management                  │
│  import.rs      - YAML/JSON config file import              │
│  models.rs      - Core data structures                      │
│  sync.rs        - Synchronization client                    │
│  integration.rs - MystiProxy integration                   │
│  error.rs       - Error types                               │
└─────────────────────────────────────────────────────────────┘
```

## 使用方法

### 1. 基本使用

```rust
use http_proxy::management::{LocalManagement, LocalManagementBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建配置
    let config = LocalManagementBuilder::new()
        .enabled(true)
        .db_path("/path/to/mystiproxy.db")
        .listen_addr("127.0.0.1:9090")
        .build();
    
    // 初始化
    let mgmt = LocalManagement::init(config).await?;
    
    // 获取 repository
    let repo = mgmt.repository();
    
    // 创建 Mock 配置
    let request = CreateMockRequest {
        name: "Test API".to_string(),
        path: "/api/test".to_string(),
        method: HttpMethod::Get,
        ..Default::default()
    };
    
    let config = repo.create(request).await?;
    
    Ok(())
}
```

### 2. 启用同步

```rust
use uuid::Uuid;

let instance_id = Uuid::new_v4();
let config = LocalManagementBuilder::new()
    .enabled(true)
    .db_path("/path/to/mystiproxy.db")
    .with_sync("http://central.example.com", instance_id)
    .sync_interval(60)  // 每 60 秒同步一次
    .api_key("your-api-key")
    .offline_queue(true)
    .max_queue_size(1000)
    .build();

let mgmt = LocalManagement::init(config).await?;
```

### 3. 创建 API 服务

```rust
// 创建 Axum router
let router = mgmt.create_router();

// 启动 HTTP 服务器
let addr = mgmt.config().api.listen_addr.parse()?;
axum::Server::bind(&addr)
    .serve(router.into_make_service())
    .await?;
```

## API 端点

### Mock 配置管理

- `GET /api/v1/mocks` - 列出所有 Mock 配置
- `GET /api/v1/mocks/:id` - 获取单个 Mock 配置
- `POST /api/v1/mocks` - 创建 Mock 配置
- `PUT /api/v1/mocks/:id` - 更新 Mock 配置
- `DELETE /api/v1/mocks/:id` - 删除 Mock 配置

### 同步管理

- `GET /api/v1/sync/status` - 获取同步状态
- `POST /api/v1/sync/trigger` - 手动触发同步

### 健康检查

- `GET /api/v1/health` - 健康检查端点

## 配置文件

### YAML 格式

```yaml
enabled: true
db_path: /path/to/mystiproxy.db

sync:
  enabled: true
  central_url: http://central.example.com
  instance_id: 00000000-0000-0000-0000-000000000001
  sync_interval_secs: 60
  api_key: your-api-key
  offline_queue_enabled: true
  max_queue_size: 1000

api:
  listen_addr: 127.0.0.1:9090
  auth_enabled: false
```

### JSON 格式

```json
{
  "enabled": true,
  "db_path": "/path/to/mystiproxy.db",
  "sync": {
    "enabled": true,
    "central_url": "http://central.example.com",
    "instance_id": "00000000-0000-0000-0000-000000000001",
    "sync_interval_secs": 60,
    "api_key": "your-api-key",
    "offline_queue_enabled": true,
    "max_queue_size": 1000
  },
  "api": {
    "listen_addr": "127.0.0.1:9090",
    "auth_enabled": false
  }
}
```

## 数据模型

### MockConfiguration

```rust
pub struct MockConfiguration {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub method: HttpMethod,
    pub matching_rules: MatchingRules,
    pub response_config: ResponseConfig,
    pub source: MockSource,
    pub version_vector: VersionVector,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_active: bool,
}
```

### VersionVector

用于冲突检测的向量时钟实现：

```rust
let mut v1 = VersionVector::new();
v1.increment(instance1);

let mut v2 = VersionVector::new();
v2.increment(instance1);
v2.increment(instance1);

assert!(v2.dominates(&v1));  // v2 优于 v1
```

## 同步机制

### Pull 同步

1. 发送本地 checksums 和 last_sync 时间戳
2. 接收变更的配置和已删除的 ID 列表
3. 应用变更到本地数据库
4. 更新 last_sync 时间戳

### Push 同步

1. 发送操作类型（Create/Update/Delete）和配置
2. 中央系统验证版本向量
3. 如果冲突返回 409 状态码
4. 如果成功返回 200 状态码

### 离线队列

当网络断开时，所有变更操作会被缓存到离线队列：

```rust
pub struct OfflineQueueEntry {
    pub id: i64,
    pub operation_type: SyncOperation,
    pub config_id: Option<Uuid>,
    pub payload: String,
    pub created_at: DateTime<Utc>,
    pub retry_count: u32,
    pub last_error: Option<String>,
}
```

### 重试策略

使用指数退避算法：

```rust
let policy = RetryPolicy {
    max_retries: 5,
    initial_delay_ms: 1000,
    max_delay_ms: 60000,
    multiplier: 2.0,
};

// 延迟序列: 1s, 2s, 4s, 8s, 16s (最大 60s)
```

## 编译和测试

### 编译

```bash
cargo build -p http_proxy --features local-management
```

### 运行测试

```bash
cargo test -p http_proxy --features local-management
```

### 运行示例

```bash
cargo run --example local_management_example --features local-management
```

## 依赖项

- `sqlx`: SQLite 数据库
- `tokio`: 异步运行时
- `axum`: HTTP 框架
- `reqwest`: HTTP 客户端
- `serde`: 序列化
- `uuid`: UUID 生成
- `chrono`: 时间处理
- `sha2`: SHA-256 哈希

## 许可证

MIT License
