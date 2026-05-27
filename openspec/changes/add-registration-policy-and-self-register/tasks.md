# tasks

## 1. Entity 扩展 + 迁移 hook

- [ ] 1.1 [code] `crates/swarmhive-entity/src/user.rs` UserStatus enum 加 `PendingApproval` 变体；api-types 同步
- [ ] 1.2 [code] `crates/swarmhive-entity/src/user.rs` Model 加 `email_verified: bool` + `email_verified_at: Option<DateTimeUtc>` 字段
- [ ] 1.3 [code] 启动期 migration hook：raw SQL `UPDATE "user" SET status='pending_verify' WHERE status='invited'`；`UPDATE "user" SET email_verified=true WHERE status='active' AND email_verified=false`；用 schema_migrations_log 类似表标记已跑（防重复）
- [ ] 1.4 [code] api-types：`UserStatus` enum 加 `PendingApproval`；DTO 加 `email_verified` 字段

## 2. registration_policy 实体

- [ ] 2.1 [code] 新建 `crates/swarmhive-entity/src/registration_policy.rs`：Model (id i32 PK, allow_self_register_email bool, allow_self_register_oauth bool, require_email_verify bool, self_register_default_role_id Uuid FK role, self_register_require_approval bool, allowed_email_domains Vec<String>, updated_at, updated_by Uuid FK user)
- [ ] 2.2 [code] `lib.rs` 注册；schema-sync 包含
- [ ] 2.3 [code] 启动期 seed：INSERT IF NOT EXISTS id=1 with defaults（全 false / require_email_verify=true / default_role=viewer / require_approval=true / domains=[]）；需要先确保 viewer role 已 seed
- [ ] 2.4 [code] api-types：`RegistrationPolicy*` DTO + utoipa schema

## 3. Server: registration_policy CRUD

- [ ] 3.1 [code] 新建 `crates/swarmhive-server/src/routes/auth/policy.rs`：`GET /api/v1/auth/registration-policy` + `PUT /api/v1/auth/registration-policy`；require `auth:manage`
- [ ] 3.2 [code] PUT handler：校验 role_id 存在 + role.name != 'owner'（禁选 owner 作为 default）+ allowed_email_domains 元素 lowercase + 合法 domain 格式（用 garde 简单校验）+ 写 audit log `registration_policy_updated`
- [ ] 3.3 [code] mount router；utoipa 注解
- [ ] 3.4 [test] integration `policy_crud_smoke.rs`：GET 默认值 + PUT 更新 + 校验失败场景

## 4. Server: /register endpoint

- [ ] 4.1 [code] 新建 `crates/swarmhive-server/src/routes/auth/register.rs`：`POST /api/v1/auth/register { email, display_name, password }`
- [ ] 4.2 [code] 校验链：policy.allow_self_register_email → 410；email 占用 → 422；domain 不匹配 → 422；password 弱 → 422（复用 ① garde）
- [ ] 4.3 [code] 分支创建：require_email_verify=true → user(status=pending_verify) + token + 发邮件 + 返 next:verify_email；否则按 require_approval 决定 status → 写 session
- [ ] 4.4 [code] 写 audit log `user_self_registered`
- [ ] 4.5 [test] integration `register_smoke.rs`：policy 全开 → register → user pending_verify + 邮件 mock 触发；关 policy → 410；domain 限制 → 422

## 5. Server: /verify-email endpoint

- [ ] 5.1 [code] 新建 `crates/swarmhive-server/src/routes/auth/verify_email.rs`：`POST /api/v1/auth/verify-email { token }` + `GET /info` + `POST /resend { email }`
- [ ] 5.2 [code] verify handler：复用 ④ 的 token 验证模块；user.email_verified=true + email_verified_at=now()；按 require_approval 决定 status → INSERT user_role(default_role) → 写 session
- [ ] 5.3 [code] resend handler：始终 200；找到 user.email_verified=false → invalidate active EmailVerify token → gen new → 发邮件；timing 拉平
- [ ] 5.4 [code] mount router；audit log `email_verified`
- [ ] 5.5 [test] integration `verify_email_smoke.rs`：full register → verify → status 转移 + role 绑定

## 6. Server: OAuth callback 自助注册分支接入

- [ ] 6.1 [code] 改 ③ `routes/auth/oauth.rs` callback 中"无现存 user 无冲突"分支：读 policy.allow_self_register_oauth；false → 401；true → domain 校验 → 创 user(status=按 require_approval) + identity_link + user_role(default_role) → 写 session → 302
- [ ] 6.2 [code] domain mismatch → 302 /login?oauth_error=domain_not_allowed
- [ ] 6.3 [code] race condition：UNIQUE user.email 兜底；INSERT 失败 → 302 /login?oauth_error=race_conflict
- [ ] 6.4 [test] 扩 ③ 的 `oauth_smoke.rs`：policy 三种组合 × OAuth 新用户 → 验状态转移

## 7. Server: pending_approval admin endpoints

- [ ] 7.1 [code] 新建 `crates/swarmhive-server/src/routes/users/approval.rs`：`GET /api/v1/users/pending-approval`（分页）+ `POST /api/v1/users/:id/approve { role_id? }` + `POST /api/v1/users/:id/reject { reason? }`；全部 require `user:manage`
- [ ] 7.2 [code] approve handler：user.status='active' + role_id 可选覆盖（UPDATE user_role）+ audit log `user_approved`
- [ ] 7.3 [code] reject handler：DELETE user（CASCADE 把 user_role / user_credentials / identity_link / account_token 全删）+ audit log `user_rejected` 含 reason
- [ ] 7.4 [code] mount router；utoipa 注解
- [ ] 7.5 [test] integration `approval_smoke.rs`：approve + reject 完整流程

## 8. Mail template: email_verify 实际化

- [ ] 8.1 [code] `crates/swarmhive-server/assets/mail-templates/email_verify.{en,zh-CN}.{subject,html,text}`：实际文案（含 verify_url, expires_at 占位）
- [ ] 8.2 [code] register/verify handler 构造 MailEnvelope.context = { verify_url, expires_at, display_name }

## 9. Admin SPA: Settings > Authentication 加 Policy 卡片

- [ ] 9.1 [code] `apps/admin/src/routes/_auth.settings.authentication.tsx` 扩展：底部加 Card "Registration Policy"；ProForm 字段如 spec 列出
- [ ] 9.2 [code] require_email_verify Switch disabled when allow_self_register_email=false（Form.useWatch 联动）
- [ ] 9.3 [code] allowed_email_domains 用 AntD Select mode="tags" tokenSeparators=[',', ' ']；onBlur 强制 lowercase
- [ ] 9.4 [code] role_id Select 从 GET /api/v1/roles 拉列表（如该 endpoint 不存在，本 proposal 加最小版 `GET /api/v1/roles` require `user:manage`）
- [ ] 9.5 [code] Save 按钮 → PUT policy → notification + invalidate query
- [ ] 9.6 [code] 顶部 banner：`useQuery mailStatus + useQuery policy` → `mail.fallback_mode && policy.allow_self_register_email && policy.require_email_verify` → AntD Alert.warning

## 10. Admin SPA: /register

- [ ] 10.1 [code] 新建 `apps/admin/src/routes/register.tsx`：公开；beforeLoad 调 setupInfo + policy；bootstrap 未完 → /setup；policy.allow_self_register_email=false → /login + Alert
- [ ] 10.2 [code] ProForm: email + display_name + password + confirm；strong-password reuse ① zod
- [ ] 10.3 [code] 顶部 i18n 提示文案根据 policy 渲染："注册后将进入待审批"/"注册后请验证邮箱"/"注册后即可使用"
- [ ] 10.4 [code] submit POST /register → 按 response.next 跳转：'verify_email' → /verify-email-sent 提示页（i18n "请查收 X 邮件"）/ 'pending_approval' → /awaiting-approval / 'home' → /
- [ ] 10.5 [code] /login 加 "没有账号？注册" 链接（仅 policy.allow_self_register_email=true 时显示）

## 11. Admin SPA: /verify-email

- [ ] 11.1 [code] 新建 `apps/admin/src/routes/verify-email.tsx`：公开；解析 search.token → query verify-email/info → 渲染确认卡片 + Button
- [ ] 11.2 [code] Button click → POST verify-email → 按 next 跳转
- [ ] 11.3 [code] 新建 `apps/admin/src/routes/verify-email-sent.tsx`：纯文案页（注册后跳到这）+ "未收到？重新发送" 按钮 → POST verify-email/resend → 始终显示通用提示

## 12. Admin SPA: /awaiting-approval + _auth guard 扩展

- [ ] 12.1 [code] 新建 `apps/admin/src/routes/_auth.awaiting-approval.tsx`：Result info 卡 + 手动 "刷新" 按钮 → invalidate me query；自动 useEffect setInterval 30s refetch
- [ ] 12.2 [code] 改 `apps/admin/src/routes/_auth.tsx` beforeLoad：ensureQueryData me → me.user.status==='pending_approval' && location.pathname !== '/awaiting-approval' → throw redirect '/awaiting-approval' replace
- [ ] 12.3 [code] me query data 含 status 字段（schema.gen 自动派生，需确认 server endpoint 返回）

## 13. Admin SPA: Users 页 pending_approval 扩展

- [ ] 13.1 [code] 改 ④ 的 `apps/admin/src/routes/_auth.users.tsx`：status filter 加 'pending_approval' 选项
- [ ] 13.2 [code] 行 actions: pending_approval 行加 Approve / Reject 按钮
- [ ] 13.3 [code] Approve Modal：role Select 默认 policy.default_role_id（可改）→ confirm POST /approve { role_id }
- [ ] 13.4 [code] Reject Modal：reason TextArea 可选 → confirm POST /reject { reason } → 行从列表消失
- [ ] 13.5 [code] OpenAPI drift：跑 `pnpm openapi`

## 14. 测试

- [ ] 14.1 [test] cargo test --workspace 全绿（含 policy + register + verify + approval smoke）
- [ ] 14.2 [test] Vitest pages：mock policy / mail status → 渲染分支检查（banner / register 入口隐藏 / verify 联动）
- [ ] 14.3 [test] Playwright e2e `apps/admin/e2e/self-register-flow.spec.ts`：Owner 开 policy → 第三方 /register → mailpit verify → /awaiting-approval → Owner approve → 第三方刷新 → /
- [ ] 14.4 [test] Playwright e2e `apps/admin/e2e/oauth-self-register-flow.spec.ts`：Owner 开 oauth 自助 + 配 GitHub provider → mock GitHub 新用户 callback → 创账号
- [ ] 14.5 [code] pnpm lint + typecheck 全绿

## 15. Docs / memory 同步

- [ ] 15.1 [docs] `docs/13-rbac.md` 加 "Registration Policy" 段（字段集 + 状态机）
- [ ] 15.2 [docs] `docs/08-admin-and-analytics.md` Users 页加 pending_approval 工作流
- [ ] 15.3 [docs] `dev-notes/knowledge/backend.md` 加 "user.status 状态机" 段（5 态完整图）+ registration_policy 模式
- [ ] 15.4 [docs] `dev-notes/knowledge/admin-spa.md` 加 "pending_approval 路由分流" + "/register 入口可见性 policy-driven"
- [ ] 15.5 [docs] `openspec/changes/README.md` 依赖图加 `add-registration-policy-and-self-register` 节点
- [ ] 15.6 [docs] apply + archive 后删 `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` 整文件（所有 5 个 proposal 收口）

## 16. 端到端验证

- [ ] 16.1 [code] 全栈：Owner → Settings>Authentication 开 allow_self_register_email + require_email_verify + require_approval → 保存
- [ ] 16.2 [code] 第三方 /register → 收 verify 邮件 → 点 → /awaiting-approval（等待页）
- [ ] 16.3 [code] Owner 在 Users 页看到 pending_approval → Approve(role=publisher) → 第三方 30s 内自动跳 /
- [ ] 16.4 [code] policy.allow_self_register_email=false → /login 无 "注册" 链接；直接访问 /register → /login + Alert
- [ ] 16.5 [code] policy.allowed_email_domains=['example.com'] → register x@other.com → 422
- [ ] 16.6 [code] Mail 未配置 + require_email_verify=true → Settings 顶部 banner
- [ ] 16.7 [code] OAuth 自助开 → mock 新 GitHub 用户 → 创账号成功
