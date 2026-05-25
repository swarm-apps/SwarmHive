# Backend

## 概览

`swarmhive-server` lib 里的业务约定：sea-orm entity 写法、auth 鉴权链路、storage trait、mailer、错误响应、RBAC permission 校验。写 `crates/swarmhive-server/src/{auth,services,storage,mail,routes}/` 或 `crates/swarmhive-entity/src/` 时读这里。

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
- token 字符串格式 `swhv_<base64url-32bytes>`（前缀便于泄露日志快速 grep）
- DB 只存 token 的 blake3 hash，明文仅在创建时返回一次

**不要做**：
- 不要引入 JWT（撤销难、scope 重发复杂、单 binary 无 stateless 收益）
- 不要把 PAT 和 API Token 当两件事——共用同一张表与同一份鉴权基建，只是 scope 默认值不同

**相关文件**：`docs/13-rbac.md`、`crates/swarmhive-server/src/auth/`（待 `add-auth-and-rbac` 填充）。

### Permission middleware

权限粒度是 verb-scoped（`release:publish`、`storage:manage` 等），不是行级。`RestrictedConnection` 不引入。

**正确做法**：
- handler 用 `require_permission!(principal, "release:publish", Scope::App(app_id))?;` 风格
- 失败返回 RFC 9457 `403 forbidden`（含 `required_permission` 字段）
- 敏感操作必须写 `audit_log` 行（actor_type、actor_id、action、resource_*、ip、user_agent）

**敏感操作清单**（必写 audit）：
登录成功 / 失败、创建/删除用户、修改角色、创建/撤销 token、修改 storage 配置、发布 release、promote / rollback / yank、修改强制更新策略。

**相关文件**：`docs/13-rbac.md` "敏感操作" / "审计日志" 段。

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

### lettre + minijinja + DB-backed templates

SMTP provider 配置和邮件模板**存 DB**，Admin 后台可编辑（与 Storage 同形态）。dev 用 mailpit。

**正确做法**：
- `swarmhive-server::mail::Mailer` trait 抽象，`SmtpMailer` + `ConsoleMailer`（dev fallback）两种实现
- minijinja 运行时从 DB 读模板字符串（不是嵌入 binary）
- 首启 seed 默认 template（`password_reset` / `user_invite` / `email_verify` / `security_alert`），支持恢复默认
- 模板用 `{{ variable }}` Jinja2 语法，按 event 类型有明确 context schema

**不要做**：
- 不要绑死某家 HTTP API provider（违反 self-hosted 主旨）
- 不要在编译期把模板烤进 binary（部署者不能改）

**相关文件**：`crates/swarmhive-server/src/mail/mod.rs`（待 `add-mail-infrastructure` 填充）、`docs/08-admin-and-analytics.md` "Mail Provider" / "Mail Templates" 段。

## OAuth provider

GitHub OAuth 走 `oauth2` crate + 自定义 `IdentityProvider` trait。未来 Google / GitLab / 内部 OIDC 只需加 provider 适配器。

**正确做法**：
- `IdentityProvider` trait 抽象 `authorize_url` / `exchange`
- 邮箱冲突（GitHub email 已被 password 用户占用）→ 409 + 引导先用密码登录后绑定，**不**自动合并账号
- `User` + `IdentityLink (provider, subject, user_id)` 拆分模型

**相关文件**：`crates/swarmhive-server/src/auth/`（待 `add-oauth-github` 填充）、`docs/13-rbac.md` "Identity Providers" 段。
