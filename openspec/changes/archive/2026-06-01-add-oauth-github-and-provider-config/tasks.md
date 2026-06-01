# tasks

## 1. Workspace deps + 共享 SecretKey

- [x] 1.1 [code] workspace `Cargo.toml` 加 `oauth2 = "5"`（pin minor）；如 ② 已加 `aes-gcm` / `base64` 则复用
- [x] 1.2 [code] 决定 SecretKey 模块位置（看 ② 是否已落）：
  - ② 已落 → 把 `crates/swarmhive-server/src/mail/crypto.rs` 提升为 `crates/swarmhive-server/src/crypto.rs`，re-export 给 mail；本 proposal 引用 `crate::crypto::SecretKey`
  - ② 未落 → 本 proposal 直接落 `crates/swarmhive-server/src/crypto.rs` + `SecretKey`，留好 mail 复用
- [x] 1.3 [code] ENV：统一 `SWARMHIVE_SECRET_KEY`（若已用 `SWARMHIVE_MAIL_PASSWORD_KEY` 则启动期 alias 兼容，warn 提示 deprecate）
- [x] 1.4 [code] `crates/swarmhive-server/Cargo.toml` 加 `oauth2.workspace = true`

## 2. Entity oauth_provider

- [x] 2.1 [code] 新建 `crates/swarmhive-entity/src/oauth_provider.rs`：Model（id Uuid PK, kind enum(Github), name, enabled bool, client_id, client_secret_encrypted, scopes Vec<String>, authorize_url, token_url, userinfo_url, email_field, created_at, updated_at）
- [x] 2.2 [code] `lib.rs` 注册；schema-sync 包含
- [x] 2.3 [code] 启动期注入 partial unique index：`CREATE UNIQUE INDEX IF NOT EXISTS oauth_provider_kind_uniq ON oauth_provider (kind)`（MVP 一种 kind 一个）
- [x] 2.4 [code] `swarmhive-api-types`：加 `OAuthProvider*` DTO + utoipa schema（永不暴露 secret 字段）

## 3. IdentityProvider trait + GithubProvider

- [x] 3.1 [code] 新建 `crates/swarmhive-server/src/auth/oauth/mod.rs`：trait `IdentityProvider` + `ExternalIdentity` struct + `OAuthError` enum
- [x] 3.2 [code] 新建 `crates/swarmhive-server/src/auth/oauth/github.rs`：`GithubProvider` 实现，用 `oauth2` crate 构造 client；exchange 流程含 `/user` + `/user/emails` 双调（取 verified primary email）
- [x] 3.3 [code] 新建 `crates/swarmhive-server/src/auth/oauth/factory.rs`：`provider_factory(row, secret_key) -> Box<dyn IdentityProvider>`；按 kind 分发
- [x] 3.4 [test] unit `github::tests::parses_user_response` + `picks_verified_primary_email` + `fails_when_no_verified`（用 wiremock 或 oauth2 自带 mock）

## 4. Server endpoints: OAuth flow

- [x] 4.1 [code] 新建 `crates/swarmhive-server/src/routes/auth/oauth.rs`：handler `start` / `callback` / `public_providers` / `link_start` / `unlink`
- [x] 4.2 [code] `start` 流程：检查 bootstrap_state.needs_bootstrap → 410；查 provider WHERE name AND enabled → 404 if missing；gen state+pkce 存 session → redirect provider.authorize_url
- [x] 4.3 [code] `callback` 流程：state 校验 → exchange → 查 identity_link → 已存在 写 session redirect；找不到 + email 冲突 → redirect /login?oauth_conflict=<name>；找不到 + 无冲突 → 401 oauth_registration_disabled（⑤ 接入时改）
- [x] 4.4 [code] `link_start`（authenticated）：复用 start 逻辑，session 加 mode='link' user_id
- [x] 4.5 [code] `unlink`：查 user_credentials 存在 → DELETE identity_link；否则 → 409 cannot_unlink_only_auth_method
- [x] 4.6 [code] `public_providers`：返 enabled provider list（仅 name + kind）
- [x] 4.7 [code] 全部 endpoint 加 utoipa 注解 + ApiError responses + audit log（oauth_login_succeeded / oauth_login_failed / identity_linked / identity_unlinked event）
- [x] 4.8 [test] integration `oauth_smoke.rs`：mock GitHub（wiremock）→ 完整 start/callback round-trip → identity_link 插入 → 第二次 callback 直接登录

## 5. Server endpoints: OAuth provider CRUD

- [x] 5.1 [code] 新建 `crates/swarmhive-server/src/routes/auth/providers.rs`：CRUD + /test handler；全部 `RequirePermission<AuthManage>`
- [x] 5.2 [code] POST/PUT 接受 client_secret 明文 → encrypt 入库；GET 永不返
- [x] 5.3 [code] kind=Github 创建时 URL 字段空 → 自动填默认值（authorize/token/userinfo URL + scopes）
- [x] 5.4 [code] `/test` 实现：校验 client_id 非空 + secret 非空 + GET authorize_url（HEAD 也行）返 200/302；否则 422
- [x] 5.5 [code] `add-auth-and-rbac` permission seed 补 `auth:manage` permission；启动期 INSERT IF NOT EXISTS + bind owner & admin
- [x] 5.6 [test] integration `oauth_provider_crud_smoke.rs`：CRUD + /test + permission 拒绝

## 6. Server: mount router + bootstrap check

- [x] 6.1 [code] `lib.rs` mount `/api/v1/auth/oauth/*` + `/api/v1/auth/providers/*`
- [x] 6.2 [code] /start handler 内调 `bootstrap_state(db)` 检查；测试覆盖 empty user 表 → 410
- [x] 6.3 [test] integration `oauth_bootstrap_block_smoke.rs`：empty DB + 已配 enabled provider → /start 返 410

## 7. Admin SPA: /login OAuth 按钮

- [x] 7.1 [code] `apps/admin/src/lib/api/oauth.ts`：导出 `publicProvidersQueryOptions = () => $api.queryOptions('get', '/api/v1/auth/oauth/providers')`
- [x] 7.2 [code] 改 `apps/admin/src/routes/login.tsx`（① 已落）：useQuery publicProviders，列表非空 → 渲染按钮组在表单下方 + `<Divider />`；空 → 不渲染
- [x] 7.3 [code] 按钮 click → `window.location.href = '/api/v1/auth/oauth/' + name + '/start?next=' + encodeURIComponent(next)`
- [x] 7.4 [code] /login URL search params 加 `oauth_conflict`：值 = provider_name → 顶部 Alert "您的 GitHub 邮箱已在系统中注册..."（不暴露 email）

## 8. Admin SPA: Settings > Authentication 页

- [x] 8.1 [code] 新建 `apps/admin/src/routes/_auth.settings.authentication.tsx`：ProTable + ProDrawerForm CRUD oauth_provider；permission gate `auth:manage`
- [x] 8.2 [code] 表单：name + kind (Select GitHub) + client_id + client_secret (Password) + scopes (Select multi) + enabled (Switch) + (折叠) URL 三件套 (autoprefill on kind=GitHub)
- [x] 8.3 [code] kind 改变 onChange → 若 kind=Github 且 URL 字段空 → 预填默认值；用户可改
- [x] 8.4 [code] Test 按钮 → POST /test → notification；启用/停用 toggle 直接 PUT
- [x] 8.5 [code] 把 ② layout 中 "Authentication" 菜单条目从 disabled 改 enabled
- [x] 8.6 [code] 页面顶部 oauth_provider list + 页面底部留 placeholder "Registration Policy 即将上线（add-registration-policy-and-self-register）" Alert

## 9. Admin SPA: Profile > Linked accounts

- [x] 9.1 [code] 新建 `apps/admin/src/routes/_auth.profile.tsx`：最小版，仅 "Linked accounts" 卡片
- [x] 9.2 [code] `GET /api/v1/auth/me/identity-links` 新加 endpoint（server 端 5 行 handler）+ 对应 query
- [x] 9.3 [code] 列表渲染 + "Link GitHub" 按钮（仅 GitHub provider enabled 且当前 user 未 link 时显示）
- [x] 9.4 [code] "Unlink" 按钮（每行）：confirm modal → DELETE；当前 user 无 password → disabled + tooltip
- [x] 9.5 [code] `__root.tsx` 用户 avatar dropdown 加 "Profile" 入口
- [x] 9.6 [code] OpenAPI drift：跑 `pnpm --filter @swarm-hive/admin openapi`

## 10. 测试

- [x] 10.1 [test] cargo test --workspace 全绿（含 OAuth unit + integration smoke）
- [ ] 10.2 [test] Vitest `apps/admin/src/routes/login.test.tsx` 扩展：mock publicProviders 两种返回（含 / 不含 GitHub）→ 渲染检查按钮是否出现
- [ ] 10.3 [test] Vitest `apps/admin/src/routes/_auth.profile.test.tsx`：mock identity_links + user 有/无 password → Unlink disabled 状态
- [ ] 10.4 [test] Playwright e2e `apps/admin/e2e/oauth-flow.spec.ts`：mock GitHub OAuth（用 wiremock 或 docker mock-oauth2-server）→ Owner 在 Settings 配 provider → 启用 → 登出 → 重登用 GitHub → 成功
- [x] 10.5 [code] pnpm lint + pnpm --filter @swarm-hive/admin typecheck 全绿

## 11. Docs / memory 同步

- [x] 11.1 [docs] `docs/13-rbac.md` Identity Providers 段：补 admin Settings 后台配置 + bootstrap 阶段排除
- [x] 11.2 [docs] `dev-notes/knowledge/backend.md` 加 "OAuth 模块" 段：trait + factory + 加密 + bootstrap block
- [x] 11.3 [docs] `dev-notes/knowledge/admin-spa.md` 补 Profile 页约定 + /login oauth_conflict 渲染
- [x] 11.4 [docs] `openspec/changes/README.md` 依赖图加 `add-oauth-github-and-provider-config` 节点（重命名提示）
- [x] 11.5 [docs] apply + archive 后删 `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` 中 ③ 段

## 12. 端到端验证

- [ ] 12.1 [code] 本地 docker postgres + cargo run + pnpm admin:dev：
  - ① 在 GitHub 注册一个 OAuth App（dev 用 `http://localhost:5173/api/v1/auth/oauth/github/callback`）
  - ② Settings>Authentication 配 client_id/secret → 启用
  - ③ 登出 → /login 看到 "Sign in with GitHub" → 点 → 跳 GitHub → 同意 → 回 → 已登录
  - ④ Profile 查看 linked accounts
  - ⑤ 邮箱冲突场景：先用 password 用户 email=X 登录 → 登出 → 用同 email 的 GitHub 点登录 → /login?oauth_conflict=github Alert
- [ ] 12.2 [code] 清空 user 表（dev DB truncate）+ 配过 oauth_provider → 访问 /api/v1/auth/oauth/github/start → 410（bootstrap block）
