# F1 用户认证与用户管理 — Spec（说明文档）

## 概述

为 mysticentral 补齐用户体系：JWT 登录、用户 CRUD、角色权限（RBAC）、首管理员引导。前端登录页与用户管理页由此变为可用。

## 端点规格

### POST /api/v1/auth/login（公开）

请求：
```json
{"username": "admin", "password": "changeme123"}
```

成功 200：
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "user": {"id": "…", "username": "admin", "email": "admin@example.com", "role": "admin"},
  "expires_at": "2026-08-15T15:00:00Z"
}
```

失败 401（用户不存在与密码错误同文案，防枚举）：
```json
{"error": {"code": 401, "message": "Invalid username or password"}}
```

规则：
- 连续失败无速率限制（v1 范围外，后续可加）
- token 为 HS256 JWT，claims: `sub/username/role/exp/iat`，有效期由 `MYSTICENTRAL_JWT_EXPIRATION_HOURS`（默认 24h）控制
- 登录成功更新 `last_login_at`

### POST /api/v1/auth/logout（需 token）

返回 204。JWT 为无状态凭证，服务端不吊销；客户端删除本地 token。端点保留以兼容前端调用与未来黑名单机制。

### GET /api/v1/users（admin/editor）

```json
{"data": [User…], "pagination": {"page":1, "limit":20, "total":N, "total_pages":M}}
```

### POST /api/v1/users（admin）

请求：`{username, email, password, role}`。密码 Argon2id 哈希后存储，明文不出现在日志。用户名/邮箱唯一，冲突 409。

### GET /api/v1/users/me（任意登录用户）

返回当前 token 对应用户（含 last_login_at）。

### PUT /api/v1/users/me/password（任意登录用户）

请求：`{old_password, new_password}`。旧密码验证失败 400。成功后现有 token 仍有效（无状态），建议客户端重新登录。

### GET /api/v1/users/:id（admin/editor）

不存在返回 404。

### PUT /api/v1/users/:id（admin）

可修改 email/role/team。**禁止修改自己的 role**（防最后一个 admin 自降权锁死系统），尝试返回 400。用户名不可改（作为稳定标识）。

### DELETE /api/v1/users/:id（admin）

**禁止删除自己**，返回 400。

## 角色权限矩阵（沿用既有 `auth_service::can`）

| 操作 | viewer | editor | admin |
|------|--------|--------|-------|
| 登录 / users/me / 改自己密码 | ✓ | ✓ | ✓ |
| 查看 users 列表/详情 | ✗ | ✓ | ✓ |
| 创建/修改/删除用户 | ✗ | ✗ | ✓ |

## 首管理员引导

启动时若 users 表为空：
- 用户名 = `MYSTICENTRAL_ADMIN_USERNAME`（默认 `admin`）
- 密码 = `MYSTICENTRAL_ADMIN_PASSWORD`（默认 `changeme123`）
- 邮箱 = `admin@mysticentral.local`，role=admin

使用默认密码时输出 WARN 日志提示修改。表非空则跳过（幂等）。

## 安全基线

- 密码：Argon2id（库内既有实现，OWASP 推荐）
- 传输：生产环境应置于 TLS 之后（mysticentral 自身支持 legacy-tls）
- JWT secret：启动强制要求（既有行为），<32 字符告警（既有行为）
- 登录失败不泄露用户是否存在

## 环境变量汇总

| 变量 | 默认 | 说明 |
|------|------|------|
| `MYSTICENTRAL_ADMIN_USERNAME` | admin | 首管理员用户名 |
| `MYSTICENTRAL_ADMIN_PASSWORD` | changeme123 | 首管理员密码（仅空表时使用） |

## 验收标准

1. 实测 `POST /auth/login` 正确/错误凭据分别 200/401
2. 无 token 访问 `/users` → 401；viewer token → 403；admin token → 200
3. admin 可创建用户，新用户可登录
4. 改密码后旧密码登录失败
5. 不能删自己/不能改自己角色
6. `cargo test` 全绿，新增代码覆盖率 ≥70%
