# tasks

> **Rebased 2026-06-10**：与真实 ship 的 ①②③④ 对齐,并按**支柱 A(policy + OAuth 自助)优先、
> 可独立 apply 验收**重排。原稿的 `pending_verify` / `email_verified` bool / backfill / marker 表全部移除;
> `Invited`→`Provisioned` 改名(语义纯净,用户拍板,含一次性 raw 数据迁移);verify-email 与邮件模板
> 降为"扩展/复用";路径改扁平。

## 0. 共享基础:Invited→Provisioned 改名 + 加 PendingApproval + 一次性迁移

- [x] 0.1 [code] `crates/swarmhive-entity/src/user.rs`:`Invited`→`Provisioned`(`string_value="invited"`→`"provisioned"`)+ 加 `PendingApproval`(`string_value="pending_approval"`);两个 `From` 臂同步
- [x] 0.2 [code] `crates/swarmhive-api-types/src/user.rs`:`UserStatus::Invited`→`Provisioned`(`rename="provisioned"`)+ 加 `PendingApproval`(`rename="pending_approval"`)
- [x] 0.3 [code] rename 调用点:`routes/invite.rs:170`(set)+ `:252`(check)`UserStatus::Invited`→`Provisioned`;doc 注释 `auth/service.rs:120`、`account_token.rs:5`
- [x] 0.4 [code] **一次性数据迁移**(2026-06-10 二次修订):新建 **`swarmhive-migration` crate**(sea-orm-migration =2.0.0-rc.38,不依赖 entity),`m20260610_000001_rename_invited_to_provisioned`(DO block 容忍表未建);`db::run_migrations` dev 经 `sync_schema` 内联、生产经 bin else 分支**无条件执行**——修掉初版 `db.rs::migrate_data` 被 `auto_sync` gate 包住导致生产不迁移的 bug;回归 `db_smoke::invited_rows_are_migrated_once`
- [x] 0.5 [code] admin `apps/admin/src/routes/_auth/users.tsx:129,156`:`status==="invited"`→`"provisioned"`(显示 + tag + i18n label);**不动** `settings/mail/templates.tsx` 的 `invited_by`(邮件变量,与 status 无关)
- [x] 0.6 [test] `account_token_smoke.rs:284`(`UserStatus::Invited`→`Provisioned`)+ `:510`(wire `"invited"`→`"provisioned"`)
- [x] 0.7 [code] **不**加 `email_verified` bool、**不**写 backfill——`email_verified_at` 已是 verify 信号(`From<&Model> for api::User` 已含,无需改)

---

## 支柱 A:Policy + OAuth 自助注册（可独立 apply）

## 1. registration_policy 实体 + seed

- [x] 1.1 [code] 新建 `crates/swarmhive-entity/src/registration_policy.rs`:Model(`id i32 PK`, `allow_self_register_email bool`, `allow_self_register_oauth bool`, `require_email_verify bool`, `self_register_default_role_id Uuid` FK role, `self_register_require_approval bool`, `allowed_email_domains Json`(`Vec<String>`,仿 `oauth_provider.scopes`), `updated_at`, `updated_by Uuid` FK user)
- [x] 1.2 [code] `entity/src/lib.rs` 注册;schema-sync 自动包含
- [x] 1.3 [code] 启动期 seed:INSERT IF NOT EXISTS id=1(全 false / require_email_verify=true / default_role=viewer role id / require_approval=true / domains=[])——走现有 `services/seed.rs` 模式,**需在 viewer role seed 之后**
- [x] 1.4 [code] api-types:`RegistrationPolicy` view + `UpdateRegistrationPolicyReq` DTO + utoipa schema

## 2. Server: registration_policy CRUD

- [x] 2.1 [code] 新建 `crates/swarmhive-server/src/routes/registration_policy.rs`(**扁平**):`GET /api/v1/auth/registration-policy` + `PUT /api/v1/auth/registration-policy`;`require_permission!(.., AuthManage, Scope::None)`
- [x] 2.2 [code] PUT 校验:role_id 存在且 `role.name != 'owner'`(禁 owner 当 default)+ `allowed_email_domains` 元素 lowercase + 合法 domain 格式(garde)+ 写 audit `registration_policy_updated`(set `updated_by`)
- [x] 2.3 [code] mount router(`build_router` + `openapi_router`);utoipa 注解;handler 函数名避免与既有模块撞(operationId 全局唯一,见 backend.md)
- [x] 2.4 [test] integration `registration_policy_smoke.rs`:GET 默认值 + PUT 更新 + 非 owner 403 + role/domain 校验失败

## 3. Server: OAuth callback 自助注册分支（改 routes/oauth.rs）

- [x] 3.1 [code] 改 `routes/oauth.rs` callback "无现存 user、无冲突" 分支(现 `:319-325` 硬 401):读 `policy.allow_self_register_oauth`;false → 维持 401 `oauth_registration_disabled`
- [x] 3.2 [code] true 分支:domain 校验(白名单非空时)→ 不匹配 302 `/login?oauth_error=domain_not_allowed`;通过 → 创 `user(status=Active|PendingApproval 看 require_approval, email_verified_at=now())` + `identity_link` + `user_role(default_role)` → 写 session(复用 `service::establish_session`)→ 302 `/` 或 `/awaiting-approval`
- [x] 3.3 [code] race:`user.email` 唯一约束 INSERT fail → 302 `/login?oauth_error=race_conflict`;audit `user_self_registered`
- [x] 3.4 [test] 扩 ③ 的 `oauth_smoke.rs`(wiremock GitHub):policy 三组合 × OAuth 新用户 → 验 status / role / identity_link;disabled → 401

## 4. Admin SPA: Settings › Authentication 加 Policy 卡片

- [x] 4.1 [code] 在 ③ 的 Settings › Authentication 页(`apps/admin/src/routes/_auth/` 下,OAuth ProTable 之下)加 Card "Registration Policy" ProForm,字段见 spec
- [x] 4.2 [code] `require_email_verify` Switch disabled when `allow_self_register_email=false`(`ProFormDependency` / `Form.useWatch` 联动)
- [x] 4.3 [code] `allowed_email_domains` 用 AntD `Select mode="tags" tokenSeparators=[',',' ']`,onBlur 强制 lowercase;`self_register_default_role_id` Select 复用 `GET /api/v1/roles`(已存在)
- [x] 4.4 [code] Save → PUT policy → notification + invalidate query;顶部 banner:`mailStatus.fallback_mode && policy.allow_self_register_email && policy.require_email_verify` → `Alert.warning`
- [x] 4.5 [code] OpenAPI client 重生成(`pnpm openapi`)+ typecheck

- [x] 4.6 [code] **(2026-06-10 用户拍板)** Policy 卡片从 Settings›Authentication 迁出为独立页 `_auth/settings/registration.tsx`(/settings/registration,菜单「注册策略」UserAddOutlined);认证页留 info Alert 链接

> ✅ **支柱 A 验收点**:apply 到此即可——Owner 开 `allow_self_register_oauth` + 域白名单 → 陌生 GitHub 用户自动建号登录(替代原 401)。支柱 B 可后续增量。

---

## 支柱 B:Email 自助注册 + pending_approval 审批

## 5. Server: /register endpoint（真新）

- [x] 5.1 [code] 新建 `crates/swarmhive-server/src/routes/register.rs`(扁平,公开):`POST /api/v1/auth/register { email, display_name, password }`
- [x] 5.2 [code] 校验链:`allow_self_register_email` → 410;email 占用 → 422 `email_already_taken`;domain → 422;弱口令 → 422(复用 `password::validate_strong_password`)
- [x] 5.3 [code] 创建:INSERT `user(status=Provisioned, email_verified_at=NULL)` + `user_credentials` + `user_role(default_role)`;require_email_verify=true → 复用 ④ account_token + 发 `email_verify` 邮件 → `{next:'verify_email'}`;否则按 require_approval 决定 status + 写 session
- [x] 5.4 [code] audit `user_self_registered`;mount + utoipa
- [x] 5.5 [test] `register_smoke.rs`:policy 开 → pending(Invited)+ 邮件 mock;关 → 410;domain → 422;占用 → 422

## 6. Server: verify-email 扩展（改 ④ 的 routes/verify_email.rs，不新建）

- [x] 6.1 [code] `POST /auth/verify-email` 消费成功后:**若 `user.status==Provisioned`** → 按 `policy.require_approval` 转 `PendingApproval`/`Active` + 写 session;`status==Active`(banner)维持原行为。role 已在 /register 绑,不重绑
- [x] 6.2 [code] 新增公开 `POST /api/v1/auth/verify-email/resend { email }`:始终 200(枚举防御 + timing 拉平);仅对 `email_verified_at IS NULL` 用户 invalidate active token + gen new + 发邮件
- [x] 6.3 [code] audit `email_verified`(④ 若已写则沿用);mount 新 resend route
- [x] 6.4 [test] `register_smoke.rs` 续:register → verify(token)→ 验 Invited→PendingApproval/Active 转移 + session;active banner verify 不转移

## 7. Server: pending_approval admin endpoints（扩 routes/users.rs）

- [x] 7.1 [code] `routes/users.rs` 加 `GET /api/v1/users/pending-approval`(分页)+ `POST /api/v1/users/:id/approve { role_id? }` + `POST /api/v1/users/:id/reject { reason? }`,全 `require user:manage`
- [x] 7.2 [code] approve:status='active' + role_id 可选覆盖(UPDATE user_role)+ audit `user_approved`
- [x] 7.3 [code] reject:DELETE user(CASCADE user_role/credentials/identity_link/account_token)+ audit `user_rejected` 含 reason
- [x] 7.4 [test] `approval_smoke.rs`:approve(含 role 覆盖)+ reject 完整流程 + 权限 gate

## 8. Mail template: email_verify context 确认（模板已存在）

- [x] 8.1 [code] 确认 `assets/mail-templates/email_verify.{en,zh-CN}.*` 的占位与 register/resend handler 构造的 context(`verify_url` / `expires_at` / `display_name`)一致;不一致则微调模板/ context(**不新建模板**)

## 9. Admin SPA: register / awaiting-approval / Users 扩展

- [x] 9.1 [code] 新建 `apps/admin/src/routes/register.tsx`(公开):beforeLoad 查 setupInfo + policy;bootstrap 未完 → /setup;`allow_self_register_email=false` → /login + Alert
- [x] 9.2 [code] ProForm email+display_name+password+confirm(复用 ① 强口令 zod);顶部按 policy 渲染提示("注册后需审批" / "需验证邮箱" / "即可使用");submit → 按 `next` 跳(`verify_email`→ verify-email-sent / `pending_approval`→ awaiting / `home`→ /);`/login` 加条件 "注册" 链接
- [x] 9.3 [code] 复用已存在 `verify-email.tsx`:按 ⑤ 的 `next` 跳转;新建 `verify-email-sent.tsx`(提示 + "未收到?重发" → 公开 resend)
- [x] 9.4 [code] 新建 `_auth/awaiting-approval`(Result 卡 + 手动刷新 + 30s 轮询 invalidate me);改 `_auth` guard:`me.status==='pending_approval' && path!=='/awaiting-approval'` → redirect
- [x] 9.5 [code] 扩 ④ Users 页:status filter 加 pending_approval;行 Approve(role Select 默认 policy.default,可改 → POST approve)/ Reject(reason TextArea → POST reject);OpenAPI 重生成
- [x] 9.6 [code] **(2026-06-10 用户拍板)** 审批独立成页:users 目录化(`/users` redirect → `/users/list`;same-path 父子菜单撞 key 导致选中态失效,与 /settings 同款解法),新建 `/users/approvals`(server 分页 + 批准/拒绝 Modal);成员列表 pending 行只留「去审批」;`RoleSelect` 抽 `users/-shared.tsx`;`GET /users/pending-approval` 改返 `UserListItem`(含 roles 供预填)
- [x] 9.7 [code] **(2026-06-10 用户扩展)** 成员管理操作:server `PUT /users/{id}/role` + `POST /users/{id}/{disable,enable}`(`guard_not_owner_not_self` 护栏、disable 踢全部 session、audit `user_role_changed`/`user_disabled`/`user_enabled`)+ 列表行「更改角色 / 禁用 / 启用」(owner 行与自己不渲染);测试 `approval_smoke::{change_role_disable_enable_lifecycle,member_management_guards_owner_and_self}`

---

## 10. 测试

- [x] 10.1 [test] `cargo test --workspace` 全绿(policy + register + verify + approval + oauth smoke)
- [ ] 10.2 [test] Vitest:mock policy / mail status → 渲染分支(banner / register 入口可见性 / verify Switch 联动)——**deferred**:项目 Vitest 现状全是 lib 纯函数/hook 单测,无页面渲染测试 harness(router+query+antd mock);与 e2e harness 一并补。现有 47 Vitest 全绿,渲染分支靠 typecheck + build 保护
- [ ] 10.3 [test] Playwright `oauth-self-register-flow.spec.ts`(**支柱 A**)——**deferred**:e2e global-setup 无 mock-GitHub 基建;流程已被 `oauth_smoke` 3 个自助注册集成测试覆盖
- [ ] 10.4 [test] Playwright `self-register-flow.spec.ts`(**支柱 B**)——**deferred**:e2e global-setup 无 mailpit(拿不到 verify 链接);流程已被 `register_smoke` 6 测试覆盖
- [x] 10.5 [code] `pnpm lint` + typecheck + `cargo clippy` 全绿

## 11. Docs / memory 同步

- [x] 11.1 [docs] `docs/13-rbac.md` 加 "Registration Policy" 段(字段集 + 状态机 = 4 态:Active/Disabled/Provisioned/PendingApproval + email_verified_at 正交轴)
- [x] 11.2 [docs] `docs/08-admin-and-analytics.md` Users 页加 pending_approval 工作流
- [x] 11.3 [docs] `dev-notes/knowledge/backend.md` 加 "user.status 状态机 + registration_policy 单例" 段;记 `Invited`→`Provisioned` 改名 + 一次性 raw 迁移(read-before-update 顺序坑)、"无 pending_verify、verify 走 email_verified_at"
- [x] 11.4 [docs] `dev-notes/knowledge/admin-spa.md` 加 "pending_approval 路由分流" + "/register 入口 policy-driven 可见性"
- [x] 11.5 [docs] `openspec/changes/README.md` 依赖图加本节点(标 ①②③④ 已 archived)

## 12. 端到端验证

> server 侧行为已由 `{registration_policy,register,approval}_smoke` + `oauth_smoke` 集成测试逐条覆盖
> (12.1/12.2/12.4 的断言与测试一一对应);以下勾选留给**真机浏览器**全栈过一遍(SPA 逻辑已
> typecheck + build 通过,但未跑真浏览器)。

- [ ] 12.1 [code] **支柱 A**:Owner 开 `allow_self_register_oauth` + `allowed_email_domains` → 陌生 GitHub 用户(白名单内)→ 自动建号 + 按 policy 决定 status;白名单外 → `/login?oauth_error=domain_not_allowed`;关开关 → 401
- [ ] 12.2 [code] **支柱 B**:Owner 开 email 自助 + verify + approval → /register → 收 verify 邮件 → 点 → /awaiting-approval → Owner approve(role=publisher)→ 30s 内跳 /
- [ ] 12.3 [code] `allow_self_register_email=false` → /login 无 "注册";直访 /register → /login + Alert
- [ ] 12.4 [code] `allowed_email_domains=['example.com']` → register x@other.com → 422
- [ ] 12.5 [code] Mail 未配置 + require_email_verify=true → Settings 顶部 banner
