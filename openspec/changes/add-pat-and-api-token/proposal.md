# add-pat-and-api-token

## Why

docs/13 把"三类凭证"明确成 Session / PAT / API Token：Session 由 `add-auth-and-rbac` 落地；本 proposal 把另外两类长期 token 加上，并给 CLI 提供 `swarmhive login` 的可持久化基础。

## What

- 新 entity `api_token`：

  ```text
  id, owner_user_id, kind (pat | api), name, token_hash (blake3),
  scope_app_id NULL, scope_channel NULL, permissions JSONB,
  expires_at NULL, last_used_at, created_at, revoked_at
  ```

  PAT = `kind=pat, scope=*`；API Token = `kind=api, scope=...`。

- `Principal` extractor 的 Bearer 路径完成实现（前一个 proposal 占位）：解析 `Authorization: Bearer <token>` → blake3 hash → 查表 → 校验未过期未撤销 → 加载 user / permissions → 应用 scope 收窄。

- Server endpoints：
  - `POST /api/v1/tokens`（PAT 或 API Token；require `token:manage`）→ 返回明文一次。
  - `GET /api/v1/tokens`（列）。
  - `DELETE /api/v1/tokens/:id`（撤销，写 audit）。

- CLI（在 `swarmhive-cli` 中实现）：
  - `swarmhive login [server]`：device-code 流程或 password grant，server 端返回 PAT，写本地 `credentials.toml`（0600）。
  - `swarmhive logout`：清本地 + 调 DELETE。
  - 所有发布命令优先读 `SWARMHIVE_TOKEN` env，再读本地 toml。

- token 字符串格式：`swhv_<base64url-32bytes>`（前缀便于日志泄露后快速 grep）。

- 限流：`/api/v1/tokens` POST 走 governor。

## Acceptance

- Admin 能创建 PAT，明文只显示一次；列表只看 token name + last_used_at。
- CLI 能通过 `swarmhive login` 拿到 PAT 并发后续请求。
- 撤销的 token 立即失效（无 grace period）。
- 一个 scoped API Token 拿到错 scope 的 endpoint 返回 403 problem+json（带 required_scope）。
- AuditLog：token_created / token_revoked / token_used_first_time 各 1 条。

## Non-goals

- 不做 token 自动 rotation。
- 不做 device-code 完整 UI（CLI MVP 只跑 password-grant，复用密码登录基础）。
- 不做按 IP / UA 的额外 scope。

## Depends on

- `add-auth-and-rbac`

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) 三类凭证 / API Token / PAT。
- [docs/12-cli.md](../../../docs/12-cli.md) `login` 命令。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2 + 阶段 5。
