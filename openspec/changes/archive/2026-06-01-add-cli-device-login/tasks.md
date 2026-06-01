# tasks

## 1. Workspace deps

- [x] 1.1 [code] workspace `Cargo.toml` 加 `webbrowser = "1"`（pin minor）；`crates/swarmhive-cli/Cargo.toml` 加 `webbrowser.workspace = true`
- [x] 1.2 [code] 确认 `rand` / `blake3` 已在 workspace（token::mint 已用）；device_code/user_code 生成复用之

## 2. Entity device_authorization

- [x] 2.1 [code] 新建 `crates/swarmhive-entity/src/device_authorization.rs`：`DeviceGrantStatus` enum（`#[serde(rename_all="lowercase")]` + string_value 对齐）+ Model（device_code_hash 唯一 / user_code 索引 / client_id / client_name / token_name / scope / status / user_id / interval_secs / last_polled_at / approved_at / expires_at / created_at）
- [x] 2.2 [code] `lib.rs` 注册 `pub mod device_authorization;`；schema-sync 自动包含（**不**手写 partial unique index）
- [x] 2.3 [test] entity 单测：`DeviceGrantStatus` serde round-trip 锁 lowercase wire 值（仿 `mail_provider::tests`）

## 3. api-types device DTO

- [x] 3.1 [code] 新建 `crates/swarmhive-api-types/src/device.rs`：`DeviceCodeRequest` / `DeviceCodeResponse` / `DeviceTokenRequest` / `DeviceTokenResponse` / `DeviceTokenError`(snake_case) / `DeviceTokenErrorResponse` / `DeviceVerifyRequest` / `DeviceAuthorizationView`（全 `ToSchema`）
- [x] 3.2 [code] `lib.rs` re-export device DTO；**删** `api_token.rs` 的 `CliTokenRequest` / `CliTokenResponse` + 对应 re-export

## 4. Server: routes/device.rs（5 endpoint，vertical-slice 单文件）

- [x] 4.1 [code] `device_code` handler：bootstrap_state 检查 → 410 typed；lazy `DELETE WHERE expires_at < now() - INTERVAL '1 hour'`（带 grace，保住刚过期行返 `expired_token` 而非误删成 not-found）；gen device_code(32B base64url)+blake3 hash / user_code(8×base20，活跃集合内唯一校验+重试)；INSERT row（单条 insert 触发 before_save 填 created_at，expires_at 显式 Set）；拼 `verification_uri{,_complete}` 用 `ServerConfig.base_url`
- [x] 4.2 [code] `device_token` handler：按 blake3(device_code) 查行 → **七分支**（查不到/已清→`invalid_grant`〔**先于所有 status 分支**〕、过期→`expired_token`、限速→`slow_down`、pending→`authorization_pending`、denied→`access_denied`、completed→`invalid_grant`、approved→铸造），错误走 RFC 8628 `400 {error}`（**非** ApiError problem+json，需单独 IntoResponse 或手写 `(StatusCode, Json)`）。请求校验：`grant_type` 非 device-code URN→`unsupported_grant_type`、`client_id` 缺失/≠`swarmhive-cli`→`invalid_request`
- [x] 4.2a [code] approved 分支**原子 claim**：先 `update_many().col_expr(Status, Completed).filter(Id.eq).filter(Status.eq(Approved))` → `rows_affected==1` 才 load user + 复刻 `cli_token` 临时 Principal + `token_service::create(Pat)`；否则 `invalid_grant`（防并发铸两个 PAT）
- [x] 4.2b [code] `slow_down` 状态机：首次 `last_polled_at=null`→不限速、按 status 返回、写 `last_polled_at=now`；之后 `now-last_polled_at<interval_secs`→`slow_down` 且**不刷新** `last_polled_at`（防边界抖动活锁）
- [x] 4.3 [code] `device_lookup`（Principal）：user_code 查活跃行 → `DeviceAuthorizationView` 或 404（未知与过期同形，不可枚举）
- [x] 4.3a [code] `device_lookup` 与 4.4 的 approve/deny **只接受 Session-derived Principal**：`match principal.auth_method { Session{..} => ok, Pat|ApiToken => 403 }`（或 Session-only extractor）。防 Bearer PAT 脱离浏览器自批
- [x] 4.4 [code] `device_approve` / `device_deny`（Session-only，见 4.3a）：置 status + user_id/approved_at；audit `auth:device_authorized` / `auth:device_denied`。（⑤ 落地后补 `user.status==active` 校验，见 design Risks）
- [x] 4.5 [code] 全部 endpoint 加 `#[utoipa::path]` + ApiError responses（token 端点的 400 用 `DeviceTokenErrorResponse` body 标注）
- [x] 4.6 [code] 挂载（注意 governor 非全局，只 `.layer()` 在 `sensitive` 子路由）：`build_router` 把**整个** `routes::device::router()` merge 进 **`sensitive` 子路由**（与 auth/setup/password_reset 同组，继承 per-IP 5rps/burst20 governor 作 DoS 兜底；轮询主限速靠 4.2b 的 slow_down）；`openapi_router` 也 merge 进对应 sensitive 组（不挂 layer，仅 codegen）。两处 merge 列表保持同步否则 schema.gen 漂移
- [x] 4.7 [code] 若 `routes/device.rs` 超 250 LOC，按阈值提 `routes/device/service.rs`（user_code 生成 / 状态机 helper）

## 5. Server: 删除 ROPC cli-token

- [x] 5.1 [code] 删 `routes/auth.rs::cli_token` handler + `CliTokenReq` struct + `routes!(cli_token)` 注册
- [x] 5.2 [code] 确认 `auth::service::verify_password` 仍被 `/auth/login` 使用（不删）；更新 `auth/service.rs` 的 doc 注释（顶层 `//!` 第 9 行「login + cli-token both verify」+ verify_password 文档第 129-131 行引用 `routes/auth.rs::cli_token`）去掉 cli-token 引用、保留 login 语义
- [x] 5.3 [test] **删除现有 `crates/swarmhive-server/tests/cli_token_smoke.rs`**（3 个用例：happy-path 200 / wrong-password 401 / governor——端点移除后必在运行时 fail；happy-path 等价覆盖由 6.1 device_login_smoke 接管）
- [x] 5.4 [test] `openapi_surface` 测试断言 doc **无** `cli-token` path；新增/复用一个 integration 断言 `POST /auth/cli-token` → 404（落 `tests/openapi_surface.rs` 或 `device_login_smoke.rs`；注：build_router 无全局 fallback，404 是 axum 默认裸 404 非 problem+json，scenario 只断 status=404）

## 6. Server: 测试

- [x] 6.1 [test] 新建 `crates/swarmhive-server/tests/device_login_smoke.rs`（testcontainers postgres）：code→pending poll(authorization_pending)→approve→poll(200 PAT)→二次 poll(invalid_grant)
- [x] 6.2 [test] 同文件覆盖：deny→access_denied；过期行→expired_token；未知 device_code→invalid_grant；首次 poll(last_polled_at=null)→authorization_pending(非 slow_down)；slow_down（已 poll 过再快速 poll）；grant_type/client_id 非法→unsupported_grant_type/invalid_request
- [x] 6.2a [test] **并发铸造单次性**：对一个 approved grant 并发两次 poll → 恰好一行 `api_token` + 一条 `auth:token_created`，败者得 `invalid_grant`（验证 4.2a 原子 claim）
- [x] 6.3 [test] bootstrap block：空 user 表 → device/code → 410
- [x] 6.4 [test] lookup 不暴露 secret + 未知 code 404；approve/deny 写 audit 行；**approve/deny 用 Bearer PAT → 403**（验证 4.3a Session-only）

## 7. CLI: 重写 login.rs

- [x] 7.1 [code] `commands/login.rs`：device flow（POST device/code → 打印 user_code+uri → `webbrowser::open(verification_uri_complete)`（失败仅 warn）→ 轮询 device/token，slow_down 时 interval+=5，access_denied/expired_token 报错退出）
- [x] 7.2 [code] **token 获取即成功边界**：拿到 token 后 `GET /api/v1/auth/me`（Bearer 新 token）取 email → 写 `credentials.toml`；打印 `Logged in as <email>`；**/me 失败时仍持久化 `{ server, token }`（email 留空）+ warn，绝不丢已铸 token**（防孤儿 PAT）。移除本文件的 `rpassword` 调用
- [x] 7.3 [code] `main.rs`：`Command::Login { server, email }` → `Login { server }`（删 `--email` arg 及其 doc 注释）；`dispatch` arm 改 `Command::Login { server } => commands::login::run(server).await?`；`login::run` 签名去掉 email 参数
- [x] 7.4 [code] `credentials.rs`：`Credentials.email` 改 `Option<String>`（/me 失败时 None；成功回填）；`auth.rs` resolve 不变（不读 email）；`logout.rs` 若读 email 做相应处理
- [x] 7.5 [test] CLI 单测（bin crate `#[cfg(test)]`）：device/token 错误码 → CLI 状态机分支映射（纯逻辑，不起 server）；poll 退避计算

## 8. Admin SPA: routes/device.tsx（public 顶层）

- [x] 8.1 [code] `lib/api/device.ts`：`deviceLookupQueryOptions(user_code)` + approve/deny mutationOptions（typed `$api`）
- [x] 8.2 [code] 新建 `apps/admin/src/routes/device.tsx`（public，仿 `accept-invite.tsx`）：`validateSearch: z.object({ user_code: z.string().optional() })`；`me` 401 → 「Sign in to continue」`Link` 到 `/login?next=` + `encodeURIComponent(/device?user_code=…)`
- [x] 8.3 [code] 已登录：user_code 预填输入框（无则手输）→ lookup 展示 `client_name`(内嵌 host)/expires → Approve/Deny 按钮 → 成功 `Result` "回到终端"；过期/未知 → 友好 Alert
- [x] 8.4 [code] **改造 `login.tsx` 成功跳转**（非「确认」，是明确 [code]）：现状 `router.navigate({ to: next })` 的 `to` 不解析 query、会丢 `?user_code`。改为 `router.navigate({ href: next })`（v1 支持 href），或 `const u=new URL(next, location.origin); router.navigate({ to:u.pathname, search:Object.fromEntries(u.searchParams) })`，确保登录后回跳 `/device?user_code=…` query 不丢
- [x] 8.5 [code] 跑 `pnpm --filter @swarm-hive/admin openapi` 重生成 `schema.gen.ts`（device 端点 + 删 cli-token）；`git add`
- [x] 8.6 [test] Vitest：device 页纯逻辑（next 编码/解码 → 确认 `/device?user_code=WDJB-MJHT` 经 encode→login next→decode 后 query 不丢 / user_code 格式化）+ `lib/api/device` 形状；整页渲染 deferred 到 foundation harness（同 apps/releases 页）

## 9. Docs / memory 同步

- [x] 9.1 [docs] `dev-notes/knowledge/backend.md` **全文扫 cli-token**（实测残留在 ~227 verify_password 复用注释、~239 鉴权段「CLI 专用 endpoint」句、~241 相关文件、~277 「cli-token 路径不受影响」账号锁注释）：逐处改为 device flow 描述/删；新增「Device flow（RFC 8628）」小节（状态机含 not-found→invalid_grant + 原子 claim + slow_down 语义 + RFC 8628 错误格式破例 + Session-only 批准 + bootstrap 排除 + governor 在 sensitive 子树 + token 铸造复用）
- [x] 9.2 [docs] `dev-notes/knowledge/backend.md` 「CLI publish/verify 上传链路」段：更新鉴权描述（login 走 device flow，不再 ROPC）
- [x] 9.3 [docs] `dev-notes/knowledge/admin-spa.md`：补 `/device` 公开页约定（为何不放 `_auth/`、login.tsx 的 `next` 需 href 形态保 search param）
- [x] 9.4 [docs] `docs/12-cli.md` login 段：device flow 步骤 + 截图位；**`docs/13-rbac.md` 「CLI 凭证流」段（~336-349）改写为 device flow，删「2. POST /api/v1/auth/cli-token」与「专用 endpoint 而非复用 /auth/login」论证**；`docs/09-mvp-roadmap.md:67` 的 cli-token 引用（按需保留为历史 ✅ 条目或更新）；`CLAUDE.md` Common commands 的 `swarmhive login` 注释更新（去掉 email/password 描述，改 device flow）
- [x] 9.5 [docs] `openspec/changes/README.md`：依赖图加 `add-cli-device-login` 节点（挂 pat-and-api-token + login-bootstrap-ui，旁路 oauth）+ 进度表 + 阶段 5 映射
- [x] 9.6 [docs] `memory/`：新增/更新 CLI auth 决策条目（device flow over loopback 的理由、ROPC 废弃）

## 10. 跨 proposal 联动

- [x] 10.1 [docs] `add-oauth-github-and-provider-config/proposal.md`：补交叉引用（CLI device login 复用 `/login`，OAuth 用户自动获 CLI 能力）+ 校准 "不影响 CLI" 措辞（本 proposal 不改 CLI 代码，但 CLI 通过复用本页间接受益）
- [x] 10.2 [code] `grep -rn "cli-token\|cli_token\|CliToken" openspec/changes/ docs/ memory/ crates/ dev-notes/`（**含 dev-notes/**，否则 backend.md 残留漏网）确认无残留引用幽灵端点；archive/ 下的历史记录与 docs/09 的 ✅ 历史条目按需豁免

## 11. 端到端验证

- [ ] 11.1 [code] 本地 docker postgres + cargo run + pnpm admin:dev（`base_url=http://localhost:5173`）：`swarmhive login http://localhost:3030` → 浏览器开 `/device?user_code=` → 已登录 owner 批准 → CLI 写 token → `swarmhive apps list` 用新 token 成功
- [x] 11.2 [code] 拒绝路径 + 过期路径（手动等 15min 或临时调短 TTL）人工验证
- [x] 11.3 [code] 全 gate：`cargo fmt --all` / `cargo clippy --workspace --all-targets -D warnings` / `cargo test --workspace` / `pnpm lint` / `pnpm --filter @swarm-hive/admin typecheck` / `cargo tree -p swarmhive-cli | grep sea-orm`(空)
