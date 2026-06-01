# RBAC 权限模型

## 设计结论

SwarmHive MVP 做 **单组织 + 完整 RBAC**，不做真正多租户。

原因：SwarmHive 涉及高风险操作，包括上传安装包、发布 stable、强制更新、rollback、配置 S3/RustFS 密钥、管理 CI Token、查看下载与埋点数据。权限边界应从第一版就建立。

## 多租户边界

MVP 只有一个默认组织：

```text
Default Organization
  └─ Apps
      └─ Releases
          └─ Artifacts
```

但数据库核心表预留 `org_id`，未来可以演进为多组织 / managed cloud。

第一阶段不做：

- 多组织切换。
- 组织级计费。
- 组织间强隔离策略。
- 配额管理。

## 角色

### Owner

系统所有者。

能力：

- 管理用户。
- 管理角色。
- 管理存储配置。
- 管理 API Token。
- 管理所有应用、版本、策略。
- 查看所有统计和埋点。

### Admin

应用和发布后台管理员。

能力：

- 管理应用。
- 管理版本。
- 管理更新策略。
- 创建 release。
- 查看统计。

限制：

- 不能管理 Owner。
- 不能修改系统级敏感配置，除非显式授予。

### Release Manager

发布负责人。

能力：

- 发布版本。
- promote。
- rollback。
- yank。
- 上传 artifact。

### Developer

开发者。

能力：

- 上传 draft / beta 产物。
- 查看自己有权限的应用和版本。

限制：

- 不能发布 stable。
- 不能强制更新。
- 不能管理 storage/token。

### Viewer

只读角色。

能力：

- 查看应用。
- 查看版本。
- 查看下载量。
- 查看埋点漏斗。

## Permission 列表

角色只是 permission 集合。服务端鉴权应按 permission 判断。

### System

- `system:manage`
- `user:manage`
- `role:manage`
- `token:manage`
- `storage:manage`
- `mail:manage` — 邮件 provider / template / log 管理（Owner + Admin 默认持有）。所有 `/api/v1/mail/*` 写接口和 logs / templates / providers 列表均 require 此权限；`GET /api/v1/mail/status` 故意公开（驱动 SPA 顶部 fallback banner）。

### App

- `app:create`
- `app:read`
- `app:update`
- `app:delete`

### Release

- `release:create`
- `release:read`
- `release:update`
- `release:publish`
- `release:promote`
- `release:rollback`
- `release:yank`

### Artifact

- `artifact:upload`
- `artifact:read`
- `artifact:delete`

### Analytics

- `analytics:read`
- `telemetry:read`

## Scope

权限支持作用域。

MVP 支持：

- org-level role。
- app-level role。
- token app scope。
- token channel scope。

示例：

```text
User A: org Owner
User B: app swarmdrop Developer
User C: app swarmnote-rn Release Manager
```

## Identity Providers

用户身份在 MVP 即支持多种来源，模型上通过 `User` + `IdentityLink` 拆分：

```text
User (id, email, display_name, status, created_at)
  └─▶ IdentityLink (user_id, provider, subject, metadata)
        provider ∈ { "password", "github", ...(future: google, gitlab, oidc) }
        subject  = provider 侧的稳定 ID（password: email；github: numeric user id）
```

MVP 首批：

- **password**：email + argon2id 密码 hash。
- **github**：通过 OAuth2 拿到 GitHub user → 写入 `IdentityLink(provider="github", subject=<github-id>)`；若 email 已存在则提示用户登录后绑定，避免账号分裂。

未来可加 Google / GitLab / 内部 OIDC，只需新增 provider 适配器。

**运行时配置（`add-oauth-github-and-provider-config`）**：OAuth provider 跟 mail provider 一样在 Admin 后台 `Settings > Authentication` 配置（client_id / client_secret / scopes / 启用开关），不改 config.toml 重启。client_secret AES-GCM 加密 at-rest（复用 `SWARMHIVE_SECRET_KEY`），任何 API 永不回明文。一种 kind 在 MVP 只配一个 provider（`UNIQUE(kind)`）。配置 / 绑定 / 解绑需 `auth:manage` 权限（owner + admin 默认持有）。

**登录 / 绑定流程**：

- 已配置 + 启用的 provider 自动出现在 `/login` 的「使用 X 登录」按钮区（公开 `GET /api/v1/auth/oauth/providers` 驱动；空则不显示）。
- callback 分支：已有 `identity_link` → 直接登录；GitHub 的 **verified** 邮箱已被某 password 用户占用 → **不自动合并**，302 回 `/login?oauth_conflict=`（提示先用密码登录再到个人资料绑定，不在 URL 暴露邮箱）；陌生邮箱无冲突 → 401（自助注册留 `add-registration-policy-and-self-register`）。只信任 GitHub `/user/emails` 的 verified 邮箱。
- 已登录用户在 `Profile`（个人资料页）绑定 / 解绑 GitHub；解绑唯一登录方式（无密码）会被拒（409），先设密码。
- **bootstrap 排除**：`user` 表空时所有 OAuth start 端点返 410 —— 首个 Owner 必须 email/password 走 `/setup`，防陌生 GitHub 用户抢成 Owner。
- CLI 登录（`add-cli-device-login` 的 device flow）的浏览器批准步骤复用同一 `/login`，故 **OAuth-only 用户（无密码）也能登录 CLI**。

## Bootstrap Owner（Coolify 模式 + 可选 ENV 锁）

首次启动 server（`user` 表为空）时，admin SPA 检测到 bootstrap window 并把任意访问跳转到 `/setup`。运维只需在浏览器填邮箱 + 密码即创建 Owner，无需 SSH 上去抓 token。流程：

```text
1. server 启动 + admin SPA 打开
   ╔════════════════════════════════════════════════════╗
   ║  SwarmHive first-run setup                         ║
   ║  Open the Admin SPA in a browser and complete      ║
   ║  /setup to create the initial Owner.               ║
   ║  Tip: set SWARMHIVE_BOOTSTRAP_OWNER_EMAIL before   ║
   ║  exposing this server to the network.              ║
   ╚════════════════════════════════════════════════════╝

2. 浏览器 → 任意路径 → root beforeLoad 读 /api/v1/setup/info
   → needs_bootstrap=true → redirect /setup
3. /setup 表单：email + display_name + password (≥12 字符 / ≥3 类 / 不在弱口令字典)
   若设置 SWARMHIVE_BOOTSTRAP_OWNER_EMAIL，email 字段固化为该值
4. 提交 → POST /api/v1/setup { email, display_name, password } → 创建 Owner + auto-login
5. 第二次访问 /setup → 410 Gone (bootstrap-already-complete) → redirect /login
6. 若 user 表被外部清空 → 下次 server 启动横 banner 重新出现；bootstrap window 重新打开
```

**为什么放弃 stdout setup token，改 Coolify 模式**：

- 部署即用：docker run / helm install 完直接打开浏览器即可，无须 `docker logs | grep`
- 与同类工具 UX 对齐：Coolify、Plausible、Outline、Vaultwarden web 模式都是这个流程
- 安全 trade-off：bootstrap window 内任何能访问 web 的人都能成 Owner —— 用 `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` ENV 锁定 + 部署文档明确"立即访问 web"两层防护

**`SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` 行为**：

- 启动期一次性读，缓存在 `AppState.bootstrap` (immutable for process lifetime)
- 仅在 bootstrap window 期间生效；bootstrap 完成后该 ENV 无效
- 值会在 `/api/v1/setup/info.locked_email` 暴露给 admin SPA → 表单 email 字段 disabled + 预填
- 大小写不敏感比较；mismatch 返 422 typed `bootstrap-email-mismatch`（body 含 `expected_email` 字段供 UI 提示运维）
- docker compose / helm chart 推荐预设该 ENV，避免公网部署的 owner 抢注风险

**密码强度（同样适用于后续 accept-invite / reset-password endpoint）**：

- 最少 12 字符
- 至少 3 类字符（大写 / 小写 / 数字 / 特殊符）
- 不能命中内置 weak-passwords 字典（top-100，`assets/weak-passwords-top100.txt`）
- 失败返 422 typed `password-too-weak`，`detail` 携带具体规则名

**账号级失败软锁**（baseline 安全）：

- 每个 user 一行 `user_login_attempts (user_id PK, failed_count, last_failed_at, locked_until)`
- 5 次连续失败 → `locked_until = now() + 30min`；锁定期内 `/login` 返 410 typed `account-locked-until`（body 含 `locked_until` ISO-8601）
- 成功登录 → DELETE 整行；失败计数清零靠重新插入
- tower-governor 请求级限流（5 rps / burst 20，per-IP）继续并存，处理 IP 维度的洪水

**相关文件**：`crates/swarmhive-server/src/auth/bootstrap.rs` (`BootstrapConfig` / `bootstrap_state`)、`crates/swarmhive-server/src/routes/setup.rs` (`register_owner`)、`crates/swarmhive-server/src/auth/password.rs` (`validate_strong_password`)、`crates/swarmhive-server/src/routes/auth.rs` (login 锁定逻辑)、`crates/swarmhive-entity/src/user_login_attempts.rs`、`apps/admin/src/routes/setup.tsx`、`apps/admin/src/routes/login.tsx`、`apps/admin/src/routes/__root.tsx` (bootstrap-aware beforeLoad)。

## 邀请 / 密码重置 / 邮箱验证（`add-invite-and-password-reset`）

Owner 之后的账号生命周期都靠一次性 token 邮件驱动。三类 token 共用单表 `account_token`（`purpose` 区分 `invite` / `password_reset` / `email_verify`），plaintext 只出现在邮件里，DB 存 `argon2(plaintext)` + `blake3(plaintext)[..16]` lookup 双层（详见 backend.md「account_token 一次性 token」）。

**邀请新成员**：

```text
1. Admin（user:manage）→ POST /api/v1/users/invite { email, role_id, display_name? }
   - role 不能是 owner（422 cannot-invite-owner），email 不能已占用（422 email-already-taken）
   - 事务内：INSERT user(status=invited, email_verified_at=NULL) + user_role + account_token(invite, 72h)
2. 被邀人收邮件 → 点 /accept-invite?token= → GET /auth/accept-invite/info 预检（不消费）
3. 设密码 → POST /auth/accept-invite { token, password }
   - 事务内：consume token + status→active + email_verified_at=now()（点链接已证明邮箱可达）
     + 写 credentials + identity_link → 自动登录
4. Admin 可对 invited 用户 POST /users/invite/{id}/resend 轮换 token（旧 token 立即失效）
```

**忘记密码**（self-service，公开）：

```text
1. POST /api/v1/auth/forgot-password { email } —— 永远返 200 通用提示，不泄露邮箱是否存在
   三分支：① 不存在 / ② 存在但 status≠active / ③ 存在但 email_verified_at=NULL
     → ①②③ 都 silent skip（未验证额外写 audit password_reset_blocked_unverified）
     → 仅「active + 已验证」才 issue token(1h) + 发邮件
   skip 路径用 FORGOT_TIMING_FLOOR=150ms 抹平响应时间，防 enumeration
2. /reset-password?token= → GET /auth/reset-password/info 预检
3. POST /auth/reset-password { token, password }
   - 事务内：consume + upsert credentials + DELETE 该用户所有 session（踢掉所有设备）+ 当前请求自动登录
```

未验证用户被挡在密码重置外，是为了避免「注册了错邮箱的人」通过重置劫持账号——验证过邮箱才证明邮箱归属。

**邮箱验证**（Owner 自助，banner 驱动）：

```text
1. 未验证用户（email_verified_at=NULL）→ Admin SPA 顶部常驻黄色 banner
2. POST /api/v1/users/me/verify-email/send（需登录）
   - 已验证 → 422 email-already-verified
   - 当前 mailer 是 console fallback → 422 mail-not-configured（body 含 expected_next_step=/settings/mail）
   - 60s 内重复发 → 429 rate-limited
   - 否则 issue token(24h) + 发邮件
3. /verify-email?token= → GET /auth/verify-email/info 预检 → POST /auth/verify-email { token }
   - UPDATE email_verified_at=now() WHERE id=? AND email_verified_at IS NULL（幂等）+ consume token
```

软验证（banner 常驻但不强制阻断登录）参考 Cal.com / Mattermost：未验证不影响日常使用，只挡密码重置这类敏感路径。

**相关文件**：`crates/swarmhive-server/src/routes/{invite,password_reset,verify_email,users}.rs`、`src/services/account_token.rs`、`crates/swarmhive-entity/src/account_token.rs`、`apps/admin/src/routes/{forgot-password,reset-password,accept-invite,verify-email}.tsx`、`apps/admin/src/routes/_auth/users.tsx`、`apps/admin/src/lib/query/useResendVerify.ts`；E2E 回归 `crates/swarmhive-server/tests/account_token_smoke.rs`。

## 三类凭证

SwarmHive 同时支持三种登录形态，统一在 server 端汇流成 `Principal { user, scope, permissions, auth_method }`：

| 凭证 | 用途 | 载体 | 撤销方式 |
| --- | --- | --- | --- |
| **Session cookie** | Admin SPA 浏览器登录 | HttpOnly + SameSite=Lax cookie，session 行存 Postgres | 登出 / 后台踢人 |
| **Personal Access Token (PAT)** | CLI 本地 `swarmhive login` 后的长期凭证 | `~/.config/swarmhive/credentials.toml`（0600），或 `SWARMHIVE_TOKEN` 环境变量覆盖 | 用户自己删 / Admin 撤销 |
| **API Token (scoped)** | CI/CD、机器对机器 | env：`SWARMHIVE_TOKEN` | Admin 撤销 / 自动过期 |

三者的 token 字符串都以高熵随机串生成，DB 只存 `blake3` hash，明文仅在创建时显示一次。**不引入 JWT**：单 binary monolith 下 stateless 验证没有收益，撤销 / scope 重发的复杂度大于价值。

**Token 字符串格式**（`add-pat-and-api-token` 落地）：

```text
swhv_pat_<43 char base64url>     # PAT  (52 char total)
swhv_api_<43 char base64url>     # API  (52 char total)
```

`swhv_` 是项目前缀，`pat`/`api` 是 kind 公开标记（便于日志泄露后 grep 与排查），后 43 字符是 32 字节随机 base64url（无 padding）。DB 不存明文，只存 `blake3(plain)` 的 hex；前 12 字符（如 `swhv_pat_AbC`）以 `prefix` 列存储，admin / CLI 列表里用来辨识 token。

**`Authorization: Bearer` 与 cookie 的优先级**：extractor 一旦看到 `Authorization` 头就走 Bearer 分支，**不**回退 cookie——浏览器扩展类工具同时携带两者时显式 Bearer 必须胜出。malformed Bearer 直接 401，不静默走 cookie。

**撤销立即生效**：`DELETE /api/v1/tokens/{id}` 翻 `revoked_at` 后，下一次请求即 401，无 grace period、无 server 端缓存。

**`last_used_at` 节流**：每次 Bearer 命中跑一句 SQL：`UPDATE api_token SET last_used_at = NOW() WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < NOW() - INTERVAL '1 minute')`。WHERE 子句自带节流，单库往返；NULL → Some 的首次翻转额外写一条 `auth:token_used_first_time` audit（actor_type=token）。

## PAT 与 API Token 权限模型

PAT 与 API Token 共用一张表 `api_token`、共用一份 Bearer 鉴权基建，区别只在 `permissions` 列：

| `kind` | `permissions` 列 | 权限来源 | 收回权限 |
| --- | --- | --- | --- |
| `pat` | NULL | **live**：每请求按 `owner_user_id` 现查 `user_role` → `role_permission` | 撤角色 → 旧 PAT 立即收缩 |
| `api` | `Some(subset)` | **snapshot**：创建时显式存的子集 | snapshot 不随 creator 权限变；要新权限就重发 token |

**Why**：

- PAT 是"用户在 CLI 上的化身"，权限应当跟随用户当前状态——撤角色后 PAT 仍带旧权限是漏洞。
- API Token 是"最小权限机器凭证"，必须可显式裁剪到只够 CI 用的几个 permission；snapshot 让 creator 后续被升/降权不影响已发的机器 token，便于审计。

API Token 创建时强制校验 `permissions ⊆ creator.permissions`——前端勾出超额 permission，server 直接 422 + problem+json 列出超额项。

API Token scope 示例（语义保留，**`scope_app_id` / `scope_channel` 列由 add-app-release-artifact 后续追加**——本阶段 app/channel 实体尚不存在，仅支持 org-wide）：

```text
token name = swarmdrop-beta-ci
permissions = artifact:upload, release:create, release:publish
expires_at = 2026-12-31
# (future) app = swarmdrop, channel = beta
```

CI Token 推荐最小权限：

- beta 构建：`artifact:upload`, `release:create`, `release:publish`。
- stable promote：`release:promote`，单独 token 或人工审批后使用。

## CLI 凭证流

`swarmhive login [server]` 是 CLI 主入口，走 **RFC 8628 OAuth 2.0 Device Authorization Grant**（`gh` 风格），CLI 全程不经手密码：

1. CLI POST `/api/v1/auth/device/code` `{ client_id: "swarmhive-cli", token_name }`（token_name 默认 `<host>-<unix-ts>`）→ 拿到 `device_code` + 用户可读 `user_code`（8×base20，`WDJB-MJHT`）+ `verification_uri`。
2. CLI 打印 `user_code` + URL，并尝试 `webbrowser::open` 打开 `{base_url}/device?user_code=…`。
3. 用户在浏览器登录 SwarmHive（**密码 或 GitHub OAuth**——认证步骤复用 Web `/login`），在 `/device` 页确认授权（`POST /device/approve`，**仅接受 session，拒 Bearer PAT 自批**）。
4. CLI 按 `interval` 轮询 `POST /api/v1/auth/device/token`（grant_type = `urn:ietf:params:oauth:grant-type:device_code`）；批准后服务端**原子 claim** 并 mint `swhv_pat_…` + audit `auth:token_created`。轮询错误走 **RFC 8628 wire 格式** `400 { error }`（`authorization_pending`/`slow_down`/`access_denied`/`expired_token`/`invalid_grant`），非 problem+json。
5. CLI 拿到 token 后 `GET /auth/me` 取 email，把 `{ server, email?, token }` 写入 `~/.config/swarmhive/credentials.toml`（`0600`）。**token 获取即成功边界**：/me 失败也保存 token（email 留空），绝不孤儿化已铸 PAT。

**OAuth-only 用户由此天然获得 CLI 能力**——CLI 不认识 GitHub，认证全在浏览器侧完成。device flow 选型（而非 loopback+PKCE）：release CLI 常在远程/SSH/CI 跑，loopback 回跳 127.0.0.1 在这些环境失效；钓鱼风险在单组织自托管场景极低，批准页明示 client/host 缓解。

`SWARMHIVE_TOKEN` env 永远比 credentials 文件优先——CI 直接注入 Web Admin 创的 scoped API Token 即可，不需要 login。

`swarmhive logout`：GET `/api/v1/tokens` 按 prefix 找到当前 token id → DELETE → 删本地文件。server 离线则只删本地、warn，不阻塞。

**bootstrap 排除**：user 表空时 `/device/code` 返 `410`（首个 Owner 必走 `/setup` 的 email/password；没有已存在用户能进批准页）。device endpoint 挂在受 `tower-governor`（5 rps / burst 20，per source IP）保护的 sensitive 子路由，轮询主限速靠协议内 `slow_down`。

## 敏感操作

以下权限需要特别保护：

- `storage:manage`：能配置 S3 / RustFS / OSS 密钥。
- `token:manage`：能创建 CI Token。
- `release:publish`：发布版本。
- `release:promote`：提升 channel。
- `release:rollback`：回滚 channel。
- `release:yank`：撤回版本。
- `analytics:read` / `telemetry:read`：可能涉及用户环境信息。

## 审计日志

关键操作必须写入 audit log：

- 登录成功 / 失败（`auth:login_succeeded` / `auth:login_failed`）。
- 创建 owner（`auth:owner_created`）。
- 创建 / 撤销 token（`auth:token_created` / `auth:token_revoked`，actor_type=user）。
- token 首次使用（`auth:token_used_first_time`，actor_type=token，仅 NULL → Some 翻转时一次）。
- 创建 / 删除用户。
- 修改角色。
- 修改 storage 配置。
- 发布 release。
- promote / rollback / yank。
- 修改强制更新策略。

审计字段：

- actor_type：user / token。
- actor_id。
- org_id。
- app_id。
- action。
- resource_type。
- resource_id。
- ip。
- user_agent。
- created_at。

## Admin UI 行为

- 无权限的按钮隐藏或禁用。
- 执行敏感操作前二次确认。
- 强制更新、rollback、yank 要显示影响范围。
- storage secret 不回显明文。
- token 只在创建时显示一次。

## 路线

MVP：

- 单组织。
- 多用户。
- 角色与权限。
- app-level role。
- scoped API Token。
- 审计日志。

后续：

- 多组织。
- 组织切换。
- 配额。
- managed cloud 隔离。
- 更细粒度数据权限。
