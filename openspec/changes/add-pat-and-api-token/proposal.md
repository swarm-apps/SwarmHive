# add-pat-and-api-token

## Why

`add-auth-and-rbac` 落地了 Session（Admin SPA 浏览器登录）。docs/13 把"三类凭证"明确成 Session / PAT / API Token：本 proposal 把另外两类长期 Bearer token 加上，解锁 `swarmhive login` CLI 主链路，并让 `Principal` extractor 的 Bearer 分支从占位变成真正实现。

## What

- 新 entity `api_token`（PAT + API Token 共表，靠 `kind` 列区分）：

  ```text
  id, owner_user_id, kind ('pat' | 'api'), name,
  token_hash (blake3, UNIQUE), prefix (前 12 char 明文，便于日志/UI 显示),
  permissions (jsonb<PermissionName[]> NULL —— NULL=live, NOT NULL=snapshot subset),
  last_used_at NULL, expires_at NULL, created_at, revoked_at NULL
  ```

  **不**包含 `scope_app_id` / `scope_channel`（留给 add-app-release-artifact 之后的 update-token-add-app-scope mini proposal——本 proposal 落地时 app/channel 实体还不存在）。

- Token 字符串格式：`swhv_pat_<43char base64url>` / `swhv_api_<43char base64url>`。kind 暴露在前缀里便于日志 grep 与排查；blake3 hash 唯一索引，单次 `WHERE token_hash = $1` 查表。

- `Principal` extractor Bearer 分支：解析 `Authorization: Bearer swhv_(pat|api)_<...>` → blake3 → DB 查 → 检查 `revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())` → 加载 owner user / live-或-snapshot permissions → 节流 update `last_used_at`（1 分钟节流）。

- Server endpoints：
  - `POST /api/v1/auth/cli-token`（公开，governor 跟 login 同档 5 rps / burst 20）：接受 `{ email, password, token_name }` → 校验密码 → 创建 PAT → 返回明文 + name。**专用 endpoint，避开 CLI 还要管 cookie**。
  - `POST /api/v1/tokens`（需要 session/PAT + `token:manage`）：创建 PAT 或 API Token，返回明文一次。API Token 的 `permissions` 必须是 creator 当前权限的子集。
  - `GET /api/v1/tokens`（self 默认；带 `token:manage` 可列他人）：返回 name / kind / prefix / created_at / last_used_at / expires_at / revoked_at。**不**回显明文。
  - `DELETE /api/v1/tokens/:id`（owner 自己或 `token:manage`）：写 `revoked_at = now()`，audit `token_revoked`。

- CLI（`swarmhive-cli`）：
  - `swarmhive login [server]`：交互或 stdin 读 password → POST `/auth/cli-token` → 写 `~/.config/swarmhive/credentials.toml`（0600，仅 owner 读写）。
  - `swarmhive logout`：删本地 + 调 `DELETE /api/v1/tokens/:id`。
  - 后续 publish/promote/rollback 命令优先读 `SWARMHIVE_TOKEN` env，再读本地 toml。

- AuditLog 事件：`token_created` / `token_revoked` / `token_used_first_time`（首次 last_used_at 由 NULL 翻非 NULL 时一次）。

## Acceptance

- Admin 在 `/api/v1/tokens` POST 能创建 PAT；响应明文只显示一次；列表只看 name + prefix + last_used_at。
- CLI `swarmhive login http://localhost:3030` 输入正确密码 → 拿到 PAT → 写本地 toml → `cat ~/.config/swarmhive/credentials.toml` 文件权限是 `0600`。
- 用 PAT 调 `GET /api/v1/auth/me` 返回 owner user；用 revoked 的 token 立即返回 401（无 grace period）。
- API Token 创建时 `permissions` 超出 creator 当前权限 → 422 + problem+json（列出超额的 permission）。
- AuditLog：一次创建写 1 条 `token_created`；一次撤销写 1 条 `token_revoked`；token 首次使用写 1 条 `token_used_first_time`，后续使用不重复写。
- `cargo test --workspace` 全绿；至少 5 个新集成测试覆盖：cli-token happy / wrong password / revoked rejected / api-token超额拒绝 / last_used_at 节流写一次。

## Non-goals

- **不**做 app / channel scope（本 proposal 落地时 app 实体不存在；scope 加到 `add-app-release-artifact` 之后的迁移里）。
- **不**做 device-code flow（CLI MVP 仅 password grant；device-code 等到有正式前端引导页时再加）。
- **不**做 token auto-rotation / auto-extend。
- **不**做按 IP / UA 的额外 scope。
- **不**做 PAT 创建时让用户裁剪权限子集（PAT 永远 = 用户 live 权限；想要"子集"就创建 API Token）。
- **不**改 session 路径（已通过 `add-auth-and-rbac` 验证，本 proposal 只加 Bearer 分支）。

## Depends on

- `add-auth-and-rbac`（Principal、permission service、audit 基础设施）

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) "三类凭证"、"API Token"、"敏感操作"、"审计日志"段
- [docs/12-cli.md](../../../docs/12-cli.md) "认证"、`swarmhive login` 命令
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2（scoped API Token）+ 阶段 5（CLI 本地发布前置）
