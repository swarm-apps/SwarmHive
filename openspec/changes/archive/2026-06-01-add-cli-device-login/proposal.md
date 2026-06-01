# add-cli-device-login

## Why

当前 CLI 登录是 OAuth 里被废弃的 **ROPC（Resource Owner Password Credentials）** 反模式：[`swarmhive login`](../../../crates/swarmhive-cli/src/commands/login.rs) 直接收集明文 email+password → `POST /api/v1/auth/cli-token`（[`routes/auth.rs::cli_token`](../../../crates/swarmhive-server/src/routes/auth.rs)）→ server `verify_password` 后铸一个永不过期的 PAT。三个实际问题：

1. **客户端经手明文密码**——CLI 漏洞 / fork / shell history 泄露的是主密码，而非可撤销 token。
2. **与 OAuth 直接对撞**——`add-oauth-github-and-provider-config` 落地后，只用 GitHub 登录、没设密码的用户 `swarmhive login` 时无密码可填，被锁死在 CLI 之外。
3. **与未来 MFA 互斥**——给 owner 加 TOTP/2FA 后，密码授权无法做 challenge。

主流 CLI（`gh` / `gcloud` / `vercel` / `stripe`）一律把认证**委托给浏览器**。本 proposal 用 **RFC 8628 OAuth 2.0 Device Authorization Grant**（`gh` 风格）替换 ROPC：CLI 拿一个 `user_code`，用户在浏览器里登录 SwarmHive 自己的 Web 页并批准，CLI 轮询换取 PAT。认证步骤复用 `/login`——于是 OAuth-only 用户**自动获得 CLI 能力**，CLI 一行 GitHub 代码都不用写。

详见 [dev-notes/explore-summaries/2026-06-01-cli-auth-standardization.md](../../../dev-notes/explore-summaries/2026-06-01-cli-auth-standardization.md)（与本 proposal 同批产出的探索记录）。

## What Changes

### 1. 实体（新增）

- `device_authorization`：`id` Uuid PK · `device_code_hash`（blake3 hex，唯一）· `user_code`（8 字符 base-20，`WDJB-MJHT` 形）· `client_id` · `client_name`（批准页展示，如 `swarmhive @ macbook.local`）· `token_name`（待铸 PAT 名）· `scope`（保留，null=全权 PAT）· `status` enum(`pending`/`approved`/`denied`/`completed`) · `user_id`（批准前 null）· `interval_secs` · `last_polled_at`（slow_down 追踪）· `approved_at` · `expires_at`（创建 +15min）· `created_at`。过期不落 `expired` 状态，由 `expires_at < now()` 派生。

### 2. Server（新增 `routes/device.rs`，5 endpoint，vertical-slice 单文件）

- `POST /api/v1/auth/device/code`（公开，governor 限流）→ `DeviceCodeResponse { device_code, user_code, verification_uri, verification_uri_complete, expires_in, interval }`。bootstrap window 期间（user 表空）→ `410` typed `device_not_available_during_bootstrap`（无人可批准，给即时清晰报错而非静默超时）。
- `POST /api/v1/auth/device/token`（公开，轮询）→ 成功 `200 DeviceTokenResponse { token, name, kind, created_at }`；未决/限速/拒绝/过期按 **RFC 8628 OAuth 错误格式**返 `400 { error }`（`authorization_pending` / `slow_down` / `access_denied` / `expired_token` / `invalid_grant`），**非** RFC 9457——让任何标准 device-flow 客户端可互操作。
- `GET /api/v1/auth/device/lookup?user_code=`（require auth）→ `DeviceAuthorizationView`（展示是哪个 CLI/host 在请求）；找不到/过期 → 404。
- `POST /api/v1/auth/device/approve`（require auth）body `{ user_code }` → 204，置 `status=approved`、`user_id=current`。
- `POST /api/v1/auth/device/deny`（require auth）body `{ user_code }` → 204，置 `status=denied`。
- token 铸造：poll 命中 `approved` 时，复刻 `cli_token` 的临时 `Principal` 套路（`service::load_user_permissions` + `token_service::create(kind=Pat, permissions=None)`），置 `status=completed`，明文随响应返回一次。
- audit：`auth:device_authorized`（approve）/ `auth:device_denied`（deny）/ `auth:token_created`（既有，铸造时）。
- `verification_uri` 复用 `ServerConfig.base_url`（与 invite/reset 链接同源）：`{base_url}/device`。

### 3. Server（删除 ROPC）

- 删 `routes/auth.rs::cli_token` handler + `CliTokenReq` struct，`build_router` / `openapi_router` 两处 unmount。
- 删 api-types `CliTokenRequest` / `CliTokenResponse`（+ `lib.rs` re-export），新增 `api-types/src/device.rs` 全套 DTO。

### 4. CLI（重写 `commands/login.rs`）

- `POST /device/code { client_id: "swarmhive-cli", token_name: <hostname>-<ts> }` → 打印 `user_code` + `verification_uri`，尝试 `webbrowser::open(verification_uri_complete)`。
- 按 `interval` 轮询 `/device/token`，`slow_down` 时 `interval += 5`，`access_denied`/`expired_token` 报错退出。
- 拿到 token 后 `GET /api/v1/auth/me` 取 email/display_name 写 `credentials.toml` 的 `email` 字段 + 打印 `Logged in as <email>`。
- `Command::Login { server, email }` 去掉 `--email`（浏览器负责身份）；`login.rs` 不再用 `rpassword`（dep 仍被 `client.rs` 的 `--secret-stdin` 使用，保留）。

### 5. Admin SPA（新增顶层 public 路由 `routes/device.tsx`）

- public 顶层路由（**不**放 `_auth/` 下——auth guard 的 `next: location.pathname` 会丢 `?user_code`）。仿 `accept-invite.tsx` 自管登录闸门。
- 未登录（`me` 401）→ 渲染 "Sign in to continue" → `Link` 到 `/login?next=` + encode 完整 `path+search`（保住 `user_code`）；登录后回到 `/device?user_code=` 继续。这就是与 `add-oauth-github-and-provider-config` 的**唯一接口契约**：`/login` 上有什么登录方式（密码 / 未来 GitHub），device 页就继承什么。
- 已登录 → 读 `?user_code` 预填输入框 → `lookup` 展示 client/host → 「批准」/「拒绝」调对应 endpoint → 成功提示「回到终端」。

## Capabilities

### New Capabilities

- `cli-device-login`：RFC 8628 device flow 的 server 端点契约 + bootstrap 排除 + 浏览器批准页 + CLI 委托登录 + ROPC 移除的可观测行为契约。

### Removed

- `POST /api/v1/auth/cli-token`（ROPC）及其 DTO——被 device flow 完全替换。

## Impact

- **Code**：server 新增 `routes/device.rs`（5 endpoint）+ 删 `cli_token`；新 entity；api-types 新 `device.rs` + 删 `api_token.rs` 的 Cli* DTO；CLI 重写 `login.rs`；admin 新 `routes/device.tsx`。
- **DB**：新增 `device_authorization` 表。
- **API**：新增 `/api/v1/auth/device/*`；删 `/api/v1/auth/cli-token`。OpenAPI drift gate 触发（跑 `pnpm --filter @swarm-hive/admin openapi`）。
- **Deps**：CLI +`webbrowser`（workspace pin）；server / api-types 无新依赖。CLI 仍 **0** entity/sea-orm 依赖（`cargo tree -p swarmhive-cli | grep sea-orm` 必须空）。
- **不影响**：PAT / API Token 鉴权链路、Storage、Mail、RBAC entity、web OAuth flow（独立）。

## Non-goals

- **不实现 Loopback + PKCE flow**——device flow 覆盖远程/SSH/容器（release CLI 的真实运行环境），loopback 的笔记本 UX 优势留后续 `add-cli-loopback-login`（`--web` 可选加速）。
- **不保留 ROPC 任何形态**——用户拍板直接废弃；CI 非交互场景本就用 Web Admin 创的 scoped API Token + `SWARMHIVE_TOKEN`（已实现），不依赖密码授权。
- **不把 token 存储改成 OS keychain**——`credentials.toml`(0600) 不动；keychain 化（`keyring` crate）是正交改进，留独立 `add-cli-credential-keychain`。
- **不发短时 access token + refresh token**——device flow 铸的仍是可撤销 PAT（Tokens 页 / `swarmhive logout` 撤销），与现模型一致；短时令牌 + refresh 留后续评估。
- **不做 device flow 的多 client_id 注册**——MVP 固定一等公民 `swarmhive-cli`（public client，无 secret）。

## Depends on

- `add-pat-and-api-token`（archived）—— `token_service::create` / `api_token` 表 / Bearer 鉴权链路 / token 格式。
- `add-login-and-owner-bootstrap-ui`（archived）—— `/login` 路由容器（认证闸门）+ `next` search param + bootstrap window 模型（`bootstrap_state`）。
- `add-admin-frontend-foundation`（archived）—— typed `$api` client / 公开页范式 / 错误链 / i18n。
- **不依赖** `add-oauth-github-and-provider-config`——两者只共享 `/login` 闸门，server 端零代码耦合，可任意顺序落地。
- **需与 `add-registration-policy-and-self-register`（⑤）协调**（非硬依赖）——⑤ 给 `_auth` guard 加 `pending_approval` 拦截；本 proposal 的 `/device` 是 public 路由 + approve/deny 仅校验 Session，故 ⑤ 落地后 approve/deny 须补 `user.status==active` 校验（见 design Risks + spec approve/deny Requirement 的协调备注）。本 proposal 单独落地时无 `pending_approval` 状态，不受影响。

## Maps to docs

- [docs/12-cli.md](../../../docs/12-cli.md) login 命令
- [docs/13-rbac.md](../../../docs/13-rbac.md) 凭证体系（PAT）
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) 鉴权段（cli-token → device flow 替换）
- [dev-notes/knowledge/admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) 公开页 / auth guard 约定

## Acceptance

- `swarmhive login http://localhost:3030` → 打印 `user_code` + 打开浏览器 → 已登录用户在 `/device` 批准 → CLI 轮询成功 → `credentials.toml` 写入 PAT + 打印 `Logged in as <email>`。
- 在 `/device` 拒绝 → CLI 轮询拿到 `access_denied` → 退出码非零 + 清晰报错。
- 未登录访问 `/device?user_code=WDJB-MJHT` → 提示登录 → 走 `/login`（密码或 GitHub）→ 回 `/device` 时 `user_code` 仍在。
- bootstrap window（user 表空）`POST /api/v1/auth/device/code` → 410 typed `device_not_available_during_bootstrap`。
- 轮询快于 `interval` → `400 { error: "slow_down" }`；`device_code` 过期 → `400 { error: "expired_token" }`；已 `completed` 的 `device_code` 再轮询 → `400 { error: "invalid_grant" }`。
- `POST /api/v1/auth/cli-token` 返 404（已移除）；`grep cli-token` 在 server/api-types/CLI 源码无残留。
- `cargo test --workspace`（含新 `device_login_smoke.rs`）/ `cargo clippy --workspace --all-targets -D warnings` / `pnpm lint` / `pnpm --filter @swarm-hive/admin typecheck` 全绿；`cargo tree -p swarmhive-cli | grep sea-orm` 空；OpenAPI drift gate 通过。
