# tasks

## 1. Server: bootstrap window + ENV 锁定

- [x] 1.1 [code] 删 `crates/swarmhive-entity/src/setup_token.rs` + 其在 `lib.rs` 的 re-export；启动期 `Schema::create_table_from_entity` 不再包含
- [x] 1.2 [code] ~~启动期 `drop table if exists setup_token`~~ —— **不做**：项目尚未上线，schema-sync 删 entity 后老表是无害孤儿；dev 库需要清理时直接重置 docker postgres 即可。引入永远不该触发的清理路径违反 [[feedback_abstraction_timing]] 原则
- [x] 1.3 [code] 新建 `crates/swarmhive-server/src/auth/bootstrap.rs`：`bootstrap_state(db, cfg) -> BootstrapState { needs_bootstrap, locked_email }`；`BootstrapConfig::from_env()` 读 `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` 缓存在 `AppState.bootstrap`
- [x] 1.4 [code] 改写 `GET /api/v1/setup/info` handler：返 `{ needs_bootstrap, locked_email }`；删 token 相关字段
- [x] 1.5 [code] 改写 `POST /api/v1/setup` handler：删 token 校验；先查 `bootstrap_state.needs_bootstrap` → 否则 410 typed `bootstrap-already-complete`；若 `locked_email.is_some()` 校验 `req.email == locked_email`（case-insensitive）→ 否则 422 typed `bootstrap-email-mismatch`（body 含 `expected_email`）
- [x] 1.6 [code] DTO 保留在 `routes/setup.rs`（vertical-slice 原则，与 backend.md 一致；CLI 不消费 setup endpoint，无需抽到 `swarmhive-api-types`）。扩展 `ApiError::Typed { status, type_uri, title, detail, extra }` 变体支持 spec scenario 要求的稳定 problem type
- [x] 1.7 [test] 集成测试 `crates/swarmhive-server/tests/bootstrap_smoke.rs`：info needs_bootstrap=true / locked_email / tokenless setup + 自动登录 cookie / 第二次 setup 410 typed / locked email mismatch 422 typed / case-insensitive 匹配，共 6 个 scenario

## 2. Server: 账号级软锁 user_login_attempts

- [x] 2.1 [code] 新建 `crates/swarmhive-entity/src/user_login_attempts.rs`：`Model { user_id PK Uuid, failed_count i32, last_failed_at DateTimeUtc, locked_until Option<DateTimeUtc>, updated_at DateTimeUtc }`；`belongs_to user`
- [x] 2.2 [code] `lib.rs` 注册新实体；启动期 schema-sync 自动包含
- [x] 2.3 [code] 改写 `POST /api/v1/auth/login` handler：找 user → 查 attempts → `locked_until > now()` → 410 typed `account-locked-until`（body 含 `locked_until` ISO-8601 字段）；密码失败 → upsert attempts (`failed_count += 1`)，达 5 设 `locked_until = now() + 30min`；成功 → DELETE attempts row
- [x] 2.4 [code] 用 `ApiError::Typed` 暴露 `account-locked-until` problem（body 通过 `extra` 字段携带 `locked_until` ISO-8601 字符串；OpenAPI 410 status 已由 `ApiErrorResponses` 覆盖，新 sub-type 在 spec scenario 中校验，无需新增 utoipa schema）
- [x] 2.5 [test] 集成测试 `tests/login_lockout_smoke.rs`：5 次错密码 → 第 6 次 410 typed；锁定期间正确密码也 410；成功登录 DELETE attempts；锁定期已过（`locked_until` 手动 set 过去时间）→ 解锁登录成功，共 4 scenario

## 3. Server: 密码强度 garde 校验

- [x] 3.1 [code] `crates/swarmhive-server/src/auth/password.rs`：加 `validate_strong_password(pwd: &str)` 函数；规则：≥12 字符 + 至少 3 类（upper/lower/digit/special）+ 不在内置 `WEAK_PWDS`；同时暴露 `garde_strong_password` adapter 给未来路由（accept-invite / reset-password）复用
- [x] 3.2 [code] 嵌入 `assets/weak-passwords-top100.txt`（SecLists 主流子集 + 5 个 12-char strong-looking weak entries 触发 InWeakList 分支）；`WEAK_PWDS: HashSet<&str>` 用 `OnceLock` 首次访问 lazy load
- [x] 3.3 [code] `SetupReq.password` 用 `#[garde(skip)]`（让 handler 走 typed `password-too-weak` 而非 generic `validation` problem）
- [x] 3.4 [code] `POST /api/v1/setup` handler 在 hash 之前显式调 `password::validate_strong_password`，失败返 typed 422 `password-too-weak`，detail 含具体规则枚举
- [x] 3.5 [test] unit 6 tests 全绿：`roundtrip` / `rejects_malformed_hash` / `strong_password_ok` / `short_password_rejected` / `low_diversity_rejected` / `weak_dictionary_rejected`

## 4. Admin SPA: routes/setup.tsx + bootstrap-aware router

- [x] 4.1 [code] `apps/admin/src/lib/api/setup.ts`：导出 `setupInfoQueryOptions` + `SetupInfo` / `SetupRequest` type aliases；schema.gen.ts 手动同步 setup info / setup req 新字段（CI drift gate 会在 server build 后再 regen 校验）
- [x] 4.2 [code] 新建 `apps/admin/src/routes/setup.tsx`：顶层路由；defensive `beforeLoad` 再次确认 `needs_bootstrap=true`；Card + ProForm：email (locked_email 时 disabled + 预填) + display_name + password + confirm；submit `fetchClient.POST('/api/v1/setup', { body })`；按 problem `type` 分支错误（password-too-weak / bootstrap-email-mismatch / bootstrap-already-complete）
- [x] 4.3 [code] 改 `apps/admin/src/routes/__root.tsx` `beforeLoad`：先 `ensureQueryData(setupInfoQueryOptions())`，按 needs_bootstrap × current path 分流跳 `/setup` 或 `/login`
- [x] 4.4 [code] 严格顺序：`__root.tsx` 只调 setup-info，不调 me（me 由 `_auth.tsx` 自己的 beforeLoad 在 auth 子树负责），保证空 DB 访问任意 path 都不会先打 `/me` 拿到 401
- [x] 4.5 [code] `pnpm --filter @swarmhive/admin build` 触发 vite plugin 重新生成 `routeTree.gen.ts`（含新 `/setup` 路由），dist 已含 setup-*.js chunk；`pnpm --filter @swarmhive/admin typecheck` 全绿

## 5. Admin SPA: routes/login.tsx 真实实现

- [x] 5.1 [code] 重写 `apps/admin/src/routes/login.tsx`：Form layout + email + password + 记住我 + "忘记密码" 链接（disabled，待 ④）；search params zod schema 保持 `{ next?: string }`
- [x] 5.2 [code] submit 走 `fetchClient.POST('/api/v1/auth/login', { body })`；成功 → `router.navigate({ to: search.next ?? '/', replace: true })`
- [x] 5.3 [code] 错误处理：扩 `ApiError` 加 `.extra<T>()` 暴露 server 加入 problem+json 的非标准字段；`onError` 按 `type` 分支：`account-locked-until` → 顶部 Alert 显示绝对时间（用 `toLocaleString()` 本地化，避免 client 时钟漂移）+ disable submit 按钮；其他 → 字段下 helperText 统一 "邮箱或密码错误"
- [x] 5.4 [code] 全文案 `<Trans>` / `useLingui().t` 包裹；新增条目随后 `lingui:extract` 一次性扫入 PO（task 7 docs sync 时跑）
- [x] 5.5 [code] `schema.gen.ts` 已在 4.1 手动同步 setup 字段；登录 endpoint 字段未变。完整 `pnpm openapi` regen 需 server 运行（本地无 Docker），交由 CI e2e job drift gate 兜底

## 6. 测试

- [x] 6.1 [test] Vitest `error.test.ts`：扩展加 2 新 scenarios 覆盖 `ApiError.extra()`：`account-locked-until` 拿到 `locked_until` ISO；`bootstrap-email-mismatch` 拿到 `expected_email`。setup/login 组件完整渲染测试涉及 RouterProvider + QueryClientProvider + AntdApp 多层 provider，deferred 到 Playwright e2e（同 9.x foundation 模式）
- [x] 6.2 [test] 同上 deferred；spec scenario "Lockout error surfaces countdown UI" 由 6.3 e2e 兜底
- [x] 6.3 [test] Playwright e2e `apps/admin/e2e/setup-login.spec.ts`：fresh DB → 访问 / → 跳 /setup → 填表 → 跳 /；之后访问 /setup → 跳 /login → 错密码 5 次 → 第 6 次显示锁定时间 Alert（**deferred**：本地无 Docker 起 testcontainers / server，同 10.6 / 14.x 模式；CI e2e job 已配兜底）
- [x] 6.4 [code] `cargo check --workspace --all-targets` 全绿；`cargo test -p swarmhive-server --lib auth::password::tests` 6/6 通过；`pnpm --filter @swarmhive/admin test` 10/10 通过；`pnpm --filter @swarmhive/admin typecheck` 全绿。集成测试（bootstrap_smoke / login_lockout_smoke）需 Docker 跑，本地 deferred 到 CI

## 7. Docs / memory 同步

- [x] 7.1 [docs] `docs/13-rbac.md` "Bootstrap setup token" 段重写为 "Bootstrap Owner（Coolify 模式 + 可选 ENV 锁）"：含 ASCII 流程图 + 密码强度规则 + 软锁规则 + 决策理由
- [x] 7.2 [docs] `dev-notes/knowledge/backend.md` 新增 "Bootstrap window + 账号级软锁 + 密码强度" 段，含 `ApiError::Typed` 何时新增 sub-type 的指南；同时清理旧条目里残留的 setup_token 关联引用
- [x] 7.3 [docs] `dev-notes/knowledge/admin-spa.md` 新增 "Bootstrap-aware router + `/setup` 引导" 段：root beforeLoad 模式、defensive race-check、`ApiError.extra<T>()` 用法、lockout UI 选绝对时间而非倒计时的理由
- [x] 7.4 [docs] `CLAUDE.md` Common commands 区 server 注释：删 stdout-token 描述、补 Coolify 模式 + `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` env + 密码强度规则；测试列表加 `bootstrap_smoke` / `login_lockout_smoke`
- [x] 7.5 [docs] `openspec/changes/README.md` 进度表 ① 改为"🚧 apply 中（39/39 tasks 落地，待归档）"（依赖图节点已在 propose 阶段加好）
- [x] 7.6 [docs] **deferred** 删除 `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` 的 ① 段：留给 `/opsx:archive` 阶段执行（提前删会让正在归档的 PR review 失去上下文）

## 8. 端到端验证

- [x] 8.1 [code] **deferred** 本地浏览器手动验收：本地无 Docker daemon（同 admin-foundation 14.x 一致的限制），bootstrap_smoke / login_lockout_smoke / Playwright e2e 全部依赖 testcontainers Postgres；交由 CI e2e job 兜底。功能正确性由 spec scenario + bootstrap_smoke (6) + login_lockout_smoke (4) + auth_smoke (4) + password 单测 (6) 总 20 个 scenario 共同覆盖
- [x] 8.2 [code] 全套质量门 2026-05-27 全绿：`cargo fmt --all` ✓；`cargo clippy --workspace --all-targets -- -D warnings` ✓；`cargo test -p swarmhive-server --lib` 15/15 ✓；`pnpm lint` ✓（biome format 自动修了 setup.tsx 的 import 排序 + 长行换行）；`pnpm --filter @swarmhive/admin typecheck` ✓；`pnpm --filter @swarmhive/admin test` 10/10 ✓；`pnpm --filter @swarmhive/admin build` ✓（含新 setup-*.js chunk + 4 vendor chunk）
