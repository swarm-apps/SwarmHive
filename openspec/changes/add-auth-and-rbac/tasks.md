# tasks

## Deps & entities

- [x] [code] workspace 引入 `argon2`、`tower-sessions`、`tower-governor`、`rand`、`base64`（不引 `tower-sessions-sqlx-store` —— 自建 sea-orm store；不引 `axum-login` —— design.md 的 Principal extractor 手写 FromRequestParts；不引 `axum-valid` —— 极简自写 garde extractor）
- [x] [code] 新建 entity `user_credentials`（user_id PK, argon2_hash, password_changed_at, created_at, updated_at）
- [x] [code] 新建 entity `setup_token`（id, token_hash UNIQUE, expires_at, used_at, created_at）
- [x] [code] schema-sync 跑通新增表（由 `REGISTRY_GLOB = "swarmhive_entity::*"` 自动覆盖；group 5 集成测试验证）

## Core auth

- [x] [code] `swarmhive-server/src/auth/password.rs`：argon2id hash / verify 封装（OWASP 2024 params: m=19456, t=2, p=1）+ 内置 timing-equalising dummy verify path
- [x] [code] `swarmhive-server/src/auth/session.rs`：`SeaOrmStore` 实现 `tower_sessions::SessionStore`（i128 ↔ Uuid bijection + `data["user_id"]` denormalisation）。**附带**：演进 `session` entity schema（`user_id` → `Option<Uuid>`，新增 `data: Json` 列）
- [x] [code] `swarmhive-server/src/auth/principal.rs`：`Principal { user_id, org_id, scope, permissions, auth_method }` + `Scope::{None, App(Uuid)}` + `AuthMethod::{Session, Pat, ApiToken}`（PAT/ApiToken 变体留给 add-pat-and-api-token）
- [x] [code] `swarmhive-server/src/auth/permission.rs`：`require_permission!` 宏 + `check(principal, perm, scope)` 函数 + `scope_covers` 语义（None 覆盖一切；App(a) 仅覆盖 App(a)）。**复用** `swarmhive_api_types::PermissionName` 闭集，不在 server 重定义
- [x] [code] `swarmhive-server/src/auth/service.rs`：`AuthService` —— `login` / `logout` / `load_principal` / `register_owner` / `issue_setup_token` / `setup_required` + `RequestCtx { ip, user_agent }` + `USER_ID_KEY`/`SESSION_TTL` 常量
- [x] [code] `swarmhive-server/src/services/audit.rs`：`AuditEntry` + `write(db, entry)`（放 `services/` 而非顶层 `audit.rs`，与 `services/seed.rs` 一致）。**附带**：扩展 `ApiError` 加 `Conflict` (409) / `Gone` (410) 变体

## Server wiring

- [x] [code] axum extractor `impl FromRequestParts<AppState> for Principal`（cookie session 路径；`Authorization: Bearer …` 头存在时短路返回 Unauthorized，等 `add-pat-and-api-token` 落地）
- [x] [code] `routes/auth.rs`：POST /api/v1/auth/login、POST /api/v1/auth/logout、GET /api/v1/auth/me（返回 `{ user, permissions[] }`）
- [x] [code] `routes/setup.rs`：GET /api/v1/setup/info、POST /api/v1/setup
- [x] [code] 启动期 `maybe_issue_setup_token`：user 表为空时颁发 token + stdout banner（ASCII 框出来便于 docker logs grep）
- [x] [code] 全局 `ApiError` 实现 RFC 9457 `IntoResponse`（add-persistence-foundation 已实现；group 2 扩展 Conflict/Gone 变体）
- [x] [code] `SessionManagerLayer` 装到 root router（health/version 路径不会触发 session 物化，session 是 lazy 的；与 design "装到 /api/\*" 等价）
- [x] [code] `tower_governor::GovernorLayer` + `SmartIpKeyExtractor` 挂在 auth + setup 子 router（per_second=5, burst=20）。bin/server.rs 加 `.into_make_service_with_connect_info::<SocketAddr>()` 让 ConnectInfo 可用
- [x] [code] stub handler `POST /api/v1/_demo/release-publish` 用 `require_permission!(p, PermissionName::ReleasePublish, Scope::None)` 校验

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
