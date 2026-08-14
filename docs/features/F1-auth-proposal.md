# F1 用户认证与用户管理 — Proposal

## 需求深度理解

### 背景与问题

前端（`frontend/src/api/users.ts`、`LoginPage.tsx`、`UsersPage.tsx`）已按目标契约完整实现了登录页与用户管理页，但 mysticentral 后端**没有任何对应路由**（实测全部 404）。这导致：

1. 用户无法登录系统（LoginPage 表单提交到不存在的 `POST /auth/login`）
2. 登录后所有页面因 401 拦截器跳回登录页，形成死循环
3. 团队协作场景（多用户、角色权限）完全不可用

### 已有资产（关键发现）

实现此功能不需要从零开始，代码库中已有 90% 的支撑设施：

| 资产 | 位置 | 状态 |
|------|------|------|
| `users` 表结构 | `mysticentral/src/db/migrations/20260301000000_initial_schema.sql` | ✅ 已建表（含 role、password_hash、唯一索引） |
| `User`/`UserRole`/`UserInfo`/`LoginResponse` 模型 | `mysticentral/src/models/user.rs` | ✅ 完整定义 |
| JWT 签发/验证（jsonwebtoken） | `auth_service.rs::generate_token/validate_token` | ✅ 完整实现 |
| Argon2 密码哈希（OWASP 推荐） | `auth_service.rs::hash_password/verify_password` | ✅ 完整实现 |
| RBAC 权限矩阵 | `auth_service.rs::can/can_modify` | ✅ 完整实现 |
| JWT 鉴权中间件骨架 | `mysticentral/src/middleware/auth.rs` | ✅ 已存在 |

**缺口仅是**：用户仓储（PostgreSQL CRUD）、HTTP handlers、路由注册、登录端点。这就是"前端先行开发"模式的典型后端补齐工作。

### 用户价值

- 登录页可用，前端整个控制台从"不可用"变为"可进入"
- 多用户 RBAC（admin/editor/viewer）支撑团队协作
- 密码用 Argon2id 哈希存储，JWT 有过期时间，符合安全基线

### 成功标准

1. `POST /auth/login` 正确凭据返回 `{token, user, expires_at}`，错误凭据返回 401
2. `/users` CRUD 全部按前端契约工作（含 `/users/me`、`/users/me/password`）
3. 管理接口受 JWT 保护（无 token 返回 401）
4. 首次启动自动创建 admin 用户（否则无人能创建第一个用户，鸡生蛋问题）
5. 覆盖率 ≥70%

### 范围边界

**做**：users 仓储、auth 登录 handlers、users CRUD handlers、JWT 中间件接线、admin bootstrap、路由注册、完整测试。
**不做**：teams 管理（表已建但前端无页面）、刷新 token、密码找回、审计日志。

## 技术方案选型（信心评估）

- JWT + Argon2：沿用现有 `auth_service.rs`，**信心 95%**（代码已在库中且经过审查）
- 首管理员 bootstrap：业界标准做法是环境变量注入初始密码（与 PostgreSQL `POSTGRES_PASSWORD` 模式一致），**信心 90%**
- axum middleware 提取 JWT：项目已有 `middleware/auth.rs` 骨架，**信心 92%**
- 无需网络调研：所有组件均为库内成熟模式。
