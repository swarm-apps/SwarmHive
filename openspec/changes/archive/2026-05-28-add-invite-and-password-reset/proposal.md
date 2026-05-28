# add-invite-and-password-reset

## Why

① 落地后 web 端能登录但只有 Owner 一个账号 + 没法换密码；② 落地后能发邮件但没有业务流消费它。整个"协作"维度在 ①② 之后还差**三条**最关键的链路：

- **邀请新成员**：Owner / admin 在后台填 email + 选角色 → 系统发邀请邮件 → 被邀人点链接 → 设密码 → 自动登录加入 org
- **忘记密码**：任意已激活 user 在 /login 点 "忘记密码" → 填邮箱 → 收 reset 邮件 → 点链接 → 设新密码 → 自动登录
- **Owner 邮箱验证**（探索拍板新增）：① 的 setup 流程不验证 owner 邮箱可达性，若 owner 在 setup 时 typo email（如 `@gmial.com`），后续密码重置邮件发到错地址永远收不到 → 唯一救援是 DB 直改。本 proposal 加 verify 流闭环这个事故路径。

没有前两条，"非 Owner 的成员"永远没有自助途径加入或自救（密码忘了只能 Owner 在 admin 里手动改，体验差）。没有第三条，①+② 给的"自助救援"承诺打了折扣（救援链路本身可能因为 email 填错而断掉）。

本 proposal 是 ②（mail）的"第一个消费者"（一次承接 invite / reset / verify 三类邮件流），也是 ⑤（self-register policy）的前置 —— policy 中"自助注册者邮箱验证"会复用同一套 `EmailVerify` token purpose + verify endpoint。

## What Changes

### 1. 实体（新增 + 扩展）

- `account_token`：单表存所有一次性 token（邀请 / 重置 / 验证），id Uuid PK、purpose enum(`Invite` | `PasswordReset` | `EmailVerify`)、user_id Option<Uuid>（reset / verify 必填，invite 为 null 因 user 还没创建）、`token_hash`（argon2 hash 形式存，不存明文）、payload Option<JSON>（invite 时存 email / role_id；其他场景留扩展）、expires_at、consumed_at Option<DateTimeUtc>、created_at、created_by Option<Uuid>（invite 时填谁发的）
  - 唯一索引 `token_hash`
  - 索引 `(user_id, purpose) WHERE consumed_at IS NULL`（一个 user 仅能有一个 active token per purpose，新发自动 invalidate 旧的）
- `user.status` enum 增 `pending_verify`（被邀请人尚未设密码的状态，区别于已 active）
  - 注：本 proposal 加 `pending_verify`；⑤ 再加 `pending_approval`
- `user.email_verified_at` 新增 `Option<DateTimeUtc>` 字段（与 status 正交：status 管账号生命周期，email_verified_at 管邮箱真实性）
  - Owner setup 完成时 = NULL（仍需手动 verify）
  - Invitee accept invite 完成时 = now()（点链接已证明邮箱可达）
  - Verify token consume 完成时 = now()

### 2. Server endpoints

#### 邀请流

- `POST /api/v1/users/invite`（require `user:manage`）：body `{ email, role_id, display_name? }`
  - 校验 email 未占用 → 返 422 `email_already_taken` 否则继续
  - INSERT user(status=`pending_verify`, email, display_name ?? email_local_part) + INSERT user_role + gen token + 写 `account_token(purpose=Invite, user_id=new_user.id, payload={role_id}, expires_at=now+72h)`
  - 调 `Mailer::send(MailEnvelope { event_name: 'user_invite', to: email, context: { invite_url, inviter_name, role_name } })`
  - 返 200 `{ user_id, expires_at }`
- `POST /api/v1/users/invite/:id/resend`（require `user:manage`）：原 token 还没 consumed → invalidate（set consumed_at=now()）+ gen new token + 重发邮件
- `GET /api/v1/auth/accept-invite/info?token=<plaintext>`：返 `{ email, display_name, role_name, inviter_name, expires_at }`；token 过期 / consumed → 410；不存在 → 404；用于 /accept-invite 页面预填信息
- `POST /api/v1/auth/accept-invite`：body `{ token, password }`
  - 查 token → 验过期 / consumed → 取关联 user_id → 设 password（user_credentials INSERT/UPDATE，garde 强校验）→ 改 user.status=`active` → mark consumed_at → 写 session → 200

#### 重置密码流

- `POST /api/v1/auth/forgot-password`：body `{ email }`
  - **始终返 200**（防 email 枚举）
  - 后端：查 user WHERE email →
    - 不存在 → 不发邮件，timing-equalising sleep ≥150ms 后返 200
    - 存在且 `email_verified_at IS NULL` → **不发邮件**（硬阻塞：未验证邮箱不能 reset，否则填错邮箱的事故无解），timing-equalise 后返 200；server warn 日志记 `password_reset_blocked_unverified`
    - 存在且已 verified → invalidate 已有 active reset token → gen new token → 写 account_token(purpose=PasswordReset, user_id, expires_at=now+1h) → 发 `password_reset` 邮件
- `GET /api/v1/auth/reset-password/info?token=<plaintext>`：返 `{ email, expires_at }`；token 无效 → 410
- `POST /api/v1/auth/reset-password`：body `{ token, password }`
  - 验 token → 查 user → 设新密码（user_credentials UPDATE + password_changed_at=now()）→ mark consumed → invalidate 用户所有 active session（DELETE session WHERE user_id 防 stolen session 继续生效）→ 写新 session → 200

#### Owner email 验证流（新增）

- `POST /api/v1/users/me/verify-email/send`（require authenticated session）：
  - 当前 user 已 `email_verified_at != NULL` → 422 `email_already_verified`
  - 检查 `mailer.kind() == "smtp"`，若 fallback_mode → 422 `mail_not_configured`，body 含 `expected_next_step: "/settings/mail"`（admin SPA 拿去引导）
  - invalidate 该 user 已有 active `EmailVerify` token → gen new + expires_at=now+24h → 发 `email_verify` 邮件
  - 60s 内重发限速（query 现有 verify token created_at；过近 → 429 `rate_limited`）
- `GET /api/v1/auth/verify-email/info?token=<plaintext>`：返 `{ email, expires_at }`；token 无效 → 410（read-only，不 consume）
- `POST /api/v1/auth/verify-email`：body `{ token }`
  - 验 token → 取 user_id → UPDATE user SET email_verified_at=now() WHERE id=? AND email_verified_at IS NULL → mark consumed → 200
  - 不要求登录态（点邮件链接的用户可能在另一个浏览器；token 本身已是凭证）

### 3. Admin SPA: routes

- `/login` 取消"忘记密码"链接的 disabled 状态，link 到 `/forgot-password`
- 新建 `/forgot-password`：表单 email → POST forgot-password → 永远显示 "如果该邮箱存在，重置邮件已发送" 文案
- 新建 `/reset-password`：解析 search.token → 调 reset-password/info 校验 → ProForm 新密码 + confirm → POST reset-password → 跳 `/`
- 新建 `/accept-invite`：解析 search.token → 调 accept-invite/info 渲染欢迎卡片（"Hi {email}，{inviter} 邀请你以 {role} 加入 SwarmHive"）+ ProForm 设密码 → POST accept-invite → 跳 `/`
- 改 `_auth.users.tsx`（暂未实现，本 proposal 包含最小版 Users 页）：ProTable 列出 users + "邀请新成员" 按钮 → ProDrawerForm（email + role + display_name?）→ POST invite → notification "邀请已发送"
- "Resend invite" 按钮（每行，仅 pending_verify 状态可见）→ POST resend
- 新建 `/verify-email` 公开路由：解析 search.token → query verify-email/info → 显示 "验证邮箱 `owner@xxx.com`" 卡片 + 单 button → POST verify → 跳 `/` (replace: true)
- 改 `_auth/route.tsx` ProLayout 上方加 **verify banner**：当 `me.email_verified_at IS NULL` 时持续显示（不可 dismiss）：
  - 若 `mailStatus.fallback_mode == true` → banner 文案 "邮箱未验证 + 邮件未配置，请先 [配置 SMTP]"，action 链到 `/settings/mail`；"重发验证"按钮隐藏
  - 否则 banner 文案 "你的邮箱 `owner@xxx.com` 尚未验证，[重发验证邮件]"，action 调 `/users/me/verify-email/send`
- 改 `/settings`（① 留位）：新增 "账户" tab，显 email + verified status (Tag) + "重发验证邮件" 按钮（与 banner 入口冗余但 Profile 是惯例位）
- 全文案 `<Trans>` 包裹

### 4. 邮件模板内容

② 已 seed 4 个模板的占位；本 proposal 把 `user_invite` / `password_reset` / `email_verify` 三个模板的实际内容写实：

- `user_invite`：
  - subject `{{ inviter_name }} 邀请你加入 SwarmHive`
  - body 含 `{{ invite_url }}`、`{{ role_name }}`、`{{ expires_at }} 前接受`
- `password_reset`：
  - subject `SwarmHive - 密码重置请求`
  - body 含 `{{ reset_url }}`、`如非你本人操作请忽略`、`{{ expires_at }} 后失效`
- `email_verify`（owner 自验证 + ⑤ self-register 共用）：
  - subject `SwarmHive - 请验证你的邮箱`
  - body 含 `{{ verify_url }}`、`如非你本人操作请忽略`、`{{ expires_at }} 后失效`

### 5. URL 约定

- 邀请链接：`<admin_base_url>/accept-invite?token=<plaintext>`
- 重置链接：`<admin_base_url>/reset-password?token=<plaintext>`
- `admin_base_url` 从 `[server] base_url` config 读，默认 `http://localhost:5173`；生产部署写明域名

### 6. Audit log

新增 event：`user_invited` / `invite_accepted` / `invite_resent` / `password_reset_requested` / `password_reset_completed` / `password_reset_blocked_unverified` / `email_verify_sent` / `email_verified`

## Capabilities

### New Capabilities

- `invite-and-password-reset`：邀请用户接受流 + 忘记密码 / 重置密码流 + 一次性 token 管理 + UI 完整端到端的可观测行为契约

### Modified Capabilities

- 扩展 `user.status` enum 加 `pending_verify`（archived spec 不变，本 proposal specs 显式 ADDED Requirements 覆盖）

## Impact

- **Code**：server `routes/auth/{accept_invite, password_reset, verify_email}.rs` + `routes/users/{invite, verify_email_send}.rs` + `account_token` entity；admin SPA 新增 5 页（forgot-password, reset-password, accept-invite, verify-email, _auth.users）+ Users 邀请 drawer + `_auth/route.tsx` verify banner + Settings 账户 tab
- **DB**：新增 `account_token` 表；`user.status` enum 加 'pending_verify'；`user` 加 `email_verified_at: Option<DateTimeUtc>` 字段
- **API**：新增 `/api/v1/auth/{forgot-password, reset-password, reset-password/info, accept-invite, accept-invite/info, verify-email, verify-email/info}` + `/api/v1/users/invite` + `/api/v1/users/invite/:id/resend` + `/api/v1/users/me/verify-email/send`
- **Mail templates**：`user_invite` + `password_reset` + `email_verify` 三模板写实
- **Config**：新增 `[server] base_url`（默认 `http://localhost:5173`，生产必填）
- **OpenAPI**：drift gate 触发
- **Deps**：无新增（复用 ① ② 的依赖）
- **不影响**：CLI / PAT / Storage / RBAC role schema / OAuth

## Non-goals

- 不实现自助注册（/register UI 留 ⑤）—— 本 proposal 邀请是 admin 单边动作
- 不实现修改密码（已登录用户在 Profile 改）—— 留后续 profile-ui proposal
- 不实现"修改 email"（owner 改 email 后重新触发 verify）—— 留 profile-ui proposal；本 proposal 假设 setup 时的 email 不再变
- 不实现"批量邀请"（一次邀请一个 email）—— NTH
- 不实现邀请撤销（Owner 直接 DELETE user 即可，副作用：account_token CASCADE delete）
- 不实现"verify token 用 OTP 验证码"形态 —— 统一用 URL token，与 invite/reset 一致

## Depends on

- `add-auth-and-rbac`（archived）—— provide user/user_credentials/user_role/permission/audit_log
- `add-admin-frontend-foundation`（archived）—— provide ProTable / ProForm / $api / 错误链
- `add-login-and-owner-bootstrap-ui`（①，pending）—— provide /login UI 容器 + 密码强度 garde / zod 复用
- `add-mail-infrastructure`（②，pending）—— provide Mailer trait + user_invite/password_reset template + ConsoleMailer dev

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) 用户管理段：补 invite 流程
- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) Users 页
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) 补 account_token 模式
- [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md) ④ 段

## Acceptance

- Owner 登 admin → Users 页 → 邀请 `alice@x.com` 选 publisher → 邮件投递（dev mailpit 可见）→ Alice 点链接 → /accept-invite 页面显示 "Owner 邀请你以 publisher 加入" → 设密码 → 跳 / → Alice 已登录且权限正确，**且 user.email_verified_at 自动 = consume 时间**
- Alice 登出 → /login 点 "忘记密码" → 填邮箱 → 显示 "如果该邮箱存在..." 文案 → 邮件投递 → Alice 点链接 → /reset-password 设新密码 → 跳 / → 已登录
- 用错密码 token / 过期 token / 已 consumed token → /accept-invite 或 /reset-password 显示 410 错误页 "邀请已过期，请联系 Owner 重发"
- `/forgot-password` 提交不存在的 email → 返回 200，无邮件发出，UI 仍显示通用提示（不暴露 email 是否存在）
- 邀请已 pending_verify 的 user → "Resend invite" 重发邮件 + 旧 token 失效
- 密码重置成功 → 该 user 之前所有 session 失效（旧浏览器 tab 下次请求 401）
- account_token.token_hash 用 argon2 存（DB 直查不出明文 token）
- **Owner 完成 setup（① 流程）→ 进 dashboard 顶部显持续 banner "邮箱未验证"**
- **Owner 在 mailpit 可达环境点 banner [重发验证邮件] → mailpit 收到 email_verify 邮件 → 点链接 → /verify-email 显欢迎 → 跳 / → banner 消失 + user.email_verified_at 已设置**
- **未验证 owner 走 /forgot-password 流程 → 始终 200 + 不发邮件（硬阻塞）+ server 日志含 `password_reset_blocked_unverified`**
- **ConsoleMailer fallback 模式下 owner 看到的 banner 变为 "邮件未配置，请先配置 SMTP" + action 跳 /settings/mail；后端 verify-email/send endpoint 返 422 `mail_not_configured`**
- **同一 user 60s 内连发两次 verify-email/send → 第二次返 429 `rate_limited`**
- `pnpm lint` / `cargo clippy` / `cargo test --workspace` / `pnpm --filter @swarmhive/admin test` 全绿
- OpenAPI drift gate 通过
