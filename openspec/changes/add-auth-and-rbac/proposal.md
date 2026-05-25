# add-auth-and-rbac

## Why

docs/13 要求 MVP 即具备单组织 RBAC + scoped credential。所有后续 handler 都要走 permission check 与审计；如果不先把基础设施落下来，后续每个 proposal 都会重复实现"如何拿到当前 user / 校验权限 / 写 audit log"。

## What

### 1. 密码登录

- 用户表已在 `add-persistence-foundation` 落好；本 proposal 新增 `user_credentials` 表（`user_id`、`argon2_hash`、`last_password_changed_at`）。
- `POST /api/v1/auth/login`（email + password） → 校验 → 建 session → 写 cookie。
- `POST /api/v1/auth/logout` → 失效 session。
- `GET /api/v1/auth/me` → 返回当前 user + 角色 + permission。

### 2. Session

- `tower-sessions` + 自建 sea-orm session-store（用 `add-persistence-foundation` 已建好的 `session` 表）。
- cookie: `swarmhive_session`，HttpOnly、SameSite=Lax、`Secure`（prod）。
- session 默认 14 天 sliding，可在 config 调。
- `axum-login` 包装 backend trait。

### 3. Principal extractor

server crate 提供一个统一的 axum extractor：

```rust
pub struct Principal {
    pub user_id: Uuid,
    pub org_id: Uuid,
    pub scope: Scope,                    // None | App(app_id) | Channel(app_id, channel)
    pub permissions: HashSet<Permission>,
    pub auth_method: AuthMethod,         // Session | Pat | ApiToken（本 proposal 只接 Session）
}
```

Extractor 顺序解析：cookie session → bearer token（留 hook，但 token 实现拆到 `add-pat-and-api-token`）。

### 4. Permission middleware

- `require_permission!(perm)` macro 或 `RequirePermission(perm)` extractor 两选一（design.md 再定）。
- 失败返回 RFC 9457 `403 forbidden`。

### 5. AuditLog 写入

- 凡 permission check 通过的"敏感操作"（按 docs/13 列出）写一条 AuditLog。
- 写入函数包在 service 层（不在 handler 里散写）。

### 6. Bootstrap Owner

- 首次启动若 user 表为空：从配置 `[bootstrap] owner_email`（必填）+ 一次性 setup token 流程登记初始 Owner。
- 一次性 token 在首次 server 启动时 stdout 打印（类似 Vaultwarden / Authelia）。

## Acceptance

- 能通过 setup token 完成 Owner 初始化。
- 能用 Owner 账号登录拿到 cookie。
- `GET /api/v1/auth/me` 返回正确 permission 集合。
- 一个需要 `release:publish` 的 stub handler，能正确放行 / 拦截。
- 失败鉴权返回 problem+json。
- 关键操作（登录成功 / 失败、密码变更）写入 AuditLog。
- 集成测试：注册 → 登录 → 拿 me → 调一个 permission-gated stub。

## Non-goals

- 不实现 OAuth（拆到 `add-oauth-github`）。
- 不实现 PAT / API Token 鉴权（拆到 `add-pat-and-api-token`）。
- 不做"邀请用户"链路（依赖邮件，拆到 `add-mail-infrastructure`）。
- 不做密码重置（同上）。
- 不做密码强度策略 UI；只做 server 端最小校验（≥10 chars）。

## Depends on

- `add-toolchain-bump`
- `add-persistence-foundation`

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) 全文，重点 "Identity Providers"、"三类凭证"、"Permission 列表"、"审计日志"。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2。
