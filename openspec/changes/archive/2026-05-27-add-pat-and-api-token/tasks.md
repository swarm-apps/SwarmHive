# tasks

## 1. Entity + schema

- [x] 1.1 [code] `crates/swarmhive-entity/src/api_token.rs`：`ApiTokenKind` enum (`pat | api`) + `#[sea_orm::model]` Model（字段见 design Schema 段）；`belongs_to user`；`From<&Model> for api::ApiToken`（DTO 不带明文 token、不带 token_hash）
- [x] 1.2 [code] `crates/swarmhive-api-types/src/lib.rs`：新增 `ApiToken` / `ApiTokenKind` / `CreateTokenRequest` / `CreateTokenResponse` / `CliTokenRequest` / `CliTokenResponse` DTOs（serde + utoipa::ToSchema + garde）
- [x] 1.3 [code] `crates/swarmhive-entity/src/lib.rs`：`pub mod api_token;`（registry 自动收）
- [x] 1.4 [code] `crates/swarmhive-server/src/db.rs`：schema-sync 通过 `#[sea_orm(unique)]` 自动建 UNIQUE(token_hash) 索引；其余次级索引按需在后续 admin list endpoint 出现性能问题时再加（YAGNI）
- [x] 1.5 [test] db smoke 集成测试 `tests/db_smoke.rs` 中添加 `api_token_table_synced_and_unique_index` 测试：connect + sync + insert PAT + 第二次同 hash insert 触发 unique violation

## 2. Token mint + hash 工具

- [x] 2.1 [code] `crates/swarmhive-server/src/auth/token.rs`：`mint(kind) -> (plain, prefix, blake3_hex)`；OsRng 32 字节 → base64url no-pad → `swhv_pat_` / `swhv_api_` 前缀；prefix = plain 前 12 char。**改用 hex string** 而非 `[u8;32]` 以与 setup_token 一致
- [x] 2.2 [code] 同文件：`parse(plain: &str) -> Option<(ApiTokenKind, String)>` 返回 (kind, blake3 hex)。Authorization 头剥离 `Bearer ` 由 caller 负责
- [x] 2.3 [test] `auth::token::tests` 4 unit tests：mint 长度/前缀、roundtrip、parse 拒绝（空/短/长/非 url-safe/带 padding/错前缀）、distinct hash

## 3. Bearer 鉴权链路

- [x] 3.1 [code] `auth/bearer.rs::resolve`：parse Authorization → blake3 → find → revoked/expired/inactive-owner 检查 → kind 分支（PAT live perm via `service::load_user_permissions`，API snapshot via JSONB → Vec<PermissionName>）
- [x] 3.2 [code] 同文件 `touch_last_used`：1-min 节流 UPDATE via `Statement::from_sql_and_values` + `execute_raw`；首次 NULL→Some 写 `auth:token_used_first_time` audit (actor_type=token)
- [x] 3.3 [code] `auth/extractor.rs`：删 Bearer 短路；Authorization 存在 → `bearer::resolve()`；无 Authorization → 走 cookie。malformed header 不回退 cookie
- [x] 3.4 [test] `tests/bearer_smoke.rs` 6 测试：PAT happy / malformed Bearer + cookie 仍 401 / revoked 401 / expired 401 / API token snapshot perms / last_used_at 节流 + first-use audit 仅 1 次

## 4. Token CRUD endpoints

- [x] 4.1 [code] `services/token.rs::create` (mint + INSERT + audit token_created)；`validate_permissions` 强制 PAT 无 perms / API perms ⊆ creator
- [x] 4.2 [code] 同文件 `revoke`：load → owner 或 `token:manage` 否则 403；UPDATE revoked_at；audit token_revoked；idempotent（重复撤销返 Ok）
- [x] 4.3 [code] 同文件 `list`：跨 user 列表要 `token:manage`；按 `created_at DESC` 返回 DTO
- [x] 4.4 [code] `routes/tokens.rs`：3 个 handler + `#[utoipa::path]`，tag = "tokens"；用 3 个 `.routes(routes!(...))` 分别注册（不同 path 不能合并）；create 显式 `require_permission!(TokenManage)`，revoke 在 service 里查
- [x] 4.5 [code] `lib.rs::build_router`：把 `routes::tokens::router()` merge 进 api_router，走 session_layer（不走 cli-token 的严格 governor，复用全局限流）
- [x] 4.6 [code] `openapi.rs`：新增 tag `tokens`；ApiToken/ApiTokenKind/CreateTokenRequest/CreateTokenResponse 通过 handler 注解自动注册到 components.schemas（utoipa 5 自动收集 request_body/response body 的 ToSchema）
- [x] 4.7 [test] 扩展 `tests/openapi_surface.rs` 的 ENDPOINTS / ERROR_BEARING_ENDPOINTS / EXPECTED_TAGS / EXPECTED_SCHEMAS 覆盖新加的 2 个 path + 4 个 schema + tokens tag，5 个测试保持通过

## 5. CLI login endpoint

- [x] 5.1 [code] `auth/service.rs::verify_password` 抽出复用（owner login + cli-token 共享 argon2 路径 + DUMMY_PHC 等时）；`routes/auth.rs::cli_token` handler 调它 → 构 Principal → `token_service::create(kind=pat)` → 返回 `CliTokenResponse`
- [x] 5.2 [code] `#[utoipa::path]` post /api/v1/auth/cli-token, tag="auth", request_body CliTokenReq, response CliTokenResponse + ApiErrorResponses
- [x] 5.3 [code] cli-token route 通过 `routes!(cli_token)` 加到 `routes::auth::router()`，与 login/logout/me 同 router；自动享受 sensitive 子路由的 5 rps governor
- [x] 5.4 [test] `tests/cli_token_smoke.rs` 3 测试：happy path (PAT → /me 200) / wrong password 401 + 无 audit + 无 row / 80 次 burst 触发 429
- [x] 5.5 [test] `tests/openapi_surface.rs` 已自动覆盖新 cli-token endpoint（无需额外修改，因为 EXPECTED_TAGS / EXPECTED_SCHEMAS 已含 auth tag + CliTokenResponse 由 cli-token handler 注解自动注册）

## 6. CLI: `swarmhive login` / `logout` / token loading

- [x] 6.1 [code] workspace `Cargo.toml` 添加 `directories = "5"` / `rpassword = "7"` / `hostname = "0.4"`；CLI Cargo.toml 引用三者 + 已有 reqwest/toml/serde/serde_json/anyhow
- [x] 6.2 [code] `crates/swarmhive-cli/src/credentials.rs`：`Credentials { server, email, token }` + `path()`（via `directories::ProjectDirs::from("dev","swarmhive","swarmhive")`）+ `load() / save() / delete()`；unix 上 `set_mode(0o600)`，non-unix warn-only
- [x] 6.3 [code] `crates/swarmhive-cli/src/auth.rs`：`resolve() -> Bearer` 优先 `SWARMHIVE_TOKEN` env → fallback `Credentials::load()`；当前 publish/promote 还是 todo，故 `#[allow(dead_code)]` 标注（add-app-release-artifact 实施时移除）
- [x] 6.4 [code] `commands/login.rs`：可选 server URL 参数（默认 `http://localhost:3030`）+ `--email` flag；`rpassword::prompt_password` 读密码；`token_name = format!("{host}-{ts}")`；POST `/api/v1/auth/cli-token` → 写 credentials；错误体解析 `problem+json.detail` 友好显示
- [x] 6.5 [code] `commands/logout.rs`：GET `/api/v1/tokens` 按 prefix 匹配找当前 token id → DELETE；server 失败仅 warn 不阻塞本地文件清理（offline-friendly）
- [x] 6.6 [code] `main.rs`：新增 `Login { server, email }` + `Logout` subcommands，wire 到 commands；其它 stub 保留 `todo!()` 不变
- [x] 6.7 [test] 编译 + clippy `-D warnings` 通过（workspace）；端到端验证留到 Group 7 docs 同步后做手动 smoke 即可。`cargo tree -p swarmhive-cli | grep sea-orm` 为空，符合 crate 边界约束

## 7. 文档同步

- [x] 7.1 [docs] [docs/13-rbac.md](../../../docs/13-rbac.md) "三类凭证" 段重写：实际 token 字符串格式（`swhv_pat_<43>` / `swhv_api_<43>`）+ Bearer > cookie 优先级 + 撤销立即生效 + last_used_at 节流；新增"PAT 与 API Token 权限模型"小节（live vs snapshot 表格）；新增"CLI 凭证流"小节；审计列表加入三个 token 事件
- [x] 7.2 [docs] [docs/12-cli.md](../../../docs/12-cli.md) "认证" 段重写：login/logout/SWARMHIVE_TOKEN 三层优先级 + 命令示例 + 三平台凭证文件路径
- [x] 7.3 [docs] [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2 任务行 + 验收行更新（CI token / 审计日志 ✅）；阶段 5 任务 `swarmhive login` 标 ✅
- [x] 7.4 [docs] [CLAUDE.md](../../../CLAUDE.md) endpoint 清单加 `/api/v1/auth/cli-token` + `/api/v1/tokens(/{id})`；CLI 命令段加 `swarmhive login` / `logout` 用法 + 测试段补 bearer_smoke / cli_token_smoke
- [x] 7.5 [docs] [openspec/changes/README.md](../README.md) 当前进度表行更新到 "apply 完成 (35/35 tasks)"
- [x] 7.6 [docs] [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) "三类凭证" 段补 token 格式 + prefix 列 + blake3 hex 决策；新增"Bearer 鉴权链路"小节（live vs snapshot / 节流 SQL / `execute_raw(Statement)` 入口 / `verify_password` 抽出复用）+ "Token CRUD endpoints"小节
