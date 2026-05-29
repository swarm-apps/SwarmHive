# add-oauth-github-and-provider-config

## Why

docs/13 要求 MVP 支持 GitHub OAuth 登录与 email-password 并存。原 `add-oauth-github` proposal 只覆盖 server 侧 OAuth flow，没考虑 OAuth provider 的"运行时配置"维度 —— 跟 mail provider 一样，admin 用户预期能在 web 后台填 client_id / client_secret / scopes，不是改 config.toml 重启。

同时探索（2026-05-27）拍板了 admin Settings 菜单的统一布局：Mail / Authentication / Storage / Telemetry。OAuth provider 落在 "Authentication" 子页。

本 proposal 在原 oauth-github 基础上加 oauth_provider 实体 + admin Settings>Authentication UI + bootstrap 阶段约束（OAuth 不在 bootstrap 入口，必须先 email/password 创 Owner 再配 OAuth）。

## What Changes

### 1. 实体（新增）

- `oauth_provider`：id Uuid PK、kind enum(`Github` MVP，留扩展)、name（显示名 e.g. "GitHub"）、enabled bool、client_id、`client_secret_encrypted`、`scopes: Vec<String>`、authorize_url、token_url、userinfo_url、email_field（默认 `'email'`，针对自定义 OIDC 后用）、created_at、updated_at
  - GitHub 预设：authorize/token/userinfo URL 在创建时若 kind=Github 自动填默认值
  - DB partial unique index：`UNIQUE(kind)`（一种 kind 只能配一个 provider；future 改为支持多 instance 时移除）
- `identity_link` 实体已在 `add-auth-and-rbac` 落地，本 proposal 仅消费

### 2. Server

- 引入 `oauth2` crate
- `swarmhive-server/src/auth/oauth/` 模块：
  - `IdentityProvider` trait（`name() -> &str`、`authorize_url(state, redirect_uri) -> Url`、`exchange(code, state) -> Result<ExternalIdentity>`）
  - `ExternalIdentity { subject: String, email: Option<String>, display_name: Option<String>, avatar_url: Option<String>, raw: serde_json::Value }`
  - `GithubProvider` 实现：调 `https://github.com/login/oauth/authorize` + token 交换 + `/user` + `/user/emails`
  - `provider_factory(row: oauth_provider::Model, secret_key: SecretKey) -> Box<dyn IdentityProvider>`：从 DB row 构造（decrypt client_secret 复用 mail crypto pattern）
- Server endpoints：
  - `GET /api/v1/auth/oauth/:provider_name/start` → 生成 state + PKCE → 重定向到 provider；session 持 state nonce 校验回调
  - `GET /api/v1/auth/oauth/:provider_name/callback` → 校验 state → exchange → 查 identity_link → 已存在 → 写 session；不存在 + email 冲突 → 409 problem+json + 文案；不存在 + 无冲突 → 留给 ⑤ 的 self-register 分支（本 proposal 默认 401 拒绝）
  - `GET /api/v1/auth/oauth/providers`（公开）：返当前 enabled provider 列表（仅 name + kind，不含 secret）；用于 admin SPA 的 /login 按钮渲染
  - `POST /api/v1/auth/oauth/providers/link/:provider_name/start`（require auth）：登录用户从 Profile 绑定 GitHub
  - `DELETE /api/v1/auth/oauth/links/:provider_name`（require auth）：解绑（保留 password 路径以免锁死）
- OAuth provider CRUD（admin Settings 配置）：
  - `GET /api/v1/auth/providers`（require `auth:manage`）：返完整 list（含 client_id，不含 secret）
  - `POST /api/v1/auth/providers`、`PUT /:id`、`DELETE /:id`、`POST /:id/test`
  - `/test` 行为：仅校验 client_id 存在且 secret 非空 + GitHub authorize_url 可达（HEAD 200/302）；不调真实 token exchange
- secret 加密：复用 ② `SecretKey` 模式（共享 `SWARMHIVE_SECRET_KEY` ENV）；若 ② 未先落地，本 proposal 自带 SecretKey 模块然后 ② 复用之，二选一
- 邮箱冲突：GitHub 返回的 verified email 已被现存 password user 占用 → 409 problem+json `type=oauth_email_conflict` body 含 `{ email, hint: 'sign in with password first then link from profile' }`
- Bootstrap window 期间（user 表空）`GET /api/v1/auth/oauth/*/start` → 410 problem+json `type=oauth_not_available_during_bootstrap`（避免抢成 Owner）

### 3. Admin SPA: /login 加 OAuth 按钮

- `routes/login.tsx`（① 落地的）增加：`useQuery($api.queryOptions('get', '/api/v1/auth/oauth/providers'))` → 按返回列表渲染按钮组
- 按钮 click → `window.location = '/api/v1/auth/oauth/:name/start?next=' + encodeURIComponent(next)`
- 列表空 → 不显示按钮区（无视觉残留）

### 4. Admin SPA: Settings > Authentication 页

- 路由 `routes/_auth.settings.authentication.tsx`（② 已落 `_auth.settings.tsx` layout，本 proposal 把 "Authentication" 菜单条目从 disabled 改 enabled）
- ProTable 列表 oauth_provider：name / kind / enabled / actions（编辑 / 测试 / 启用 / 删除）
- ProDrawerForm 编辑：client_id + client_secret（编辑时空 = 不改）+ scopes (Select multi) + enabled toggle；kind 选 GitHub → 自动预填 authorize/token/userinfo URL（可改）
- "Test" 按钮：POST `/test` → notification 成功 / 失败（含具体错误）
- 提示卡片："注意：OAuth 配置后，新成员的自助注册行为由 Registration Policy 控制（Settings > Authentication > Registration Policy，待 ⑤ 落地）"

### 5. Admin SPA: Profile 绑定 GitHub

- 新建 `routes/_auth.profile.tsx`（最小版，本 proposal 只放 OAuth Linked accounts 区，其他 profile 字段后续 proposal 补）
- 列表当前 user 的 identity_link rows（`GET /api/v1/auth/me/identity-links`）
- "Link GitHub" 按钮 → `window.location = '/api/v1/auth/oauth/github/link/start'`
- "Unlink" 按钮（每行）→ confirm modal → DELETE → 提示 "解绑后只能用 [email] + 密码登录"；如果该 user 无 password（OAuth-only 来源），禁用 Unlink

### 6. 权限补充

- `add-auth-and-rbac` permission 集补 `auth:manage`，默认 `owner` 持有，`admin` 持有（同 mail:manage）

## Capabilities

### New Capabilities

- `oauth-github-and-provider-config`：OAuth 登录 + 绑定 + 解绑 + admin 后台 provider CRUD + 邮箱冲突处理的可观测行为契约

### Modified Capabilities

- 扩展 `add-auth-and-rbac` permission 集（加 `auth:manage`），不修改 archived spec

## Impact

- **Code**：server `src/auth/oauth/` 新模块（4-5 文件） + 1 新 entity + 6 endpoint；admin SPA 新增 `/login` 按钮渲染 + Settings>Authentication 页 + Profile linked accounts
- **DB**：新增 `oauth_provider` 表；user_role 表无变化
- **API**：新增 `/api/v1/auth/oauth/*` + `/api/v1/auth/providers` 全套
- **OpenAPI**：drift gate 触发
- **Deps**：server +`oauth2`；admin 无新依赖
- **不影响**：CLI / PAT / Storage / Mail entity / RBAC entity

## Non-goals

- 不实现 Google / GitLab / 内部 OIDC（trait 留好，本 proposal 只挂 GitHub kind）
- 不实现 GitHub Enterprise（host 字段保留可改，但不测试矩阵）
- 不做 SCIM 用户同步
- 不做 OAuth 自助注册分支（callback 中"无现存 user 且无冲突"的处理留给 ⑤；本 proposal 默认拒绝）
- **不在 bootstrap 阶段提供 OAuth 入口**：首个 Owner 必须 email/password；登录后再配 OAuth
- 不实现 Profile 页其他字段（display_name / avatar 修改、密码修改、API token 管理等）—— 留后续 profile-ui proposal

## Depends on

- `add-auth-and-rbac`（archived）—— provide identity_link 实体 + permission 集 + Principal extractor
- `add-admin-frontend-foundation`（archived）—— provide ProTable / ProForm / Settings 菜单注入位 / $api client / 错误链
- `add-login-and-owner-bootstrap-ui`（①，pending）—— provide /login 路由容器（按钮注入点） + bootstrap window 模型（保证 OAuth 不能成为 first owner）
- `add-mail-infrastructure`（②，pending）—— provide SecretKey 加密复用（client_secret_encrypted 用同样 AES-GCM 模式）；若 ② 未先落地，本 proposal 自带 SecretKey 模块然后 ② 复用之，二选一

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) Identity Providers
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) 补 OAuth 模块
- [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md) ③ 段

## Acceptance

- Owner 登录 → Settings > Authentication → 新建 GitHub provider 填 client_id/secret → 启用 → /login 出现 "Sign in with GitHub" 按钮
- 已用密码登录的 Owner → Profile → Link GitHub → 跳 GitHub → 回跳 → 显示 linked
- 已 link 的 user → /logout → /login 点 GitHub → 直接登录成功
- 第三人首次用 GitHub 登录，email 已被某 password user 占用 → 跳 /login + Alert "GitHub 邮箱 X 已注册，请先用密码登录后到 Profile 绑定"
- 第三人首次用 GitHub 登录，email 无冲突 → 401（待 ⑤ self-register 分支接入）
- Bootstrap window 期间（user 表空）访问 `/api/v1/auth/oauth/github/start` → 410 problem+json `type=oauth_not_available_during_bootstrap`
- `pnpm lint` / `cargo clippy` / `cargo test --workspace` / `pnpm --filter @swarm-hive/admin test` 全绿
- OpenAPI drift gate 通过
