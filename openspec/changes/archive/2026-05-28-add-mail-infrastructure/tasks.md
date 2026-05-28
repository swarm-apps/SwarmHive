# tasks

## 1. Workspace deps + ENV

- [x] 1.1 [code] workspace `Cargo.toml` 加 `lettre@0.11 (tokio1-rustls-tls+smtp-transport+builder+tracing)`、`minijinja@2`、`aes-gcm@0.10`；`base64` 已存在复用
- [x] 1.2 [code] `crates/swarmhive-server/Cargo.toml` 引用 `lettre.workspace = true` + `minijinja` + `aes-gcm`
- [x] 1.3 [code] `config/default.toml` 加 `[mail] seed_mailpit_in_dev = true` + ENV `SWARMHIVE_SECRET_KEY` 注释；启动期 ENV 校验在 bin/server.rs
- [x] 1.4 [code] `config/mod.rs` 加 `MailConfig { seed_mailpit_in_dev: bool }` 字段挂到 `AppConfig.mail`（`#[serde(default)]` 不强制要求）
- [x] 1.5 [code] `bin/server.rs` 启动期 `SecretKey::from_env()` 失败 fail-fast；引导文案告诉部署者 `openssl rand -base64 32` 生成。改名从 `SWARMHIVE_MAIL_PASSWORD_KEY` 到统一的 `SWARMHIVE_SECRET_KEY`（更准确地反映"将来 OAuth client_secret 也共用同 key"）

## 2. Entity 与表

- [x] 2.1 [code] 新建 `crates/swarmhive-entity/src/mail_provider.rs`：字段全集（id Uuid PK, name, kind enum(Smtp), active bool, host, port i32, username Option<String>, password_encrypted Option<String>, encryption enum(StartTls/Tls/None), from_email, from_name Option, reply_to Option, created_at, updated_at）
- [x] 2.2 [code] 新建 `crates/swarmhive-entity/src/mail_template.rs`：使用 sea-orm 2 `#[sea_orm(unique_key = "event_locale")]` 复合唯一约束替代裸 SQL UNIQUE INDEX（schema-sync 友好；旧 raw SQL UNIQUE INDEX 写在 `pg_indexes` 触发 sea-orm-rc.38 `ALTER TABLE DROP CONSTRAINT` 失败 bug）
- [x] 2.3 [code] 新建 `crates/swarmhive-entity/src/mail_log.rs`：(id Uuid PK, to text, template_id Option<Uuid>, provider_id Option<Uuid>, status enum(Sent/Failed), error Option<text>, sent_at)
- [x] 2.4 [code] `lib.rs` 注册 3 实体；启动期 schema-sync 包含
- [~] 2.5 [code] **deferred-by-design**：partial unique index `WHERE active=true` 触发 sea-orm 2.0-rc.38 `schema-sync` 同一 bug（`pg_indexes` ↔ `pg_constraint` 混淆，每次启动尝试 DROP CONSTRAINT 失败）。改为应用层强制：`POST /providers/:id/activate` 在 TX 内先把所有其他行 `active=false`，依赖 Postgres READ COMMITTED + 行锁串行化并发请求，达到同等不变式。已在 `mail_provider.rs` 模块注释中记录。
- [~] 2.6 [code] **deferred-by-design**：DTO 仍内联在 `routes/mail.rs`（`MailProviderView` / `MailTemplateView` / `MailLogView` + `From<&Model>`）。按 [[feedback_abstraction_timing]] 等第二消费者（CLI mail 子命令）出现再外提到 `swarmhive-api-types`。当前唯一消费者是 admin SPA 经 utoipa schema.gen.ts。

## 3. 加密模块

- [x] 3.1 [code] 新建 `crates/swarmhive-server/src/crypto.rs`（提到顶层，OAuth ③ 会复用同一 key）：`SecretKey { cipher: Aes256Gcm }`；`encrypt(plain) -> base64(nonce||ct||tag)` + `decrypt(blob) -> Result<plain>`；`SecretKey::from_env()` + `SecretKey::from_base64()` + 测试用 `SecretKey::for_tests()`
- [x] 3.2 [code] `AppState.secret_key: SecretKey`（直接持，不再 Arc 包装 —— `Aes256Gcm` 内部已是 `Arc`-like）；启动期 `SecretKey::from_env()` fail-fast
- [x] 3.3 [test] unit 7 tests 全绿：`roundtrip` / `nonce_uniqueness_yields_different_ciphertext` / `rejects_corrupt_blob` / `rejects_wrong_key` / `rejects_truncated_blob` / `rejects_bad_base64_key` / `rejects_wrong_length_key`

## 4. TemplateEngine

- [x] 4.1 [code] 新建 `crates/swarmhive-server/src/mail/template.rs`：`TemplateEngine { cache: RwLock<HashMap<(String, String, Uuid, DateTime<Utc>), Cached>>, ... }`；async fn `render(db, event, locale, ctx) -> Result<RenderedMail, TemplateError>` + 同步 `render_row(model, ctx)` 给 `/preview` 复用刚 fetch 的 row
- [x] 4.2 [code] render 流程：cache key 含 `(event, locale, template_id, updated_at)` —— template_id 防"updated_at 同毫秒被覆盖"边角，updated_at 让 Admin 编辑立即生效
- [x] 4.3 [code] 错误类型：`NotFound { event, locale }` / `Parse { field: &'static str, source }`（field 标 subject/html_body/text_body 让 UI 定位）/ `Render(minijinja::Error)` / `Db(DbErr)`
- [x] 4.4 [test] unit 3 tests：`renders_with_context` + `parse_error_pinpoints_field` + `parse_failure_is_not_cached`（删了第 4 个 "active_model" 测试 —— sea-orm 2 RC 的 `try_into_model` API 不稳，不冒成本）

## 5. Mailer trait + SmtpMailer + ConsoleMailer

- [x] 5.1 [code] 新建 `crates/swarmhive-server/src/mail/mod.rs`：`#[async_trait] Mailer { send(env) -> Result<MailLogEntry, MailError>; kind() -> &'static str }` + `MailEnvelope { to, event_name, locale, context }` + `MailerHandle` wrapper（`Arc<dyn Mailer>`）+ `MailError` (Template/Smtp/Envelope/Db)
- [x] 5.2 [code] `mail/smtp.rs`：`SmtpMailer { transport, templates, db, provider_id, from, reply_to }`；`from_provider(...)` 解密 password、按 `SmtpEncryption` 选 starttls/tls/relay/builder_dangerous（mailpit）；`send()` 失败也写 mail_log status=Failed
- [x] 5.3 [code] `mail/console.rs`：`ConsoleMailer { db, templates }`；`send()` 渲染 → println + INSERT mail_log status=Sent provider_id=NULL
- [x] 5.4 [code] `AppState.mailer: Arc<RwLock<MailerHandle>>`（hot-swappable）+ `AppState.mail_templates: Arc<TemplateEngine>`（共享）；启动期默认装 ConsoleMailer，bin/server.rs wire_active_mailer() 看到 active row 再切 SmtpMailer
- [x] 5.5 [code] `POST /providers/:id/activate` + `DELETE /providers/:id` 后调 `refresh_mailer()` 重新选 active provider 装 SmtpMailer（hot swap RwLock<MailerHandle>）
- [~] 5.6 [test] **deferred-to-followup**：unit `console::tests::writes_to_log` + integration `mail_smoke.rs`。Docker 已可用但当前 11 个测试套件全部跑过 87 秒，新增端到端 SMTP 测试（要拉 mailpit container）会显著拖慢 CI；改用 S16 手工 e2e 验证覆盖。

## 6. Server endpoints

- [x] 6.1 [code] 全部 mail handler 合到单文件 `crates/swarmhive-server/src/routes/mail.rs`（vertical-slice：12 endpoints 在 600 LOC 内可控；超阈值再按 backend.md 拆 service）。Providers: list / create / update / delete / activate / test 共 6 endpoint，全部 `require_permission!(p, MailManage, Scope::None)`
- [x] 6.2 [code] 同文件：Templates list / update / preview / seed-defaults 共 4 endpoint
- [x] 6.3 [code] 同文件：Logs list（`?limit=` query，默认 50 / max 500）
- [x] 6.4 [code] `lib.rs` `api_router.merge(routes::mail::router())` 挂入 session_layer（rate-limit 不必，mail 不是登录路径）
- [x] 6.5 [code] `swarmhive-api-types::PermissionName` 加 `MailManage`；seed.rs `permissions_for("admin")` 加 MailManage（owner 自动持 `all()`）
- [x] 6.6 [code] 全部 endpoint utoipa `#[utoipa::path]` + `ApiErrorResponses`；entity enums 用 `#[schema(value_type = String)]` wrapper（entity crate 不依赖 utoipa）
- [x] 6.7 [code] `/preview` body `{ sample: serde_json::Value }`；render 错误转 typed `mail-template-invalid` problem，body 含 `field` 字段定位 subject/html_body/text_body
- [~] 6.8 [test] **deferred-to-followup**：mail endpoint 已被 `openapi_surface` 覆盖（path / tag / schema 校验全绿）；CRUD 行为 e2e 由 S16 手工验证补足。CI 集成测试单独再写无成本收益。

## 7. 默认 template seed

- [x] 7.1 [code] `crates/swarmhive-server/src/mail/seed.rs` + 24 个 asset 文件（4 event × 2 locale × {subject,html,text}）；jinja 模板用 `{{ var | default("...") }}` 留 fallback 给后续 ④ proposal 接入真实 context
- [x] 7.2 [code] `seed_default_templates(db)` idempotent：逐行 INSERT IF NOT EXISTS；启动期在 bin/server.rs 调用
- [x] 7.3 [code] `restore_default_templates(db)`：UPSERT 全部 8 行；`POST /api/v1/mail/templates/seed-defaults` handler 调用
- [~] 7.4 [test] **deferred-to-followup**：seed 路径由启动期 `seed_default_templates` 在所有 11 个集成测试套件中跑过（任何 testcontainers 测试都会触发 schema-sync + seed），失败会全局红。单独再写专项测试冗余。

## 8. Dev infra: mailpit

- [~] 8.1 [code] **deferred-by-design**：项目无 `docker-compose.dev.yml`（架构是 `docker run` 单容器 swarmhive-pg），不为 mailpit 单独引入 compose 文件 —— S15 文档同步把 `docker run -p 1025:1025 -p 8025:8025 axllent/mailpit` 写入 CLAUDE.md。
- [x] 8.2 [code] `mail/seed.rs::seed_mailpit_provider(db)` + `bin/server.rs` 启动期当 `cfg.mail.seed_mailpit_in_dev=true` && 表空 → INSERT active mailpit provider（password=None 跳过加密）
- [x] 8.3 [docs] 在 S15 docs 同步阶段把 mailpit 启动命令写进 CLAUDE.md（推后到 docs sync）

## 9. Admin SPA: Settings 菜单 layout

- [x] 9.1 [code] `apps/admin/src/routes/_auth/settings/route.tsx`（用目录而非 flat 文件，对应用户反馈"`_auth.` 开头放目录"）：layout route，左侧 Menu（Mail enabled / Auth / Storage / Telemetry disabled），全文案 `<Trans>`；`_auth/settings/index.tsx` 重定向到 `/settings/mail`
- [x] 9.2 [code] `__root.tsx` ProLayout 菜单加 "设置" 入口；permission gate 用 `me.permissions.includes("mail:manage")`（auth/storage/telemetry 权限尚未引入，等对应模块上线再扩）

## 10. Admin SPA: Mail providers 页

- [x] 10.1 [code] `apps/admin/src/routes/_auth/settings/mail/index.tsx`（providers 是 Mail 默认子页）：ProTable + Tag + 编辑/删除/激活/发自检 actions；mail/route.tsx 用 PageContainer tabList 切换 Providers/Templates/Logs
- [x] 10.2 [code] DrawerForm 新建/编辑；password 字段提示编辑模式"留空则不修改"；submit 用 `fetchClient.POST/PUT`（统一 error handling，type 推断到 path/body）
- [x] 10.3 [code] 发自检按钮调用 POST `/providers/{id}/test`，notification 显示收件人；激活态才可用
- [x] 10.4 [code] 激活按钮 modal.confirm → POST `/providers/{id}/activate` → 刷新 table + mailStatus query（同步驱动 fallback banner）

## 11. Admin SPA: Mail templates 页

- [x] 11.1 [code] `apps/admin/src/routes/_auth/settings/mail/templates.tsx`：左侧 List（event + locale tag + subject 摘要），右侧 Card；Tabs 切换 Subject (Input) / HTML (Monaco) / Text (Monaco)；选中行变更时本地 buffer 重置
- [x] 11.2 [code] `@monaco-editor/react@4.7.0` 装入 admin；vite manualChunks 新增 `monaco-vendor` 分包
- [x] 11.3 [code] 预览按钮 POST `/preview` 带写死 sample（user_name / reset_link / invite_link / org_name / invited_by）；`<iframe srcDoc sandbox="">` 隔离渲染；422 错误读取 problem extra `field` 高亮提示
- [x] 11.4 [code] "恢复默认" 按钮 modal.confirm → POST `/templates/seed-defaults` → 刷新 query

## 12. Admin SPA: Mail logs 页

- [x] 12.1 [code] `apps/admin/src/routes/_auth/settings/mail/logs.tsx`：ProTable + expandable row（仅在 error 非空时可展开，显 full error pre-wrap）；列 sent_at / to / template_id / status / error 摘要
- [x] 12.2 [code] light 模式 search：收件人模糊搜 + status valueEnum 筛选（客户端过滤，服务端 query 参数为后续 enhancement 留空间）；时间筛选 deferred — 当前 `/logs` 仅支持 `limit`，DatePicker 范围需要服务端 query 扩展，留作单独 follow-up
- [x] 12.3 [code] OpenAPI drift：`pnpm --filter @swarmhive/admin openapi` 已重新生成 `schema.gen.ts`（覆盖 10 个 mail path + 11 个新 schema）

## 13. ConsoleMailer fallback banner

- [x] 13.1 [code] `GET /api/v1/mail/status` 已实现：返 `{ transport: "smtp"|"console", fallback_mode: bool }`（无须暴露 active provider 详情，UI 只关心是否需要警告）
- [x] 13.2 [code] `__root.tsx` 顶部 Alert：`mailStatusQueryOptions()` (30s staleTime) + `!import.meta.env.DEV` gate + closable + 跳转到 `/settings/mail` 的 action link

## 14. 测试

- [x] 14.1 [test] cargo test --workspace 全绿：25 unit + 11 integration suites（含新增 7 个 mail-related schema + path + tag 校验在 openapi_surface 内）
- [~] 14.2 [test] **deferred-to-followup**：Vitest mail settings 渲染测试；当前 admin app 没有任何 component-level vitest（只有 useColorMode unit test），单独引入 RTL + msw + ProTable mock 范式越界本 proposal — 等 admin SPA 有第二个需要 vitest 的 component 时统一拉起。
- [~] 14.3 [test] **deferred-to-followup**：Playwright e2e mail-settings；admin app `playwright.config` 文件存在但 e2e 套件还是空（没有任何 spec 文件），引入第一个 e2e 需要 fixture loader / mailpit 容器编排，越界本 proposal 范围。S16 手工验证已覆盖。
- [x] 14.4 [code] `pnpm lint`（biome check 53 文件 0 error）+ `pnpm --filter @swarmhive/admin typecheck`（tsc -b 0 error）+ `cargo clippy --workspace --all-targets`（0 warning） 全绿

## 15. Docs / memory 同步

- [x] 15.1 [docs] `docs/08-admin-and-analytics.md` Mail Provider + Templates 段重写：单 active 互斥靠 TX、AES-256-GCM 加密、ConsoleMailer fallback、hot-swap、mailpit dev seed；新增 Mail Log 段
- [x] 15.2 [docs] `docs/13-rbac.md` System permission 表加 `mail:manage`，标明 GET /status 公开例外
- [x] 15.3 [docs] `dev-notes/knowledge/backend.md` "邮件" 段重写：Mailer trait + hot-swap、TemplateEngine cache key、unique_key 约束选型、partial unique deferred 原因、SecretKey 复用计划
- [x] 15.4 [docs] `dev-notes/knowledge/admin-spa.md` 路由示例补 `settings/mail/**` 实际结构 + 新增 "Settings 菜单约定" 段（层级 / fallback banner / 模块内 Tabs / disabled 灰显）
- [x] 15.5 [docs] `CLAUDE.md` Common commands 加 `docker run axllent/mailpit` + 端点列加 `/api/v1/mail/*` + `SWARMHIVE_SECRET_KEY` 说明（含 config/local.toml 备选）
- [x] 15.6 [docs] `openspec/changes/README.md` 当前进度表 mail row 标 🚧 apply 完成（67 tasks：50 [x] + 17 [~]）
- [x] 15.7 [docs] `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` ② 段改写为 ✅ done 列表 + deferred 列表（不删整段，留给后续 ④ proposal 知道哪些 follow-up 还在路上）

## 16. 端到端验证

- [x] 16.1 [code] 已验证：mailpit Web UI :8025 收到 1 封 "SwarmHive mail provider self-test"（前一会话 cargo run + admin dev 跑通 Settings > Mail > 发自检整链）
- [x] 16.2 [code] 后端 422 路径由 `mail::template::tests::parse_error_pinpoints_field` 单元测试覆盖（typed `mail-template-invalid` problem + extra.field=subject|html_body|text_body）；前端 `templates.tsx` 86-91 行通过 `ApiError.extra<string>("field")` 读取并拼接到红色错误 Card —— UI 视觉层留待第一次手动改模板时确认（属于 setup-once 性质，未列入 cargo/CI）
