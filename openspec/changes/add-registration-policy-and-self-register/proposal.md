# add-registration-policy-and-self-register

> **Rebased 2026-06-10**：本 proposal 原写于 2026-05-29,是对 ①②③④ "将来长成什么样" 的预测。
> ①②③④ 已全部 apply + archive,实际 ship 的形状与预测有结构性偏差,本次按真实代码重定基。
> 三处核心修正:① 不存在 `pending_verify` 状态(真实只有 `Active/Disabled/Invited`,verify 闸门
> 用正交的 `email_verified_at: Option<ts>` 表达)——⑤ 把 `Invited` **改名 `Provisioned`**(语义纯净,
> 统得起 invite + self-register 两条流,2026-06-10 用户拍板)+ 加 `PendingApproval`,含一次性数据迁移;
> ② **不加 `email_verified: bool`、不做 backfill**(`email_verified_at` 已存在,且 Owner
> setup 故意留 NULL,照原 spec backfill 会覆盖掉它=引入 bug);③ verify-email 端点 + `email_verify`
> 邮件模板**已由 ④ 落地**,⑤ 从"新建"降为"扩展"(给现有 handler 加 `Invited→?` 状态转移)。

## Why

①②③④ 落地后整个账号体系已经能 "Owner 邀请 + email/password + GitHub OAuth + 忘记密码" 全闭环,但还差一类入口:**用户自助注册**(Coolify / Plausible / Outline 都默认关闭但提供该选项)。

同时 ③ 的 OAuth callback 分支 "新 GitHub 用户无冲突登录" 目前硬返 `401 oauth_registration_disabled`(`routes/oauth.rs:319-325`),需要本 proposal 落 policy 后才能开放——这是当前部署里"陌生人首次 GitHub 登录被拦"的直接成因。

本 proposal 引入 **registration_policy** 作为运行时可配的开关系统,统一管理:

- 邮箱自助注册开 / 关 + verify 强度 + 默认角色 + 是否需 Owner 审批
- OAuth 自助注册开 / 关(独立于邮箱开关)

并补齐 **pending_approval 工作流**(Owner 审批界面)。

### 两根支柱 + 推荐落地顺序

| | 支柱 A:Policy + OAuth 自助 | 支柱 B:Email 自助 + 审批 |
|---|---|---|
| 内容 | `registration_policy` 表/CRUD/admin 卡片 + OAuth callback 把硬 401 换成 policy 判定 | `POST /register` + verify-email 扩展 + `PendingApproval` + approve/reject + 4 admin 页 |
| 价值 | 直接解 "陌生 GitHub 用户 + 域白名单 → 自动建号登录" | 填满注册矩阵的 "email/自助" 格 |
| 体量 | 小(原设计 ~20%) | 大(UI 大头) |

本 change 不拆,但 **tasks 排序让支柱 A 在前、可独立 apply 验收**;支柱 B 的审批工作流量大、对单组织内部工具价值偏边际(见 Open Questions),可后续增量推进。

## What Changes

### 1. 实体(新增 + 扩展)

- **新增** `registration_policy` singleton 表(`id` always 1):
  - `allow_self_register_email: bool` default false
  - `allow_self_register_oauth: bool` default false
  - `require_email_verify: bool` default true(仅 email register 时生效)
  - `self_register_default_role_id: Uuid`(FK role,默认指向 viewer)
  - `self_register_require_approval: bool` default true(true → status=PendingApproval,false → Active)
  - `allowed_email_domains: Vec<String>`(空 = 无白名单;非空 = 仅这些 domain 可注册)
  - `updated_at: DateTimeUtc` + `updated_by: Uuid`
- **扩展** `user.status` enum:`Invited` **改名 `Provisioned`** + 加 `PendingApproval`(api-types 同步),最终 `{Active, Disabled, Provisioned, PendingApproval}`。
  - **不引入 `pending_verify`**(verify 走 `email_verified_at`)。`Provisioned` 当 "已建档、待确认(接受邀请 / 验证邮箱)" 统称,统得起 invite(④)+ self-register(⑤)。
  - rename 含**一次性数据迁移** `UPDATE user SET status='provisioned' WHERE status='invited'`,须 raw SQL、排在 entity 读 user 之前(见 design Decision 2 + Migration Plan)。blast radius:`entity/user.rs`、`api-types/user.rs`、`routes/invite.rs`、2 处注释、`account_token_smoke.rs`、admin `_auth/users.tsx`。
- **不改** `email_verified` 信号:复用 ④ 已有的 `user.email_verified_at: Option<DateTimeUtc>`(NULL=未验证,`Some(ts)`=已验证)。**不加 bool 字段、不做 backfill**。

### 2. Server endpoints

#### 自助注册(email)— 真新

- `POST /api/v1/auth/register { email, display_name, password }`(`routes/register.rs`,**扁平**,公开)
  - `policy.allow_self_register_email=false` → 410 `registration_disabled`
  - email 已占用 → 422 `email_already_taken`
  - `allowed_email_domains` 非空且 domain 不在内 → 422 `email_domain_not_allowed`
  - 弱口令 → 422 `password_too_weak`(复用 ① `password::validate_strong_password`)
  - INSERT `user(status=Provisioned, email_verified_at=NULL)` + `user_credentials` + `user_role(default_role)`
  - `require_email_verify=true` → 复用 ④ 机制发 `email_verify` token+邮件 → 200 `{ next: 'verify_email' }`
  - `require_email_verify=false` → 按 `require_approval` 决定:true → `status=PendingApproval` 写 session 返 `{ next: 'pending_approval' }`;false → `status=Active` 写 session 返 `{ next: 'home' }`

#### Email verify — 扩展 ④ 现有 `routes/verify_email.rs`(不新建)

- ④ 已有:`POST /auth/verify-email`(公开消费,写 `email_verified_at`,**不碰 status**)、`GET /auth/verify-email/info`(公开预检)、`POST /users/me/verify-email/send`(auth banner 重发)。
- ⑤ 增量:`POST /auth/verify-email` 消费成功后,**若用户当前 `status=Provisioned`** 则按 `policy.require_approval` 转移到 `PendingApproval`/`Active` + 绑 default role + 写 session;`status=Active`(banner verify)维持原行为只写时间戳。靠 status 字段消歧,与 invite-accept 流不撞(后者走 `/invite/accept` 单设密直接 Active,不碰 verify-email)。
- ⑤ 新增 `POST /api/v1/auth/verify-email/resend { email }`(**公开**,枚举防御始终 200):自助注册者(Provisioned、无 session)用不了 auth 的 `me/send`,需公开按 email 重发。

#### OAuth 自助注册分支接入 — 真新(小),改 `routes/oauth.rs`

- callback "无现存 user、无 email 冲突" 分支(现 `routes/oauth.rs:319-325` 的硬 401)改为读 `policy.allow_self_register_oauth`:
  - false → 维持 401 `oauth_registration_disabled`
  - true → 校验 GitHub verified email 的 domain 在白名单 → 创 `user(status=Active|PendingApproval 看 require_approval, email_verified_at=now())` + `identity_link` + `user_role(default_role)` → 写 session → 302(`/` 或 `/awaiting-approval`)
  - domain 不匹配 → 302 `/login?oauth_error=domain_not_allowed`;`user.email` 唯一约束兜底 race → 302 `/login?oauth_error=race_conflict`

#### Pending approval 工作流 — 真新,扩展 `routes/users.rs`(不新建子目录)

- `GET /api/v1/users/pending-approval`(require `user:manage`,分页 list status=PendingApproval)
- `POST /api/v1/users/:id/approve { role_id? }`(`user:manage`):status→Active;role_id 可选覆盖 policy 默认
- `POST /api/v1/users/:id/reject { reason? }`(`user:manage`):DELETE user(CASCADE user_role/credentials/identity_link/account_token);拒绝邮件 NTH 不发

#### Policy CRUD — 真新,`routes/registration_policy.rs`(扁平)

- `GET /api/v1/auth/registration-policy`(require `auth:manage`)返单 row
- `PUT /api/v1/auth/registration-policy`(require `auth:manage`)更新 + audit log;校验 role_id 存在且非 owner、domain lowercase + 格式
- 首启 seed 默认行(需先确保 viewer role 已 seed)

> 权限零新增:`auth:manage` + `user:manage` 都已在 `PermissionName`;roles 列表复用已有 `GET /api/v1/roles`(`users.rs`)。

### 3. Admin SPA

- **Settings › Authentication 页**(③ 已落,在 `apps/admin/src/routes/_auth/` 下):底部加 "Registration Policy" ProForm 卡片(6 字段 + Save);顶部条件 banner(`require_email_verify=true` 且 mail fallback_mode → Alert.warning)。role Select 复用 `GET /api/v1/roles`。
- **新建公开页** `register.tsx`(已无)+ 复用已存在的 `verify-email.tsx`(④ 已落,需按 ⑤ 的 `next` 分支跳转)+ `verify-email-sent.tsx`(注册后提示 + 公开 resend)。
- **新建** `_auth/awaiting-approval`(等待审批页,轮询 me query 30s)+ `_auth` guard:`me.status==='pending_approval' && path!=='/awaiting-approval'` → redirect。
- **扩展 Users 页**(④ 已落,`_auth/` 下):status filter 加 pending_approval;行加 Approve(role 覆盖 Modal)/ Reject(reason Modal)。
- `/login` 加 "没有账号?注册" 链接(仅 `allow_self_register_email=true` 时)。

### 4. Audit log events

`user_self_registered` / `user_approved` / `user_rejected` / `registration_policy_updated`(`email_verified` 事件 ④ 已有,沿用)。

## Capabilities

### New Capabilities

- `registration-policy-and-self-register`:注册策略 + 自助注册 + pending_approval 工作流的可观测行为契约

### Modified Capabilities

- 扩展 `add-auth-and-rbac` 的 `user.status` enum:`Invited`→`Provisioned` 改名 + 加 `PendingApproval`(**不**加 `email_verified` bool)
- 扩展 `add-oauth-github-and-provider-config` 的 OAuth callback "新 GitHub user 无冲突" 分支:由硬 401 改为 policy-driven
- 扩展 `add-invite-and-password-reset` 的 `routes/verify_email.rs`:消费成功后对 `Provisioned` 用户做状态转移 + 角色绑定

## Impact

- **Code**:server 新增 `registration_policy` 实体 + CRUD(`routes/registration_policy.rs`)+ `routes/register.rs` + 公开 resend;扩展 `routes/{oauth,verify_email,users}.rs`;`user.status` 加一个变体。admin 新增 `register` / `awaiting-approval` 页 + Settings policy 卡片,扩展 `verify-email` / Users 页。
- **DB**:新增 `registration_policy` 表 + `user.status` `Invited`→`Provisioned` 改名 + 加 `PendingApproval`(**含一次性 raw `UPDATE invited→provisioned`,天然幂等、无 marker 表;无 backfill**)。
- **API**:新增 `/api/v1/auth/{register, verify-email/resend, registration-policy}` + `/api/v1/users/{pending-approval, :id/approve, :id/reject}`。
- **OpenAPI**:drift gate 触发。
- **Deps**:无新增。
- **Mail templates**:`email_verify.{en,zh-CN}` 已存在,仅确认 context 占位(verify_url/expires_at/display_name)。
- **不影响**:CLI / PAT / Storage。

## Non-goals

- 不引入 `pending_verify` 状态(verify 走 `email_verified_at`);但 `Invited` 改名 `Provisioned`(语义纯净,含一次性数据迁移)
- 不加 `email_verified: bool`、不做 active-user backfill(用既有 `email_verified_at`)
- 不实现 "邀请用户必须再 verify email"(邀请 accept 隐式 verify)
- 不实现 OAuth-only user 的 "补设密码"(profile-ui 已处理)
- 不实现拒绝邀请的邮件通知(仅 DB delete + 审计)
- 不实现 SCIM / Workforce Identity Sync / magic-link
- 不实现 "自助注册带 invite token bypass approval" 混合模型

## Depends on

> 以下原 "pending" 依赖**均已 apply + archive**,本 proposal 现可直接基于其 ship 的真实接口实现。

- `add-auth-and-rbac`(archived)—— user / role / permission(`PermissionName::{UserManage, AuthManage}` 已在)
- `add-admin-frontend-foundation`(archived)—— UI 基础 + meQueryOptions
- `add-login-and-owner-bootstrap-ui`(archived)—— bootstrap window + `password::validate_strong_password` + `/login`
- `add-mail-infrastructure`(archived)—— Mailer + `email_verify` 模板(已 seed)
- `add-oauth-github-and-provider-config`(archived)—— `oauth_provider` + callback 钩子(`routes/oauth.rs`)
- `add-invite-and-password-reset`(archived)—— `account_token` 机制 + `routes/verify_email.rs` + `GET /roles` + Users 页

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) Registration Policy 段(新增)
- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) Users 页 pending_approval 状态
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) registration_policy + user.status 状态机(加 `PendingApproval`)
- [dev-notes/knowledge/admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) pending_approval 路由分流

## Acceptance

- Owner → Settings › Authentication 开 `allow_self_register_oauth` + 配 `allowed_email_domains=['mycompany.com']` → 保存(**支柱 A 验收**)
- 陌生 GitHub 用户(verified email @mycompany.com)首次 OAuth → callback 自动建号(按 policy 决定 status / role)→ 登录成功(替代原 401)
- 关 `allow_self_register_oauth` → 陌生 GitHub 用户 callback 维持 401 `oauth_registration_disabled`
- `@other.com` 的 GitHub 用户 → 302 `/login?oauth_error=domain_not_allowed`
- Owner 开 `allow_self_register_email` + `require_email_verify` + `require_approval` → 第三方 `/register` → 收 verify 邮件 → 点链接 → 验证成功(Invited→PendingApproval)→ 跳 `/awaiting-approval`(**支柱 B 验收**)
- Owner 在 Users 页看到 pending_approval 行 → Approve → 第三方 30s 内自动跳 `/`
- 关 `allow_self_register_email` → `/login` 无 "注册" 链接;直访 `/register` → redirect `/login` + Alert
- 邮箱验证启用但 Mail 未配置 → Settings › Authentication 顶部 banner 警示
- `pnpm lint` / `cargo clippy` / `cargo test --workspace` / `pnpm --filter @swarm-hive/admin test` 全绿
- OpenAPI drift gate 通过
