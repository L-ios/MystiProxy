# F1 用户认证与用户管理 — Task 规划

## 任务分解（TDD 顺序）

### T1 迁移与模型
- [ ] 新迁移 `20260814000000_user_last_login.sql`：users 表加 `last_login_at TIMESTAMPTZ`
- [ ] `models/user.rs`：User 加 `last_login_at: Option<DateTime<Utc>>`；加 `UserCreateRequest/UserUpdateRequest/ListResponse` DTO
- 测试：模型 serde 往返（last_login_at None/Some 序列化形态）

### T2 UserRepository（先测后写）
- [ ] 写 `services/user_repository.rs` 的 trait 测试骨架（create/find/update/delete/唯一冲突/count/update_password/update_last_login）
- [ ] 实现 PostgresUserRepository
- 测试：sqlx 测试库（`#[sqlx::test]`）跑全套；唯一约束冲突映射 409

### T3 Bootstrap
- [ ] `services/bootstrap.rs::ensure_admin_user(pool)`：空表创建 admin
- 测试：空库创建/非空跳过/自定义用户名密码

### T4 登录端点
- [ ] `handlers/auth.rs::login/logout`
- 测试：成功登录返回三字段；错密码 401；不存在的用户同样 401（防枚举断言文案一致）

### T5 鉴权中间件接线
- [ ] `middleware/auth.rs` 补全：Bearer 解析 → validate_token → 查库 → CurrentUser 注入
- 测试：无 token 401；坏 token 401；好 token 通过

### T6 Users CRUD
- [ ] `handlers/users.rs`：list/create/me/me-password/get/update/delete
- [ ] RequireRole admin 提取器
- 测试：RBAC 矩阵逐格（viewer 403 等）；防锁死规则（自删 400、自改角色 400）；改密码后旧密码失效

### T7 路由注册 + main 接线
- [ ] routes.rs 挂 public/protected 两层；main.rs 启动时调 `ensure_admin_user`
- 测试：路由可达性冒烟

### T8 验证闭环
- [ ] `cargo test` 全绿；`cargo clippy` 无新告警
- [ ] 覆盖率：`cargo llvm-cov -p mysticentral` ≥70%（新增模块）
- [ ] 真实运行：起 PostgreSQL，实测 spec 的 6 条验收标准逐条 curl
- [ ] 前端 `npm run build` 仍绿

### T9 推送与 CI
- [ ] 提交（约定式 feat(mysticentral): …）
- [ ] push GitHub，盯 `.github/workflows` Actions 至全绿
- [ ] 更新 DEVELOPMENT.md 勾选、OVERVIEW.md 接口清单

## 依赖与风险

- sqlx `#[sqlx::test]` 需要 `migrations` feature 与 DATABASE_URL 测试库（CI 中已有 PostgreSQL service 时直接用；否则回退为跳过标记）
- Argon2 哈希较慢（~100ms），测试批量建用户用低配 Params 缩短时间
- 信心评估：全部组件为库内既有模式，>90%，无需网络调研
