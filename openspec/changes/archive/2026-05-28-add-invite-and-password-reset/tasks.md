# tasks

## 1. Config + ENV

- [x] 1.1 [code] `config/default.toml` 加 `[server] base_url = "http://localhost:5173"`；`SwarmServerConfig` struct 加 `base_url: Url`
- [x] 1.2 [code] 启动期校验 `base_url` 解析合法；invite/reset URL 拼接复用 `base_url`

## 2. Entity account_token + user.status enum 扩展

- [x] 2.1 [code] 新建 `crates/swarmhive-entity/src/account_token.rs`：Model (id Uuid PK, purpose enum(Invite/PasswordReset/EmailVerify), user_id Option<Uuid>, token_hash text, token_lookup text, payload Option<Json>, expires_at, consumed_at Option, created_at, created_by Option<Uuid>)
- [x] 2.2 [code] 索引：唯一 `token_hash`（虽然 argon2 含 salt 理论不重复，做防御）+ `(purpose, token_lookup)` 索引 + partial unique `(user_id, purpose) WHERE consumed_at IS NULL` — partial unique 由应用层 `AccountTokenService::issue_replacing` 在事务内保证（sea-orm 2 rc.38 schema-sync `WHERE` 子句 mishandles，与 mail_provider 同样处理）
- [x] 2.3 [code] `crates/swarmhive-entity/src/user.rs` UserStatus enum 加 `PendingVerify` 变体 — 决定复用现有 `Invited` 变体表达 spec 的 pending_verify 语义（`Invited` 之前无业务逻辑分支挂钩，二者等同）
- [x] 2.4 [code] `crates/swarmhive-entity/src/user.rs` 加 `email_verified_at: Option<DateTimeUtc>` 字段（默认 NULL）
- [x] 2.5 [code] api-types `UserStatus` + `User` 同步加（email_verified_at 暴露给前端供 banner 判断）；schema-sync 包含
- [x] 2.6 [code] api-types 加 `AccountToken*` DTO（永不暴露 token_hash / token_lookup） — 实际不需要暴露 token 实体给客户端，所有 token 相关 DTO（IssueResp/InfoResp）就近放在 routes 各 handler 文件内

## 3. Token 加密 + 校验工具

- [x] 3.1 [code] 落点改为 `crates/swarmhive-server/src/services/account_token.rs`（与 mail / audit / seed 并列），`mint()` 产 (plaintext, lookup, hash) 三元组；lookup 改用 blake3 替 sha256（性能 + 抗碰撞同级，依赖更轻）
- [x] 3.2 [code] `AccountTokenService::verify(db, purpose, plaintext)`：先按 (purpose, lookup) SELECT → argon2 verify → 检查 expires_at + consumed_at；`TokenError → ApiError` 集中在 service 内实现
- [x] 3.3 [code] `AccountTokenService::consume(db, token_id)`：UPDATE SET consumed_at=now()；已 consumed 返回 410 Typed
- [x] 3.4 [code] `AccountTokenService::issue_replacing(db, purpose, user_id, ttl, ...)`：事务内 invalidate 已存在的 active token + insert 新 token，单调用替代 `invalidate_active_tokens` + `issue`
- [x] 3.5 [test] 3 个 unit test 在 `services/account_token.rs::tests`：mint 三元组校验、verify expired/consumed/unknown 三分支

## 4. Server: 邀请 endpoint

- [x] 4.1 [code] 落点 `crates/swarmhive-server/src/routes/invite.rs`（顶层 routes/，遵守 backend.md vertical-slice 规范）：handler `POST /api/v1/users/invite`；TX 内 INSERT user + user_role + token；调 dispatch_email
- [x] 4.2 [code] 校验 role.name != 'owner' → 422 cannot_invite_owner
- [x] 4.3 [code] 校验 email 未占用 → 422 email_already_taken
- [x] 4.4 [code] `POST /api/v1/users/invite/{id}/resend` handler：找到 user.status=Invited → invalidate active invite token → gen new → 发邮件
- [x] 4.5 [code] `GET /api/v1/auth/accept-invite/info` 公开 endpoint：返 email, display_name, role_name, inviter_name, expires_at（read-only，不 consume）
- [x] 4.6 [code] `POST /api/v1/auth/accept-invite` 公开 endpoint：verify token → 设密码 (garde strong) → user.status=active → **user.email_verified_at=now()**（点链接已证明邮箱可达）→ mark consumed → 写 session → 200
- [x] 4.7 [code] 全部加 utoipa 注解 + audit log events
- [x] 4.8 [test] integration（`tests/account_token_smoke.rs::invite_then_accept_activates_and_verifies` + `invite_rejects_owner_role_and_duplicate_email` + `resend_invite_invalidates_old_token`）：full invite → accept-invite → user active + email_verified_at 已设 + 登录可用；owner 角色/重复邮箱 422；resend 轮换 token

## 5. Server: 重置密码 endpoint

- [x] 5.1 [code] 落点 `crates/swarmhive-server/src/routes/password_reset.rs`：handler `POST /api/v1/auth/forgot-password`：三分支（不存在 / 存在但未验证 / 存在且已验证）→ 已验证才 invalidate + gen + 发邮件；未验证 silent skip + audit log `password_reset_blocked_unverified`；所有 skip 路径 timing floor `FORGOT_TIMING_FLOOR = 150ms` 防 enumeration
- [x] 5.2 [code] `GET /api/v1/auth/reset-password/info`：返 email, expires_at
- [x] 5.3 [code] `POST /api/v1/auth/reset-password`：TX 内 verify token → upsert credentials → mark consumed → DELETE FROM session WHERE user_id；audit log `password_reset_completed`
- [x] 5.4 [code] mount router 到 sensitive subrouter（governor 速率限制）
- [x] 5.5 [test] integration（`account_token_smoke.rs::forgot_reset_for_verified_user_revokes_old_sessions` + `forgot_password_unverified_is_silently_skipped`）：full forgot → reset → 旧 cookie 调 /me 返 401（session 撤销）+ 新密码可登录 + 旧密码失效；未验证 user forgot 始终 200 不发邮件 + audit blocked_unverified

## 6. Mailer template 实际化

- [x] 6.1 [code] `crates/swarmhive-server/assets/mail-templates/user_invite.{en,zh-CN}.{subject,html,text}`：写实际文案（含 invite_url, inviter_name, role_name, expires_at 占位）
- [x] 6.2 [code] `crates/swarmhive-server/assets/mail-templates/password_reset.{en,zh-CN}.{subject,html,text}`：写实际文案
- [x] 6.3 [code] `crates/swarmhive-server/assets/mail-templates/email_verify.{en,zh-CN}.{subject,html,text}`：写实际文案（含 verify_url, expires_at 占位）
- [x] 6.4 [code] 抽象到 `services::account_token::dispatch_email(state, to, event_name, context)`；三 handler 各传 `{ invite_url, inviter_name, role_name, expires_at }` / `{ reset_url, expires_at }` / `{ verify_url, expires_at }` 直接对齐模板占位
- [x] 6.5 [test] `account_token_smoke.rs` 用 `CapturingMailer`（kind="smtp"）替换 AppState.mailer，从 `invite_url` / `reset_url` / `verify_url` context 提取 `?token=` plaintext 驱动全链路（替代单独的 envelope 字段断言，更端到端）

## 7. Admin SPA: /forgot-password

- [x] 7.1 [code] 新建 `apps/admin/src/routes/forgot-password.tsx`：公开路由；Form email；submit POST forgot-password → 永远显示通用 Result success（不暴露邮箱是否存在）
- [x] 7.2 [code] `/login` "忘记密码" 链接改为 `<Link to="/forgot-password">`（移除 disabled + 占位文案）

## 8. Admin SPA: /reset-password

- [x] 8.1 [code] 新建 `apps/admin/src/routes/reset-password.tsx`：公开路由；validateSearch zod token → query reset-password/info；token 无效 → Result error 页
- [x] 8.2 [code] Form 新密码 + confirm；submit POST reset-password → 跳 `/login` (replace: true)，提示用新密码登录
- [x] 8.3 [code] 密码强度复用 `lib/validation/password.ts` 的 `passwordRules` / `confirmPasswordRules`（setup / reset / accept 三页共用，对齐服务端 password-too-weak）

## 9. Admin SPA: /accept-invite

- [x] 9.1 [code] 新建 `apps/admin/src/routes/accept-invite.tsx`：公开路由；validateSearch zod token → query accept-invite/info
- [x] 9.2 [code] 渲染欢迎卡片（inviter_name + role_name Tag）+ email / display_name 只读字段 + 密码 + confirm
- [x] 9.3 [code] submit POST accept-invite → router.invalidate() 刷新 /me → 跳 `/` (replace: true)

## 10. Admin SPA: /_auth/users 最小版

- [x] 10.0 [code] **新增后端依赖**：`routes/users.rs` 加 `GET /api/v1/users`（list + roles，user:manage gate）+ `GET /api/v1/roles`（invite drawer 角色目录），tag = "users"；spec 原先漏了列表读端点
- [x] 10.1 [code] 新建 `apps/admin/src/routes/_auth/users.tsx`：菜单 user:manage gate；ProTable 列 email/display_name/role(Tag)/status/created_at/操作；status 列 Tag (active/invited/disabled)
- [x] 10.2 [code] "邀请成员" 按钮 → DrawerForm：email + confirm_email（双输入校验）+ role ProFormSelect（exclude Owner）+ display_name 可选；ERR_EMAIL_ALREADY_TAKEN / ERR_CANNOT_INVITE_OWNER 专属提示
- [x] 10.3 [code] 重发邀请按钮（仅 invited 行可见）→ Popconfirm → POST /resend → notification + invalidate users
- [x] 10.4 [code] OpenAPI drift：`pnpm openapi` 改用 `dump-openapi` bin 离线生成（无需起 server）+ 末尾 biome format
- [x] 10.5 [code] `_auth/route.tsx` ProLayout 菜单加 "成员" 入口（user:manage gate，TeamOutlined）

## 11. 测试

- [x] 11.0 [test] `tests/openapi_surface.rs` 补 10 新 path + 3 新 tag (`invite` / `password_reset` / `verify_email`) + 11 新 schema → 5/5 通过
- [x] 11.1 [test] cargo test --workspace 全绿（28 unit + account_token_smoke 9 + 既有 smoke + openapi_surface 5）
- [~] 11.2 [test] Vitest 各 page 渲染分支 — deferred 到 CI（与 add-login / add-mail 既有惯例一致；后端 9 个 integration smoke + 手动验证已覆盖核心链路）
- [~] 11.3 [test] Playwright e2e invite-reset-flow — deferred 到 CI（同上；e2e 与 `account_token_smoke.rs` 全链路覆盖重叠，UI 部分人工已验证）
- [x] 11.4 [code] pnpm lint + typecheck + admin:build 全绿

## 12. Docs / memory 同步

- [x] 12.1 [docs] `docs/13-rbac.md` 加「邀请 / 密码重置 / 邮箱验证」段（三类 token 流程 + 软验证理由）
- [x] 12.2 [docs] `docs/08-admin-and-analytics.md` Users & Roles 页：补邀请抽屉 + status Tag + 重发 + verify banner
- [x] 12.3 [docs] `dev-notes/knowledge/backend.md` 加「account_token 一次性 token」段（单表 + blake3 lookup + argon2 双层 + issue_replacing 不变式 + TokenError 映射）；另 admin-spa.md 加 dump-openapi 离线生成段
- [x] 12.4 [docs] `openspec/changes/README.md` 节点已在依赖图（①②→④）；状态行更新为「apply 完成 / 待归档」
- [x] 12.5 [docs] apply + archive 后删 `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` 中 ④ 段 — archive 时做

## 13. 端到端验证

- [x] 13.1 [code] 本地端到端：Owner 邀请 alice → mailpit → /accept-invite → Alice 登录正常 + email_verified_at 已 set（用户手动验证通过；smoke `invite_then_accept_activates_and_verifies` 自动覆盖）
- [x] 13.2 [code] 忘记密码 → 重置 → 旧 session 失效（用户手动验证通过；smoke `forgot_reset_for_verified_user_revokes_old_sessions` 自动覆盖：旧 cookie 调 /me 返 401）
- [x] 13.3 [code] /forgot-password 填不存在 email → 通用提示 + timing floor（`FORGOT_TIMING_FLOOR = 150ms` 服务端实现，smoke 验证 200 + 不发邮件）
- [x] 13.4 [code] 已失效 invite token → 错误页（smoke `resend_invite_invalidates_old_token`：旧 token 返 410/404；前端 accept-invite Result error 页）
- [x] 13.5 [code] owner banner 未验证 → 重发 → /verify-email → 验证 → banner 消失（smoke `verify_email_send_consume_then_idempotent_gone` 覆盖后端链路；前端 VerifyBanner + verify-email 页已接 useResendVerify / invalidate meQuery）
- [x] 13.6 [code] 未验证 owner forgot 自己邮箱 → 200 但不发邮件 + `password_reset_blocked_unverified`（smoke `forgot_password_unverified_is_silently_skipped`）
- [x] 13.7 [code] ConsoleMailer fallback → verify-email/send 返 422 `mail_not_configured` + expected_next_step（smoke `verify_email_send_requires_configured_smtp`；前端 banner fallback 分支切 [配置 SMTP]）
- [x] 13.8 [code] 60s 内连发两次 verify-email/send → 第二次 429 `rate_limited`（smoke `verify_email_send_is_rate_limited_within_window`）

## 14. Server: 邮箱验证 endpoint

- [x] 14.1 [code] 落点 `crates/swarmhive-server/src/routes/verify_email.rs`（与 invite/password_reset 同层，三 handler 同文件）：handler `POST /api/v1/users/me/verify-email/send`（require authenticated session）
  - 当前 user 已 verified → 422 `email_already_verified`
  - 读 `state.mailer.read().mailer().kind()`；console → 422 `mail_not_configured`，body 含 `expected_next_step: "/settings/mail"`
  - 查现有 active EmailVerify token；若 created_at > now-60s → 429 `rate_limited`
  - invalidate 旧 active → gen new (24h expires) → 调 `dispatch_email("email_verify", to, { verify_url, expires_at })`
  - 写 audit log `email_verify_sent`
- [x] 14.2 [code] 同文件加：
  - `GET /api/v1/auth/verify-email/info?token=` 公开 read-only：返 `{ email, expires_at }`；invalid → 410 / 404
  - `POST /api/v1/auth/verify-email { token }` 公开（无 session 要求）：verify token → UPDATE user SET email_verified_at=now() WHERE id=? AND email_verified_at IS NULL → mark consumed → audit log `email_verified` → 200
- [x] 14.3 [code] mount router；utoipa 注解；ApiErrorResponses
- [x] 14.4 [test] integration（`account_token_smoke.rs::verify_email_send_consume_then_idempotent_gone` + `verify_email_send_is_rate_limited_within_window` + `verify_email_send_requires_configured_smtp`）：full send → consume → email_verified_at 已设 + 二次 consume 410；ConsoleMailer 模式 send 返 422 + expected_next_step；60s 内重发 429

## 15. Admin SPA: verify banner

- [x] 15.1 [code] 改 `apps/admin/src/routes/_auth/route.tsx`：AuthLayout 顶部加 `VerifyBanner` 组件，条件 `me.data != null && me.data.user.email_verified_at == null`
- [x] 15.2 [code] banner 分支：根据 `mailStatus.fallback_mode` 切换文案 + action
  - fallback → "邮箱未验证 + 邮件未配置，请先 [配置 SMTP]"，action 是 `<Link to="/settings/mail">`；"重发验证"按钮隐藏
  - 否则 → "你的邮箱 `{email}` 尚未验证，[重发验证邮件]"，action 调 `useResendVerify`
- [x] 15.3 [code] 不可 dismiss（不传 `closable`）；样式与 mail fallback banner 一致（top yellow Alert.banner）
- [x] 15.4 [code] 重发逻辑抽到 `lib/query/useResendVerify.ts`：success → notification.success；失败按 ApiError type 三分支（email_already_verified / rate_limited / mail_not_configured 专属文案）
- [x] 15.5 [code] already_verified 分支 invalidate meQuery；verify-email 页成功也 invalidate，让 banner 自然消失

## 16. Admin SPA: /verify-email 页

- [x] 16.1 [code] 新建 `apps/admin/src/routes/verify-email.tsx`：公开路由；validateSearch zod token → query verify-email/info
- [x] 16.2 [code] 渲染卡片 + 显示 email 只读；单 primary button "确认验证"（倒计时省略，info 仅校验有效性）
- [x] 16.3 [code] submit POST verify-email → 成功 invalidate meQuery → 跳 `/` (replace: true)
- [x] 16.4 [code] token 410 / 404 → Result error 页 "链接已失效，请到 dashboard 顶部 banner 重发验证"

## 17. Admin SPA: Settings 账户 tab

- [x] 17.1 [code] 改 `_auth/route.tsx` settingsRoute：menu 加 "账户" 子项（置于 Mail 之上）；Account 人人可见，Mail/Auth/Storage/Telemetry 仍 mail:manage gate；`settings/index.tsx` redirect 改 `/settings/account`
- [x] 17.2 [code] 新建 `apps/admin/src/routes/_auth/settings/account.tsx`：ProDescriptions 显示 email + display_name + verified status Tag（success / 灰色未验证）+ "重发验证邮件" 按钮 + 验证时间
- [x] 17.3 [code] 重发按钮共用 `useResendVerify` hook（banner 与 account tab 同源）
