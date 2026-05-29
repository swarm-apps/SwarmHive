# add-registration-policy-and-self-register

## Why

①②③④ 落地后整个账号体系已经能"Owner 邀请 + email/password + GitHub OAuth + 忘记密码"全闭环，但还差一类入口：**用户自助注册**（Coolify / Plausible / Outline 都默认关闭但提供该选项）。

同时 ③ 的 OAuth callback 分支"新 GitHub 用户登录"目前是默认 401 拒绝，需要本 proposal 落 policy 后才能开放。

本 proposal 是整个 onboarding 蓝图的收口，引入 **registration_policy** 作为运行时可配的开关系统，统一管理：

- 邮箱自助注册开 / 关 + verify 强度 + 默认角色 + 是否需 Owner 审批
- OAuth 自助注册开 / 关（独立于邮箱开关）

附带补齐两个状态机相关的能力：**email 验证流**（已留 token purpose）+ **pending_approval 工作流**（Owner 审批界面）。

## What Changes

### 1. 实体（新增）

- `registration_policy` singleton 表（id always 1）：
  - `allow_self_register_email: bool` default false
  - `allow_self_register_oauth: bool` default false
  - `require_email_verify: bool` default true（仅 email register 时生效）
  - `self_register_default_role_id: Uuid`（默认指向 viewer role）
  - `self_register_require_approval: bool` default true（true → status=pending_approval，false → status=active）
  - `allowed_email_domains: Vec<String>`（空 = 无白名单；非空 = 仅这些 domain 可注册）
  - `updated_at: DateTimeUtc`
  - `updated_by: Uuid`

### 2. 实体扩展

- `user.status` enum 增 `pending_approval`（④ 已加 pending_verify；本 proposal 再加）

### 3. Server endpoints

#### 自助注册（email）

- `POST /api/v1/auth/register { email, display_name, password }`
  - 校验 `registration_policy.allow_self_register_email=true` → 否则 410 `registration_disabled`
  - 校验 email 未占用 → 422 `email_already_taken`
  - 校验 email domain in `allowed_email_domains` if 非空 → 422 `email_domain_not_allowed`
  - 校验 password 强度（复用 ① garde）
  - INSERT user(status=pending_verify if require_email_verify else active/pending_approval) + user_credentials
  - 若 `require_email_verify=true` → gen EmailVerify token → 发 `email_verify` 邮件 → 返 200 `{ next: 'verify_email' }`
  - 否则 → 写 session（如 status=active）or 返 200 `{ next: 'pending_approval' }`（如 status=pending_approval）

#### Email verify

- `POST /api/v1/auth/verify-email { token }`：verify EmailVerify token → user.email_verified=true → 若 require_approval=true 设 status=pending_approval 否则 status=active + role_id 绑定 → 写 session → 返 200 `{ next: 'pending_approval' | 'home' }`
- `GET /api/v1/auth/verify-email/info?token=`：返 email, expires_at
- `POST /api/v1/auth/verify-email/resend { email }`：始终返 200（防枚举）；找到 user.email_verified=false → invalidate active token → gen new → 发邮件

#### user.email_verified 字段

- `add-auth-and-rbac` 的 user entity 加 `email_verified: bool` default false（archived 中无；本 proposal 加）
- `add-auth-and-rbac` 已 active 的 user 全部 backfill `email_verified=true`（迁移期一次性 UPDATE，避免老用户被卡）

#### OAuth 自助注册分支接入

- ③ 的 callback 流程在"无现存 user 无冲突"分支：
  - 改原 401 oauth_registration_disabled 为：读 `registration_policy.allow_self_register_oauth`
    - false → 维持 401 oauth_registration_disabled
    - true → 校验 GitHub email domain in allowed_email_domains → 创 user(status=active/pending_approval 看 require_approval) + role_id 绑定 + 创 identity_link → 写 session

#### Pending approval 工作流

- `GET /api/v1/users/pending-approval`（require `user:manage`）：分页 list status=pending_approval users
- `POST /api/v1/users/:id/approve { role_id? }`（require `user:manage`）：将 user status 改 active；role_id 可选覆盖 policy 默认值
- `POST /api/v1/users/:id/reject { reason? }`（require `user:manage`）：DELETE user（CASCADE delete user_role / user_credentials / identity_link / account_token）+ 可选发拒绝邮件（NTH，本 proposal 不发）
- 已登录的 pending_approval user：访问任何 `_auth/*` → 路由 beforeLoad 检查 user.status，pending_approval → redirect 到 `/awaiting-approval` 页

#### Policy CRUD

- `GET /api/v1/auth/registration-policy`（require `auth:manage`）：返单 row（id=1）
- `PUT /api/v1/auth/registration-policy`（require `auth:manage`）：更新字段；audit log
- 首启 seed 默认行（id=1, 全 false / require_email_verify=true / default_role_id=viewer / require_approval=true）

### 4. Admin SPA

#### Settings > Authentication 扩展

- ③ 已落 `routes/_auth.settings.authentication.tsx`（OAuth provider 配置）；本 proposal 在同页底部加 "Registration Policy" 卡片：
  - allow_self_register_email Switch + allow_self_register_oauth Switch
  - require_email_verify Switch（disable 若 allow_self_register_email=false）
  - self_register_default_role_id Select（role 列表）
  - self_register_require_approval Switch
  - allowed_email_domains 多 Tag 输入
  - "保存" 按钮 → PUT policy + notification

#### 自助注册页

- 新建 `routes/register.tsx`：公开路由；beforeLoad 调 `setupInfoQueryOptions` + policy → 若 bootstrap 未完成 → redirect /setup；若 allow_self_register_email=false → redirect /login + Alert
- ProForm: email + display_name + password + confirm；submit POST register；成功后按 response.next 跳转：
  - 'verify_email' → /verify-email-sent 页（i18n "请查收邮箱完成验证"）
  - 'pending_approval' → /awaiting-approval
  - 'home' → /

#### Email verify 页

- 新建 `routes/verify-email.tsx`：公开路由；解析 search.token → query info → 渲染 "点击下方按钮确认邮箱" → POST verify-email → 跳 next

#### Pending approval 状态页

- 新建 `routes/_auth.awaiting-approval.tsx`：受 _auth guard；渲染 Result info 卡片 "你的账号正在等待管理员审批"；定期 invalidate me query 检查是否已 approved（5s polling 或手动 refresh）
- `_auth.tsx` beforeLoad 加：若 me.user.status === 'pending_approval' && current_path !== '/awaiting-approval' → redirect

#### Users 页扩展

- ④ 已落 `routes/_auth.users.tsx` 最小版；本 proposal 加：
  - status filter 加 'pending_approval' 选项
  - pending_approval 行加 "Approve" / "Reject" actions
  - Approve 按钮 → confirm modal (含 role select) → POST approve
  - Reject 按钮 → confirm modal (含 reason text) → POST reject

#### Settings > Authentication 顶部 banner（如果 ⑤ 检测到 mail 未配置且 require_email_verify=true）

- 提示 "邮箱验证已启用但 Mail 未配置，注册流程会卡住；请先到 Settings > Mail 配置 SMTP"

### 5. Audit log events

`user_self_registered` / `email_verified` / `user_approved` / `user_rejected` / `registration_policy_updated`

## Capabilities

### New Capabilities

- `registration-policy-and-self-register`：注册策略 + 自助注册 + email verify + pending_approval 工作流的可观测行为契约

### Modified Capabilities

- 扩展 `add-auth-and-rbac` 的 user entity 加 `email_verified` + status enum 加 'pending_approval'
- 扩展 ③ 的 OAuth callback "新 GitHub user 无冲突" 分支由 401 改为 policy-driven

## Impact

- **Code**：server 新 endpoints 6 个 + entity 1 个 + user entity 字段 / enum 扩展；admin SPA 新增 4 页（register / verify-email / awaiting-approval / settings authentication policy 卡片）+ Users 页扩展
- **DB**：新增 `registration_policy` 表 + `user.email_verified` 字段 + `user.status` enum 'pending_approval'
- **API**：新增 `/api/v1/auth/{register, verify-email, verify-email/info, verify-email/resend, registration-policy}` + `/api/v1/users/{pending-approval, :id/approve, :id/reject}`
- **OpenAPI**：drift gate 触发
- **Deps**：无新增
- **Mail templates**：扩展 ② 已 seed 的 `email_verify` 模板内容
- **不影响**：CLI / PAT / Storage

## Non-goals

- 不实现"邀请用户必须再 verify email"（邀请 accept 隐式表示邮箱已 verify）
- 不实现 OAuth-only user 的"补设密码"流程（profile-ui proposal 处理）
- 不实现拒绝邀请的邮件通知（仅 DB delete + 可选审计）
- 不实现 SCIM / Workforce Identity Sync
- 不实现"自助注册带 invite token bypass approval"（混合模型留 NTH）
- 不实现 magic-link 登录（NTH，跟 password reset 模型不同）

## Depends on

- `add-auth-and-rbac`（archived）—— provide user / role / permission
- `add-admin-frontend-foundation`（archived）—— provide UI 基础
- `add-login-and-owner-bootstrap-ui`（①，pending）—— provide bootstrap window + 密码强度 + /login
- `add-mail-infrastructure`（②，pending）—— provide Mailer + email_verify template
- `add-oauth-github-and-provider-config`（③，pending）—— provide oauth_provider + callback handler 钩子
- `add-invite-and-password-reset`（④，pending）—— provide account_token 通用机制 + Users 页基础

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) Registration Policy 段（新增）
- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) Users 页 pending_approval 状态
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) registration_policy + status 状态机
- [dev-notes/knowledge/admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) pending_approval 路由分流
- [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md) ⑤ 段

## Acceptance

- Owner → Settings > Authentication 打开"邮箱自助注册" + "要求验证邮箱" + "需 Owner 审批" + 默认角色 viewer → 保存
- 第三方访问 / → /register 入口出现（/login 顶部加 "没有账号？注册" 链接）
- 第三方填表注册 → 收 verify email → 点链接 → 验证成功 → 跳 /awaiting-approval
- Owner 在 Users 页看到 pending_approval 行 → Approve → 第三方下次刷新 me query → 跳 /
- 关闭"邮箱自助注册" → /register 路由 redirect /login + Alert "自助注册已关闭"
- 配 allowed_email_domains = ['example.com'] → @other.com 注册 → 422 email_domain_not_allowed
- 开 allow_self_register_oauth=true + GitHub email 无冲突 + GitHub provider enabled → OAuth callback 自动创账号（按 policy 决定 status / role）
- pending_approval user 登录后访问 /apps → redirect /awaiting-approval
- 邮箱验证已启用但 Mail 未配置 → Settings > Authentication 顶部 banner 警示
- `pnpm lint` / `cargo clippy` / `cargo test --workspace` / `pnpm --filter @swarm-hive/admin test` 全绿
- OpenAPI drift gate 通过
