# tasks

## 1. Config + ENV

- [ ] 1.1 [code] `config/default.toml` 加 `[server] base_url = "http://localhost:5173"`；`SwarmServerConfig` struct 加 `base_url: Url`
- [ ] 1.2 [code] 启动期校验 `base_url` 解析合法；invite/reset URL 拼接复用 `base_url`

## 2. Entity account_token + user.status enum 扩展

- [ ] 2.1 [code] 新建 `crates/swarmhive-entity/src/account_token.rs`：Model (id Uuid PK, purpose enum(Invite/PasswordReset/EmailVerify), user_id Option<Uuid>, token_hash text, token_lookup text, payload Option<Json>, expires_at, consumed_at Option, created_at, created_by Option<Uuid>)
- [ ] 2.2 [code] 索引：唯一 `token_hash`（虽然 argon2 含 salt 理论不重复，做防御）+ `(purpose, token_lookup)` 索引 + partial unique `(user_id, purpose) WHERE consumed_at IS NULL`
- [ ] 2.3 [code] `crates/swarmhive-entity/src/user.rs` UserStatus enum 加 `PendingVerify` 变体
- [ ] 2.4 [code] api-types `UserStatus` 同步加；schema-sync 包含
- [ ] 2.5 [code] api-types 加 `AccountToken*` DTO（永不暴露 token_hash / token_lookup）

## 3. Token 加密 + 校验工具

- [ ] 3.1 [code] 新建 `crates/swarmhive-server/src/auth/tokens.rs`：`generate_token() -> (plaintext, lookup, hash)`；plaintext = base64url(rand 32B)，lookup = base64(sha256(plaintext)[..16])，hash = argon2(plaintext)
- [ ] 3.2 [code] `verify_token(plaintext, db, purpose) -> Result<account_token::Model, TokenError>`：先按 (purpose, lookup) SELECT → argon2 verify → 检查 expires_at + consumed_at
- [ ] 3.3 [code] `consume_token(db, token_id) -> Result<()>`：UPDATE SET consumed_at=now()；返回 已 consumed 错误
- [ ] 3.4 [code] `invalidate_active_tokens(db, user_id, purpose)`：UPDATE 已存在的 active token 设 consumed_at
- [ ] 3.5 [test] unit `tokens::tests::roundtrip` + `expired_token_rejected` + `consumed_token_rejected` + `unknown_lookup_returns_not_found`

## 4. Server: 邀请 endpoint

- [ ] 4.1 [code] 新建 `crates/swarmhive-server/src/routes/users/invite.rs`：handler `POST /api/v1/users/invite`；TX 内 INSERT user + user_role + token + audit_log；调 Mailer
- [ ] 4.2 [code] 校验 role.name != 'owner' → 422 cannot_invite_owner
- [ ] 4.3 [code] 校验 email 未占用 → 422 email_already_taken
- [ ] 4.4 [code] `POST /api/v1/users/invite/:id/resend` handler：找到 user.status=pending_verify → invalidate active invite token → gen new → 发邮件
- [ ] 4.5 [code] `GET /api/v1/auth/accept-invite/info` 公开 endpoint：返 email, display_name, role_name, inviter_name, expires_at（read-only，不 consume）
- [ ] 4.6 [code] `POST /api/v1/auth/accept-invite` 公开 endpoint：verify token → 设密码 (garde strong) → user.status=active → mark consumed → 写 session → 200
- [ ] 4.7 [code] 全部加 utoipa 注解 + audit log events
- [ ] 4.8 [test] integration `invite_smoke.rs`：full invite → accept-invite → user active + session 写入

## 5. Server: 重置密码 endpoint

- [ ] 5.1 [code] 新建 `crates/swarmhive-server/src/routes/auth/password_reset.rs`：handler `POST /api/v1/auth/forgot-password`：找 user → invalidate active reset token → gen new + 发邮件；timing-equalising sleep 150ms 防 enumeration
- [ ] 5.2 [code] `GET /api/v1/auth/reset-password/info`：返 email, expires_at
- [ ] 5.3 [code] `POST /api/v1/auth/reset-password`：verify token → 设密码 → mark consumed → DELETE FROM session WHERE user_id → 写新 session → audit log
- [ ] 5.4 [code] mount router
- [ ] 5.5 [test] integration `password_reset_smoke.rs`：full forgot → reset → 旧 session 全失效 + 新 session 可用

## 6. Mailer template 实际化

- [ ] 6.1 [code] `crates/swarmhive-server/assets/mail-templates/user_invite.{en,zh-CN}.{subject,html,text}`：写实际文案（含 invite_url, inviter_name, role_name, expires_at 占位）
- [ ] 6.2 [code] `crates/swarmhive-server/assets/mail-templates/password_reset.{en,zh-CN}.{subject,html,text}`：写实际文案
- [ ] 6.3 [code] 邀请 handler 构造 MailEnvelope.context = { invite_url, inviter_name, role_name, expires_at }；reset 同理
- [ ] 6.4 [test] integration mock Mailer 捕获 envelope → 校验 context 字段完整

## 7. Admin SPA: /forgot-password

- [ ] 7.1 [code] 新建 `apps/admin/src/routes/forgot-password.tsx`：公开路由；ProForm email；submit POST forgot-password → 永远显示通用提示
- [ ] 7.2 [code] `/login` 取消 "忘记密码" 链接 disabled，link 到 `/forgot-password`

## 8. Admin SPA: /reset-password

- [ ] 8.1 [code] 新建 `apps/admin/src/routes/reset-password.tsx`：公开路由；解析 search.token → query reset-password/info；token 无效 → Result error 页
- [ ] 8.2 [code] ProForm 新密码 + confirm；submit POST reset-password → 跳 `/` (replace: true)
- [ ] 8.3 [code] 密码强度复用 ① 的 zod schema

## 9. Admin SPA: /accept-invite

- [ ] 9.1 [code] 新建 `apps/admin/src/routes/accept-invite.tsx`：公开路由；解析 search.token → query accept-invite/info
- [ ] 9.2 [code] 渲染欢迎卡片（i18n 化）+ email 只读字段 + ProForm 密码 + confirm
- [ ] 9.3 [code] submit POST accept-invite → 跳 `/` (replace: true)

## 10. Admin SPA: /_auth/users 最小版

- [ ] 10.1 [code] 新建 `apps/admin/src/routes/_auth.users.tsx`：permission gate user:manage；ProTable 列 email/display_name/role/status/created_at；status 列加 Tag (active/disabled/pending_verify) + status filter
- [ ] 10.2 [code] "Invite member" 按钮 → ProDrawerForm：email + confirm_email（双输入校验）+ role Select（exclude Owner）+ display_name 可选
- [ ] 10.3 [code] Resend invite 按钮（仅 pending_verify 行可见）→ confirm modal → POST /resend → notification
- [ ] 10.4 [code] OpenAPI drift：跑 `pnpm openapi`
- [ ] 10.5 [code] `__root.tsx` ProLayout 菜单加 "Users" 入口

## 11. 测试

- [ ] 11.1 [test] cargo test --workspace 全绿
- [ ] 11.2 [test] Vitest 各 page：mock token info → 渲染分支（有效 / 过期 / 未找到）
- [ ] 11.3 [test] Playwright e2e `apps/admin/e2e/invite-reset-flow.spec.ts`：full invite → mailpit 拿 link → accept → 登录；forgot → mailpit 拿 link → reset → 旧 session 失效
- [ ] 11.4 [code] pnpm lint + typecheck 全绿

## 12. Docs / memory 同步

- [ ] 12.1 [docs] `docs/13-rbac.md` 补 invite + reset 流程描述
- [ ] 12.2 [docs] `docs/08-admin-and-analytics.md` Users 页：补邀请 + status filter
- [ ] 12.3 [docs] `dev-notes/knowledge/backend.md` 加 "account_token 模式" 段（单表 + lookup + argon2 双层）
- [ ] 12.4 [docs] `openspec/changes/README.md` 依赖图加 `add-invite-and-password-reset` 节点
- [ ] 12.5 [docs] apply + archive 后删 `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` 中 ④ 段

## 13. 端到端验证

- [ ] 13.1 [code] 本地端到端：Owner 登 → Users 邀请 alice → mailpit :8025 看邮件 → 点链接 → /accept-invite → 设密码 → 跳 / → Alice 登录正常
- [ ] 13.2 [code] Alice 登出 → /login → 忘记密码 → 填 alice email → mailpit 看邮件 → 重置 → 旧 session 失效（用另一浏览器开 Alice 的旧 tab 刷新 → 跳 /login）
- [ ] 13.3 [code] /forgot-password 填不存在的 email → UI 仍显示通用提示 + 响应时间 ≥150ms
- [ ] 13.4 [code] 已过期 invite token 访问 /accept-invite → 显示错误页 "邀请已过期"
