# tasks

## Deps & entities

- [ ] [code] workspace 引入 `argon2`、`tower-sessions`、`tower-sessions-sqlx-store`（或自建 sea-orm store）、`axum-login`、`garde`、`axum-valid`、`rand`、`base64`
- [ ] [code] 新建 entity `user_credentials`（user_id PK, argon2_hash, password_changed_at）
- [ ] [code] 新建 entity `setup_token`（id, token_hash, expires_at, used_at）
- [ ] [code] schema-sync 跑通新增表

## Core auth

- [ ] [code] `swarmhive-server/src/auth/password.rs`：argon2 hash / verify 封装（OWASP 2024 params）
- [ ] [code] `swarmhive-server/src/auth/session.rs`：自建 sea-orm session store
- [ ] [code] `swarmhive-server/src/auth/principal.rs`：Principal 结构 + Scope / AuthMethod 枚举
- [ ] [code] `swarmhive-server/src/auth/permission.rs`：Permission enum + `require_permission!` macro
- [ ] [code] `swarmhive-server/src/auth/service.rs`：AuthService（login / logout / current_principal）
- [ ] [code] `swarmhive-server/src/audit.rs`：`fn write_audit(...)` 包装

## Server wiring

- [ ] [code] axum extractor `Principal`（cookie session 路径；bearer 路径返回 unauthorized 占位）
- [ ] [code] `routes/auth.rs`：POST /auth/login、POST /auth/logout、GET /auth/me
- [ ] [code] `routes/setup.rs`：GET /setup/info（是否需要 setup）、POST /setup（消耗 token + 建 Owner）
- [ ] [code] 启动期：检测 user 表为空 → 生成 setup_token → stdout 打印
- [ ] [code] 全局 `ApiError` 实现 RFC 9457 `IntoResponse`
- [ ] [code] tower-sessions middleware 装配到 `/api/*`
- [ ] [code] 限流：`tower-governor` 在 `/auth/login` 和 `/setup`（per-IP）
- [ ] [code] stub handler `/api/v1/_demo/release-publish` 用于验证 permission 校验（PR 合入后删）

## DTO 校验

- [ ] [code] `LoginReq { email, password }` 用 garde (`email`、`length(min=10)`)
- [ ] [code] `SetupReq { email, display_name, password }` 同上 + `password` 强度规则

## Tests

- [ ] [test] argon2 verify 正确性 + 反向
- [ ] [test] login → cookie → me 走通（集成）
- [ ] [test] 错误密码返回 401 problem+json
- [ ] [test] setup_token 一次性：使用过后再用返回 410
- [ ] [test] require_permission stub：缺 perm 返回 403 problem+json，含 perm
- [ ] [test] AuditLog 写入：login 成功 / 失败、密码变更各 1 条

## Docs

- [ ] [docs] [docs/13-rbac.md](../../../docs/13-rbac.md) 加 "Bootstrap setup token" 子节
- [ ] [docs] [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2 任务勾选
- [ ] [docs] CLAUDE.md 增加 setup token 操作提示
