# Backend

## 概览

`swarmhive-server` lib 里的业务约定：sea-orm entity 写法、auth 鉴权链路、storage trait、mailer、错误响应、RBAC permission 校验。写 `crates/swarmhive-server/src/{auth,services,storage,mail,routes}/` 或 `crates/swarmhive-entity/src/` 时读这里。

## 模块组织规则（vertical slice + 横切层）

server 内部不走 layer-based (`controllers/` + `services/` + `repos/`)、不走 hexagonal、不走 NestJS 风格 `feature/{handler,service,dto}.rs`。**采用 vertical slice：每个 HTTP 业务一个 `routes/<feature>.rs` 文件，handler + DTO + 业务逻辑同文件**。横切关注点（被多 route / extractor / bin 复用的）放 `auth/` / `services/` 顶层。

**Why**：30+ 真实 Rust axum 仓库调研：launchbadge/realworld（sqlx 团队官方）、atuin-server、rustfulapi 全是单文件 vertical slice 或 layer-flat；**零仓库**采用 `feature/{handler,service,dto}.rs` 三件套。Rust 没 DI 容器、`#[derive]` 让 DTO < 15 行、`pub use` 在 Rust 圈不常见——NestJS/Spring 那套理由全部不成立，强切只会增加 import path 段数（4-5 段）和文件碎片。

### 拆分阈值（硬规则）

| 触发条件 | 动作 |
|---|---|
| 单 feature ≤ **250 LOC** 且 ≤ **5 endpoint** | **不拆**。handler + DTO + 内部 helper 全放 `routes/<feature>.rs` |
| 250-400 LOC 或 6-10 endpoint | 拆出 service：`routes/<feature>.rs + routes/<feature>/service.rs`（Rust 2018 sibling，**不要 mod.rs**） |
| > 400 LOC，或 > 10 endpoint，或同 service 被 ≥ 2 个 route 复用 | 拆 service + dto：`routes/<feature>.rs + routes/<feature>/{service.rs,dto.rs}`。**永远不预先拆 `handler.rs`**——`<feature>.rs` 本身就是 handler 容器 |
| 同一函数被 ≥ 2 个 route 文件复用 | 提到 `services/<topic>.rs` 顶层（参考 `services/token.rs`、`services/audit.rs`） |
| 函数被 extractor / bearer / bin 复用 | 提到 `auth/service.rs`（横切，非 feature） |
| DTO 在 ≥ 2 个 route 间共享 | 提到 `swarmhive-api-types` crate（不在 server 内 cross-import） |

### 命名

- HTTP 接入层叫 **`routes/`**（与 axum 圈 7:3 偏好的 `handler` 一致；`controller` 是 Rails/NestJS 风，本项目不用）
- 横切复用业务叫 `services/`（services/audit, services/token, services/seed）
- 鉴权 + 横切安全基础设施叫 `auth/`（principal, extractor, bearer, password, session, token util, permission 宏）

### 反面案例（不要这样做）

- 把单 route 用一次的 service 函数（如 `register_owner`）抽到 `auth/service.rs` 增加跨文件跳转——**已踩过坑并回滚**：`add-pat-and-api-token` apply 后期把 `auth/service.rs` 从 450 行回收到 253 行，就是把 `login` / `logout` / `register_owner` / `setup_required` 4 个单 caller 函数下沉回各自 route 文件（参考 git log + `openspec/changes/archive/`）
- 提前给 `mail/` `storage/` 这种只有 `mod.rs` 的占位目录——用顶层 `mail.rs` `storage.rs` 平铺，要拆 driver 时再升 sibling

**相关文件**：`crates/swarmhive-server/src/{routes,auth,services}/`。

## 数据库

### Postgres only（不保留 SQLite 路径）

整个项目唯一数据库后端是 PostgreSQL。dev 用 coolify-managed 实例；single-server 部署通过 compose profile 同机起 Postgres + RustFS。

**Why**：用户 2026-05-25 explore 阶段拍板的决策。SQLite/Postgres 双轨会导致 SQL 方言、migration 工具、并发模型都要双倍维护，得不偿失。

**正确做法**：
- sea-orm features 只开 `sqlx-postgres`
- 可放心使用 Postgres 特性：JSONB、ILIKE、partial index、`ON CONFLICT DO UPDATE`、`SKIP LOCKED`、LISTEN/NOTIFY、BRIN
- testcontainers 测试用 `testcontainers-modules` 的 Postgres image

**不要做**：
- 不要写"兼容 SQLite"的 fallback 查询
- 不要为某个 query 写两份方言版本

**相关文件**：`crates/swarmhive-server/Cargo.toml` 的 sea-orm features、`memory/project-design-principles.md` 第 11 条。

### schema-sync only（不引入 sea-orm-migration crate）

schema 演进策略：`get_schema_registry("swarmhive_entity::*").sync(&db).await?`。**不**引入 `sea-orm-migration` crate。

**Why**：MVP 阶段 schema 还在迭代，displeasure-sync 提高节奏；真正生产升级压力出现时再决定要不要切到 migration crate。

**正确做法**：
- entity crate 顶层暴露 `pub const REGISTRY_GLOB: &str = "swarmhive_entity::*";`
- server 启动调 `db::sync` 时仅在 `config.database.auto_sync = true` 才跑（prod profile 默认 false）
- 生产 DBA 通过 `sea-orm-cli generate migration`（外部工具）或人工 SQL 控制 schema

**相关文件**：`crates/swarmhive-entity/src/lib.rs`、`crates/swarmhive-server/src/config/mod.rs`、`openspec/changes/add-persistence-foundation/design.md` "Schema 同步策略" 段。

### Entity 写法用 sea-orm 2.0 新格式

```rust
use sea_orm::entity::prelude::*;
use swarmhive_api_types as api;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub org_id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    // ...
    #[sea_orm(belongs_to, from = "org_id", to = "id")]
    pub organization: Option<crate::organization::Entity>,
    #[sea_orm(has_many)]
    pub identity_links: HasMany<crate::identity_link::Entity>,
}
impl ActiveModelBehavior for ActiveModel {}

impl From<&Model> for api::User { /* ... */ }
```

**正确做法**：
- 用 `#[sea_orm::model]` 新格式，不要旧的手写 Relation 枚举（sea-orm 1.x 风格）
- `From<&Model> for api::*` 转换写在 entity crate（不在 server）
- 主键用 `uuid v7`（已在 workspace dep）；时间字段 `chrono::DateTime<Utc>`
- 复合主键最多 12 字段（sea-orm 限制）

**详细参考**：调 `/sea-orm-2` skill 获取完整模式速查（Entity Loader、Nested ActiveModel、raw_sql! 宏、关系类型对照表）。

**相关文件**：`crates/swarmhive-entity/src/*.rs`、`openspec/changes/add-persistence-foundation/design.md` Entity 写法段。

### `ActiveModelBehavior::before_save` 的作用边界

每个 entity 的 `#[async_trait] impl ActiveModelBehavior for ActiveModel { async fn before_save(...) }` 自动填 `created_at` / `updated_at`，handler 写单条 insert/update 时不需要传时间戳。

**但 hook 只在以下路径生效**：

- `model.insert(db)` / `model.update(db)` / `model.save(db)`（单条 ActiveModel API）

**以下路径会跳过 before_save**：

- `Entity::insert(model).on_conflict(...).exec_*` —— upsert 路径
- `Entity::insert_many(rows).exec_*` —— 批量 path
- `Entity::insert(model).exec_without_returning(db)` —— bypass returning 的快速路径

**正确做法**：

- **handler 业务代码**：用 `model.save(db).await?`，时间戳由 hook 自动填
- **seed / 批量任务 / upsert**：显式 `Set(Utc::now())`（参考 `crates/swarmhive-server/src/services/seed.rs` 的注释）

**不要做**：
- 不要假设 `before_save` 在所有路径都触发——遇到 `null value violates not-null constraint` 通常就是这个 caveat。

## 鉴权

### 三类凭证，一套校验

| 凭证 | 用途 | 载体 |
|---|---|---|
| Session cookie | Admin SPA 浏览器 | HttpOnly + SameSite=Lax，session 行存 Postgres |
| PAT (Personal Access Token) | CLI `swarmhive login` | `~/.config/swarmhive/credentials.toml` 或 `SWARMHIVE_TOKEN` env |
| API Token (scoped) | CI/CD | env `SWARMHIVE_TOKEN`，per (app, channel, perms) scope |

三者在 `Principal` extractor 汇流到 `{ user, scope, permissions, auth_method }`。

**Why**：单 binary monolith 下 JWT 的 stateless 优势没用，撤销 / scope 重发反而是负担。三类长期 token 都 blake3 hash 存 DB，撤销立即生效。

**正确做法**：
- 用 `argon2id`（OWASP 2024 params: m=19456, t=2, p=1）hash 用户密码
- token 字符串格式 `swhv_pat_<43>` / `swhv_api_<43>`（kind 公开在前缀里便于日志泄露 grep；43 char = 32 字节 base64url-no-pad）
- DB 只存 token 的 `blake3` hex（64 char hex string）；明文仅在创建时返回一次
- `prefix` 列存明文前 12 char，admin/CLI 列表展示用——不暴露 secret，又能辨识 token

**不要做**：
- 不要引入 JWT（撤销难、scope 重发复杂、单 binary 无 stateless 收益）
- 不要把 PAT 和 API Token 当两件事——共用同一张表 `api_token` 与同一份鉴权基建，只是 `kind` + `permissions` 列语义不同
- 不要给 token_hash 用 `Vec<u8>`/`bytea` —— 64 char hex string 的 2x 体积开销可忽略，且字符串列在 grep/SQL 排查时更友好

**相关文件**：`docs/13-rbac.md`、`crates/swarmhive-server/src/auth/{principal,extractor,bearer,token,service,session,password,permission}.rs`、`crates/swarmhive-server/src/services/token.rs`。

### Bearer 鉴权链路（`add-pat-and-api-token`）

`Principal::from_request_parts` 先看 `Authorization: Bearer …` 头：

- 存在 → `auth::bearer::resolve()`：parse `swhv_(pat|api)_<43>` → blake3 → `api_token` 表查 → revoked/expired/owner-inactive 三道关 → 节流 UPDATE `last_used_at` + 首次写 `auth:token_used_first_time` audit
- 不存在 → 走 cookie session（已有路径）
- 存在但 parse 失败 → 直接 401，**不**回退 cookie（显式 header 必须胜出，否则 CLI 测试被旧浏览器 cookie 污染）

**正确做法**：
- PAT (kind=pat) 走 live：每请求 `service::load_user_permissions(owner_id)` 重新拉权限。撤角色后 PAT 立即收缩，这是特性不是 bug
- API Token (kind=api) 走 snapshot：`row.permissions` 列 deserialize 成 `HashSet<PermissionName>`，与 creator 当前权限解耦
- 创建 API Token 时强制 `permissions ⊆ creator.permissions`，超额返 422 + 列出超额项
- `last_used_at` 1-min 节流靠 `UPDATE … WHERE id=$1 AND (last_used_at IS NULL OR last_used_at < NOW() - INTERVAL '1 minute')`，单库 round-trip、无 race、无应用层缓存。用 `ConnectionTrait::execute_raw(Statement)` 调（sea-orm 2 raw SQL 入口）
- `auth::service::verify_password` 抽出来同时供 `/auth/login` 与 `/auth/cli-token` 复用，DUMMY_PHC 等时

**不要做**：
- 不要把 cookie 路径放在 Bearer 之前——显式 header 必须胜出
- 不要给 `last_used_at` 节流加 in-memory cache：多实例不一致，重启丢
- 不要在 `bearer::resolve()` 里写完整 audit（first-use 一次就够），高 QPS 下会撑爆 audit 表

### Token CRUD endpoints

- `POST /api/v1/tokens` 需 `token:manage`；PAT (`permissions IS NULL`) 与 API (`permissions = Some(subset)`) 强制规则在 `services::token::validate_permissions`
- `GET /api/v1/tokens?owner=...` 列他人需 `token:manage`；自己列自己无需特殊权限
- `DELETE /api/v1/tokens/:id` 业主或 `token:manage`；幂等（重复撤销返 Ok 不报错）
- `POST /api/v1/auth/cli-token` 是 CLI 专用 endpoint：单次 RTT 换 PAT，避免 CLI 维护 cookie jar。与 `/auth/login` 共享 5 rps / burst 20 governor

**相关文件**：`crates/swarmhive-server/src/routes/tokens.rs`、`crates/swarmhive-server/src/routes/auth.rs::cli_token`。

### Permission middleware

权限粒度是 verb-scoped（`release:publish`、`storage:manage` 等），不是行级。`RestrictedConnection` 不引入。

**正确做法**：
- handler 用 `require_permission!(principal, "release:publish", Scope::App(app_id))?;` 风格
- 失败返回 RFC 9457 `403 forbidden`（含 `required_permission` 字段）
- 敏感操作必须写 `audit_log` 行（actor_type、actor_id、action、resource_*、ip、user_agent）

**敏感操作清单**（必写 audit）：
登录成功 / 失败、创建/删除用户、修改角色、创建/撤销 token、修改 storage 配置、发布 release、promote / rollback / yank、修改强制更新策略。

**相关文件**：`docs/13-rbac.md` "敏感操作" / "审计日志" 段。

### Bootstrap window + 账号级软锁 + 密码强度（`add-login-and-owner-bootstrap-ui`）

Owner bootstrap 走 **Coolify 模式**（无 stdout setup token；user 表空时 `/setup` 裸表单，首人即 Owner）。两层防护补 baseline 安全：

**正确做法**：
- `AppState.bootstrap: Arc<BootstrapConfig>` 启动期一次性读 `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` env，process-lifetime immutable
- `bootstrap_state(db, &cfg)` 每请求 COUNT(user) 判断 `needs_bootstrap`；bootstrap 完成后 `locked_email` 自动消失（避免 stale env 误导）
- `POST /api/v1/setup` 三重守门：bootstrap window 关闭 → 410 typed `bootstrap-already-complete`；email mismatch 锁定 → 422 typed `bootstrap-email-mismatch`（body 含 `expected_email`）；密码弱 → 422 typed `password-too-weak`
- `user_login_attempts` 表 5/30min 软锁；锁定期 `/login` 返 410 typed `account-locked-until`，body 通过 `ApiError::Typed.extra` 携带 `locked_until` ISO-8601
- 锁定检查 **先于** 密码 verify —— 正确密码也会被锁挡掉（防"密码已知 mid-lockout 旁路"）
- 弱口令字典走 `include_str!` 嵌入 + `OnceLock<HashSet<&'static str>>` lazy load；规则 ≥12 字符 + ≥3 类 + 不在字典

**`ApiError::Typed` 变体**：

- 现有 `Unauthorized` / `Forbidden` / `NotFound` / `Validation` / `Conflict` / `Gone` 是固定 type_uri 的"通用桶"；**业务子类型**走新增的 `Typed { status, type_uri, title, detail, extra: serde_json::Map }`
- `extra` map 中的字段会 merge 进 problem+json 顶层 object，让前端用 `error.extra<T>(key)` 拿到（不需要解析二次 JSON）
- 何时新增 `type_uri`：spec scenario 显式要求前端按 type 分支 + 业务字段需透传给 UI（如 `locked_until` 倒计时、`expected_email` 错配提示）。一般 422/403/404 不必拆 sub-type

**不要做**：
- 不要为 setup endpoint 加任何 stdout token 模式 ——`add-login-and-owner-bootstrap-ui` 明确删除该路径，二轨会让安全模型反复横跳
- 不要把账号锁逻辑塞进 `verify_password`——保持 verify 只做"密码匹配"，锁逻辑在 handler，便于 cli-token 路径不受影响
- 不要在 `/login` 的密码强度上做严格校验（强校验只在 set / change / reset 路径；登录强校验会锁死老账号）

**相关文件**：`crates/swarmhive-server/src/auth/bootstrap.rs`、`crates/swarmhive-server/src/auth/password.rs::validate_strong_password`、`crates/swarmhive-server/src/routes/auth.rs` (LOGIN_LOCKOUT_THRESHOLD + check_account_lock / record_failed_attempt / clear_login_attempts)、`crates/swarmhive-entity/src/user_login_attempts.rs`、`crates/swarmhive-server/src/error.rs::Typed`、`crates/swarmhive-server/assets/weak-passwords-top100.txt`。

## 错误响应（RFC 9457 problem+json）

所有 4xx / 5xx 响应统一格式：

```json
{
  "type": "https://swarmhive.dev/errors/forbidden",
  "title": "Forbidden",
  "status": 403,
  "detail": "Missing permission: release:publish",
  "instance": "/api/v1/apps/swarmdrop/releases",
  "required_permission": "release:publish",
  "scope": "app:swarmdrop"
}
```

`Content-Type: application/problem+json`。

**正确做法**：
- `swarmhive-server::error::ApiError` 实现 `axum::response::IntoResponse`
- 域内错误用 `thiserror`（`AuthError`、`StorageError`、`ReleaseError` 等），绑定层用 `From<DomainError> for ApiError` 映射
- 用 `anyhow` 兜底，但仅在 handler 入口或 main

**相关文件**：`crates/swarmhive-server/src/error.rs`。

## Storage

### S3 trait + presign

详见 [architecture.md](architecture.md) "存储抽象" 段。SwarmHive 唯一 storage 后端是 S3-compatible（`aws-sdk-s3`）。

**正确做法**：
- `presign` 接口按文件粒度签名，TTL 5–10 min
- `complete` 接口幂等（Postgres `ON CONFLICT`）
- server HEAD 对象做 sanity check（size + etag），**不**二次下载校验 hash

**相关文件**：`crates/swarmhive-server/src/storage/mod.rs`（待 `add-storage-and-presign-upload` 填充）。

## 邮件

### lettre + minijinja + DB-backed templates（`add-mail-infrastructure`）

SMTP provider 配置和邮件模板存 DB，Admin 后台可编辑。dev 用 mailpit。

**正确做法**：

- `mail::Mailer` trait（`send(env) -> Result<MailLogEntry, MailError>` + `kind() -> &'static str`），`SmtpMailer`（lettre AsyncSmtpTransport）+ `ConsoleMailer`（dev / fallback）两种实现；`AppState.mailer = Arc<RwLock<MailerHandle>>` 支持 hot swap。
- 启动期 `wire_active_mailer()` 查 active provider；任何失败（DB 抖 / 密钥错 / 主机解析失败）回落 ConsoleMailer，server 继续启，Admin SPA 顶 banner 提示。
- `POST /providers/:id/activate` + `DELETE /providers/:id` 后调 `refresh_mailer()` 实时切换槽位，不需重启。
- minijinja 运行时渲染；`TemplateEngine` cache key `(event, locale, template_id, updated_at)` —— `updated_at` 保证 admin 编辑立即生效，`template_id` 防同毫秒 updated_at 覆盖。
- 4 event × 2 locale = 8 行默认模板（`user_invite` / `password_reset` / `email_verify` / `security_alert`，en + zh-CN）；首启 `seed_default_templates` idempotent INSERT-if-not-exists；`restore_default_templates` UPSERT 全 8 行。
- 复合唯一 `(event_name, locale)` 用 sea-orm 2 `#[sea_orm(unique_key = "event_locale")]` 同标签字段对表达（**不**用 raw `CREATE UNIQUE INDEX`，会触发 sea-orm rc.38 schema-sync `pg_indexes` ↔ `pg_constraint` 混淆 bug）。
- `mail_provider` 单 active 不变式靠应用层 TX 维护（`POST /activate` 先把其他行置 false 再开自身），不引 partial unique index（同样触发 schema-sync bug；Postgres READ COMMITTED + 行锁串行化并发 activate 已足够）。
- 失败也写 `mail_log status=Failed` 留 audit trail；ConsoleMailer fallback 写 `provider_id=NULL`。
- 加密：`SWARMHIVE_SECRET_KEY`（base64-32B，env 优先；缺则读 `[secret] key` of `config/local.toml`，gitignored）→ AES-256-GCM 通过 `crypto::SecretKey::encrypt/decrypt`；密文格式 `base64(nonce(12) || ct || tag(16))`。同一把 key 后续给 OAuth `client_secret` 复用。
- `Mailer::send` 失败不抛到 axum handler，由调用方（future invite / reset 流程）按需 retry；`/test` 自检构建临时 SmtpMailer 直接给当前登录用户发，不污染 active 槽。

**不要做**：

- 不要绑死某家 HTTP API provider（违反 self-hosted 主旨）。
- 不要在编译期把模板烤进 binary（部署者不能改）。
- 不要往 GET response 回写 `password_encrypted`；只返 `password_set: bool`。
- 不要 raw SQL 创建 UNIQUE INDEX / partial INDEX —— sea-orm 2.0-rc.38 `schema-sync` 每次启动尝试 DROP CONSTRAINT 会因 `pg_indexes` 与 `pg_constraint` 不同源而失败。

**相关文件**：

- `crates/swarmhive-server/src/mail/{mod,smtp,console,template,seed}.rs`
- `crates/swarmhive-server/src/crypto.rs`
- `crates/swarmhive-server/src/routes/mail.rs`
- `crates/swarmhive-entity/src/{mail_provider,mail_template,mail_log}.rs`
- `docs/08-admin-and-analytics.md` "Mail Provider" / "Mail Templates" / "Mail Log" 段

## OAuth provider

GitHub OAuth 走 `oauth2` crate + 自定义 `IdentityProvider` trait。未来 Google / GitLab / 内部 OIDC 只需加 provider 适配器。

**正确做法**：
- `IdentityProvider` trait 抽象 `authorize_url` / `exchange`
- 邮箱冲突（GitHub email 已被 password 用户占用）→ 409 + 引导先用密码登录后绑定，**不**自动合并账号
- `User` + `IdentityLink (provider, subject, user_id)` 拆分模型

**相关文件**：`crates/swarmhive-server/src/auth/`（待 `add-oauth-github` 填充）、`docs/13-rbac.md` "Identity Providers" 段。
