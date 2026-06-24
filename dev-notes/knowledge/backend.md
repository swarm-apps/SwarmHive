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

### LOC 阈值的真正信号

250 LOC 是**参考阈值**而非 fail gate。`add-invite-and-password-reset` apply 后实测：5/8 个 route 文件超 250（mail 702 / invite 532 / password_reset 398 / auth 338 / verify_email 299）——但每个文件**内聚性都仍是单一 feature**（mail 12 endpoints / invite 4 endpoints / password_reset 3 endpoints），没有拆 service 的真实收益。

**真信号**：

- ✅ **拆 service** —— 文件里出现"业务流 A 的 3 个 handler + 业务流 B 的 3 个 handler"两类不相关的处理（这是 vertical-slice 失守）
- ✅ **拆 service** —— 同一段业务逻辑被 2+ handler 复用且复用逻辑超过 10 行（如 `mail::refresh_mailer` 被 activate / delete 两处共享）
- ✅ **抽到 services/** —— 同一函数被 ≥ 2 个 route **文件**复用（参考 `services/account_token.rs` 由 invite / password_reset / verify_email 三个 route 文件共享）
- ❌ **不拆** —— 单文件 600+ LOC 但 80% 是 doc comments + utoipa::path 长 attributes + 单一 feature 的同形 handler（属于 verbose-but-cohesive，强拆只增加 import 跳转）

`mail.rs` 当前 702 LOC 是边缘案例 —— 应该在下次有改动时趁机做"轻量提 service"，但不为重构而重构。

### 共享 helper 落点速查（2026-06 收敛后）

写 handler 前先看有没有现成的横切 helper，别再内联复制（这批是一次审查驱动重构的产物，散落复制都已收敛到单一来源）：

| 要做的事 | 用这个 | 落点 |
|---|---|---|
| 构造 `extra` 为空的 typed problem | `ApiError::typed(status, type_uri, title, detail)` | `error.rs`（带业务字段才直接构造 `ApiError::Typed{..extra}`） |
| 已认证后建立 session（cycle_id+insert+expiry） | `service::establish_session(&session, user_id)` | `auth/service.rs`（login/setup/invite/password_reset/oauth 五处共用） |
| 弱口令 → 422 | `password::validate_strong_password(pw)?`（`From<WeakPasswordReason> for ApiError`） | `auth/password.rs` |
| 取一次性 token 的 user_id | `token_svc::require_user_id(&token_row)?` | `services/account_token.rs` |
| 发已渲染邮件 / 写 mail_log / 查 template_id | `mail::dispatch_email` / `mail::record_log` / `mail::lookup_template_id` | `mail/mod.rs`（**不在** `services/account_token.rs`，2026-06 从那挪出） |
| Draft→Published 状态翻转（幂等） | `releases::mark_published(txn, rel)` | `routes/releases.rs`（publish_release + uploads::complete 共用） |
| 拼下载入口 URL | `download::download_url(base, slug, version, id)` | `routes/download.rs` |

CLI 侧同理：`commands/project.rs` 收敛了 publish/verify 共用的 `absolutize`/`config_server`/`project_dir`/`resolve_slug`/`resolve_artifacts`/`resolve_tauri_conf` + `parse_enum`；`client::detail_of`（`pub(crate)`）是 RFC 9457 detail 提取的唯一来源（login/logout 复用）。

### routes/ 顶层组织演化指南

`routes/` 当前 10 文件平铺（业界 launchbadge/realworld 9 文件、atuin-server handlers/ 11 文件都是这个量级）。**未来阈值**：

| 文件数 | 组织方式 | 理由 |
| --- | --- | --- |
| ≤ **15** | **平铺**（当前形态） | IDE Cmd+P + 字典序排列足以管理；分目录反而多一层跳转 |
| 16-25 | **按业务域子目录**（`routes/accounts/{auth,setup,invite,password_reset,verify_email,users}.rs` 等） | 文件数突破 IDE 一屏可见时按 domain 分组提升 navigability；每个子目录用 sibling rust 2018 模块（**不要 mod.rs**），由 `routes/mod.rs` 集中 `pub mod accounts;` 重新导出 |
| > 25 | **micro-crate** 切（如 `swarmhive-server-accounts` / `swarmhive-server-releases`） | 单 server crate 已达管理上限；超出 self-host MVP 范围，等真到了再设计 |

**预定的业务域分组**（达到 16+ 文件时启用，仅供未来 reference）：

- `accounts/` —— auth / setup / invite / password_reset / verify_email / users / oauth_provider
- `releases/` —— apps / releases / artifacts / promote / rollback
- `storage/` —— config / presign / wizard
- `analytics/` —— telemetry ingest + admin queries
- `infra/` —— mail / notifications / system / audit

但不要现在就建空目录占位 —— 等到第 15 个文件出现时再做一次性重构，新加的 feature 直接落到对应子目录。

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

### schema 走 schema-sync,data migration 走 swarmhive-migration(2026-06-10 修订)

原决策是"schema-sync only,不引入 sea-orm-migration"。⑤ 的 `Invited`→`Provisioned` 改名带来第一个**数据**迁移需求,曾临时放 `db.rs::migrate_data`(raw SQL,藏在 `sync_schema` 内)——用户 review 指出不规范并要求建专门 crate;调研还发现**真 bug**:bin 里 `sync_schema` 被 `auto_sync` gate 包着,生产(auto_sync=false)根本不跑数据迁移 → 存量行 enum 反序列化启动崩。

**现行分工**:

- **schema**(建表/改列):dev/CI 需要时显式设 `auto_sync=true`,由 `get_schema_registry(REGISTRY_GLOB).sync()` 自动同步;生产默认 `auto_sync=false`,由 deployer 控制(人工 SQL / sea-orm-cli)。这半边不变。
- **data migration**(存量数据改写):`swarmhive-migration` crate(`sea-orm-migration =2.0.0-rc.38`,版本与 sea-orm **精确同 rc 序号**;`default-features=false` 去掉 cli/clap)。`Migrator::up()` 经 `db::run_migrations` 在 **server 每次启动无条件执行**——`seaql_migrations` 表记账,每条全局只跑一次、留历史、可 `down`。

**正确做法**:
- `db::sync_schema` = sync + `run_migrations`(dev/测试单入口,顺序硬约束:migration 先于任何受影响 entity 的 SELECT);bin 的 `auto_sync=false` 分支单独调 `db::run_migrations`(生产不能漏)。
- `config/default.toml` 的 `database.auto_sync` 默认 **false**(2026-06-24 线上事故后修正):生产启动不能跑 schema-sync,否则 sea-orm rc.38 会把 raw unique index(`uq_artifact_release_variant`)当成待删除 constraint,报 `constraint ... does not exist`;本地开发 / CI drift gate 要建表时显式设置 `SWARMHIVE_DATABASE__AUTO_SYNC=true`。
- migration crate **不依赖 entity**(实体漂移;见 architecture.md 5-crate 拓扑),数据改写用 raw SQL,且用 `DO $$ ... IF to_regclass('"user"') IS NOT NULL ...` 容忍表尚不存在(全新生产库 deployer 未建 schema 时记账跳过)。
- migration 文件命名 `mYYYYMMDD_NNNNNN_描述`;回归测试范式见 `db_smoke::invited_rows_are_migrated_once`(raw SQL 插旧值 → 首次 up 改写 → 二次 up no-op)。

**不要做**:
- 不要把数据迁移塞进 `sync_schema` 私有函数或任何被 `auto_sync` gate 的路径(就是这次的 bug)。
- 不要在 migration 里 `use swarmhive_entity::*`。

**相关文件**：`crates/swarmhive-migration/src/`、`crates/swarmhive-server/src/{db.rs,bin/server.rs}`、`crates/swarmhive-server/tests/db_smoke.rs`。

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

#### ⚠️ `DeriveActiveEnum` + `Serialize/Deserialize` 同存时 wire 格式会分叉

枚举同时派生 `DeriveActiveEnum`（DB 落库）和 `serde::{Serialize, Deserialize}`（HTTP wire，因为该 enum 直接当 DTO 字段）时，两套表示**互相独立**：

- `#[sea_orm(string_value = "starttls")]` 只管 DB 列值
- serde 默认用 **Rust variant 名**（PascalCase，如 `StartTls`），**不读** `string_value`

后果：DB 存 `starttls`、OpenAPI example 写 `starttls`、前端 select 发 `starttls`，但 serde Deserialize 只认 `StartTls` → `POST` 必 422（`unknown variant 'starttls', expected 'StartTls'...`）。`mail_provider::{SmtpEncryption, ProviderKind}` 早期就踩了这个，整个建 provider 流程对外不可用。

**正确做法**：凡是「既落库又上 wire」的 enum，显式加 `#[serde(rename_all = "lowercase")]` 让 serde 与 `string_value` 对齐。

```rust
#[derive(Clone, Copy, ..., DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]   // ← 对齐 string_value，否则 PascalCase 422
pub enum SmtpEncryption {
    #[sea_orm(string_value = "starttls")] StartTls,  // serde 也 → "starttls"
    // ...
}
```

**不要做**：

- 不要用 `rename_all = "snake_case"`：它把 `StartTls` 转成 `start_tls`（带下划线），与无下划线的 `string_value = "starttls"` 仍然不匹配。多词 variant 必须用 `lowercase`（或逐个 `#[serde(rename = "...")]`）。
- 不要只在 DTO 上 `#[schema(value_type = String, example = "starttls")]` 就以为修好了——那只改 OpenAPI 文档的展示，serde 反序列化逻辑没变，schema 反而误导前端发小写。
- 回归保护：wire 值用 `serde_json` round-trip 单测锁死（见 `mail_provider::tests`）。

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
- `last_used_at` 1-min 节流靠单条 UPDATE 的 WHERE 守卫 `id=X AND (last_used_at IS NULL OR last_used_at < cutoff)`，单库 round-trip、无 race、无应用层缓存。**用 sea-orm 结构化 API 表达，不写 raw SQL**：`api_token::Entity::update_many().col_expr(Column::LastUsedAt, Expr::value(Some(now))).filter(Column::Id.eq(id)).filter(Condition::any().add(Column::LastUsedAt.is_null()).add(Column::LastUsedAt.lt(cutoff)))`，读 `result.rows_affected`。cutoff = `Utc::now() - Duration::minutes(1)`（app 时钟，与 `verify_email`/`device` 一致；与原 DB `NOW()` 的微小时钟差对 1-min 节流无影响）。**早期用过 `Statement::from_sql_and_values` + `execute_raw` 的 raw SQL，2026-06-01 已重构成上面的 sea-orm 写法——server crate 现已无任何手写业务 SQL**（`db.rs` 的 `execute_unprepared("SELECT 1")` 只是健康检查 ping，不算）
- `auth::service::verify_password` 供 `/auth/login` 复用，DUMMY_PHC 等时（CLI 登录不再验密码，走 RFC 8628 device flow，见下）

**不要做**：
- 不要把 cookie 路径放在 Bearer 之前——显式 header 必须胜出
- 不要给 `last_used_at` 节流加 in-memory cache：多实例不一致，重启丢
- 不要在 `bearer::resolve()` 里写完整 audit（first-use 一次就够），高 QPS 下会撑爆 audit 表

### Token CRUD endpoints

- `POST /api/v1/tokens` 需 `token:manage`；PAT (`permissions IS NULL`) 与 API (`permissions = Some(subset)`) 强制规则在 `services::token::validate_permissions`
- `GET /api/v1/tokens?owner=...` 列他人需 `token:manage`；自己列自己无需特殊权限
- `DELETE /api/v1/tokens/:id` 业主或 `token:manage`；幂等（重复撤销返 Ok 不报错）
- CLI 登录不走 token CRUD，走独立的 RFC 8628 device flow（见下「Device flow」）；旧的 `POST /api/v1/auth/cli-token`（ROPC 密码授权）已被 `add-cli-device-login` 移除

**相关文件**：`crates/swarmhive-server/src/routes/tokens.rs`、`crates/swarmhive-server/src/routes/device.rs`。

### Device flow（RFC 8628，`add-cli-device-login`）

`swarmhive login` 用 OAuth 2.0 Device Authorization Grant 替换 ROPC：CLI 不经手密码，认证委托给 Web `/login`，故 OAuth-only 用户也能用 CLI。5 个 endpoint 在单文件 `routes/device.rs`（vertical-slice，≤250 LOC）：`POST /device/code`（公开，发 device_code+user_code）、`POST /device/token`（公开，轮询）、`GET /device/lookup` + `POST /device/{approve,deny}`（**Session-only**）。

**正确做法**：

- **token 端点破例走 RFC 8628 wire 格式** `400 { "error": <code> }`（`authorization_pending`/`slow_down`/`access_denied`/`expired_token`/`invalid_grant`/`unsupported_grant_type`/`invalid_request`），**不**走仓库通用 RFC 9457 problem+json——让标准 device-flow 客户端可互操作。infra 故障（DB）仍走 `ApiError` 的 500 problem+json。handler 返回 `Result<Response, ApiError>`：协议结果用 `device_err()` 构造 `(StatusCode::BAD_REQUEST, Json(DeviceTokenErrorResponse))`，DB 错 `?` 传播。
- **not-found 必须先于所有 status 分支**返 `invalid_grant`（含已被 lazy 清理的过期行）；`completed` 不能兜底 not-found。
- **approved→completed 原子 claim**：铸 PAT 前条件 `update_many().filter(Status.eq(Approved)).exec()`，`rows_affected==1` 才铸（`token_service::create` 非幂等），否则 `invalid_grant`。防并发轮询双铸。铸造复刻「临时 Principal + `token_service::create`」套路（`load_user_permissions` + `AuthMethod::Session{nil}` + `kind=Pat`，PAT 继承 owner 实时权限）。
- **slow_down 状态机**：首次轮询 `last_polled_at=null` → 不限速、按 status 返回、写 `last_polled_at=now`；之后 `now-last_polled_at<interval` → `slow_down` 且**不**刷新 `last_polled_at`（防边界抖动活锁）。slow_down 闸门在 status 匹配之前，故 approved 被快速轮询也会先返 slow_down（合规客户端退避后重试即铸，无害）。
- **approve/deny/lookup 仅接受 Session**：`matches!(principal.auth_method, AuthMethod::Session{..})`，否则 403 Typed（`extractor` 里 Bearer 优先于 session，不挡就会让 PAT 持有者脱离浏览器自批 → 钓鱼缓解失效）。注：`load_principal` 已拒非 Active 用户，故 pending_approval（⑤）天然被挡。
- **user_code**：8×base20 辅音字母表 `BCDFGHJKLMNPQRSTVWXZ`，`WDJB-MJHT` 格式；活跃集合内唯一靠生成时校验 + 重试（**不**装唯一索引，rc.38 schema-sync bug）。`device_code` 32B base64url + blake3 hex 落库（同 PAT/account_token）。
- **bootstrap 排除**：user 表空 → `/device/code` 返 410 typed `device-not-available-during-bootstrap`（对称 OAuth 排除）。
- **lazy 清理带 1h grace**：`/device/code` 入口 `DELETE WHERE expires_at < now() - INTERVAL '1 hour'`，保住刚过期行返 `expired_token`（而非误删成 not-found→invalid_grant）。
- **挂载**：整个 `routes::device::router()` merge 进 `build_router` 与 `openapi_router` 两处的 `sensitive` 子路由（governor 非全局，只 `.layer()` 在 sensitive；轮询主限速靠 slow_down）。
- **verification_uri** 复用 `ServerConfig.base_url`（同 invite/reset 链接）；dev 须配 `base_url=http://localhost:5173`（SPA origin）。

**不要做**：

- 不要让 token 端点走 problem+json（破坏 RFC 8628 客户端互操作）。
- 不要无 grace 地 `DELETE WHERE expires_at<now()`（过期码会立刻返 invalid_grant 而非 expired_token）。
- 不要给 approve/deny 用裸 `Principal` 不校验 auth_method（Bearer PAT 会自批）。

**相关文件**：`crates/swarmhive-server/src/routes/device.rs`、`crates/swarmhive-entity/src/device_authorization.rs`、`crates/swarmhive-api-types/src/device.rs`、`crates/swarmhive-cli/src/commands/login.rs`、`apps/admin/src/routes/device.tsx`、`crates/swarmhive-server/tests/device_login_smoke.rs`。

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
- 不要把账号锁逻辑塞进 `verify_password`——保持 verify 只做"密码匹配"，锁逻辑在 handler，便于其他不验密码的登录路径（如 device flow）不受影响
- 不要在 `/login` 的密码强度上做严格校验（强校验只在 set / change / reset 路径；登录强校验会锁死老账号）

**相关文件**：`crates/swarmhive-server/src/auth/bootstrap.rs`、`crates/swarmhive-server/src/auth/password.rs::validate_strong_password`、`crates/swarmhive-server/src/routes/auth.rs` (LOGIN_LOCKOUT_THRESHOLD + check_account_lock / record_failed_attempt / clear_login_attempts)、`crates/swarmhive-entity/src/user_login_attempts.rs`、`crates/swarmhive-server/src/error.rs::Typed`、`crates/swarmhive-server/assets/weak-passwords-top100.txt`。

### account_token 一次性 token（`add-invite-and-password-reset`）

invite / password_reset / email_verify 三类一次性 token 共用单表 `account_token`，`purpose` enum 区分。逻辑全部集中在 `services/account_token.rs`（`AccountTokenService`），三个 route 文件（invite / password_reset / verify_email）只组合它 + 各自业务不变式。

**双层 hash（核心设计）**：

- plaintext = `base64url(rand 32B)`（43 字符，无 padding），只出现在邮件里
- DB 存两列：`token_hash = argon2(plaintext)`（DB dump 也无法还原 plaintext）+ `token_lookup = base64(blake3(plaintext)[..16])`
- 校验路径：先按 `(purpose, token_lookup)` 索引命中候选行（O(1)），**再**对该行做一次 argon2 verify。没有 lookup 列就得对全表逐行 argon2（极慢）；lookup 是为了避免「argon2 不可索引」与「明文不可落库」的冲突
- 用 blake3 而非 sha256：性能更好、抗碰撞同级、依赖更轻（已在 workspace）

**单不变式：每个 (user_id, purpose) 至多一个未消费 token**：

- 由 `issue_replacing()` 在**事务内** invalidate 旧 active + insert 新行保证，**不装 partial unique index**（sea-orm 2 rc.38 schema-sync 处理 `WHERE consumed_at IS NULL` 有 bug，与 `mail_provider` 同样的 workaround）
- resend / forgot 重复请求天然轮换 token，旧链接立即失效

**TokenError → ApiError 集中映射**：`verify()` 返回的 `TokenError`（NotFound / Expired / AlreadyConsumed）在 service 内统一 `From` 成 `ApiError`（404 / 410 / 410 Typed），handler 直接 `?` 传播，不在每个调用点重复 match。

**dispatch_email(state, to, event_name, context)**：薄封装 `mailer.send`，caller 传完整 context（`invite_url` / `reset_url` / `verify_url` 等），不做魔法前缀注入——key 名与模板占位符一一对应。URL 由 `build_url(base_url, page_path, plaintext)` 拼成 `{base}{path}?token={plaintext}`，三个 SPA 公开页共用 `?token=` 解析。

**不要做**：

- 不要把 plaintext 落任何持久层（含 mail_log body）——E2E 测试要拿 token 只能从注入的 capturing mailer 的 envelope context 里取（见 `tests/account_token_smoke.rs`）
- 不要给 token 表加 `WHERE` 条件的 partial unique index（schema-sync 会漏建或建错）

**相关文件**：`crates/swarmhive-server/src/services/account_token.rs`、`crates/swarmhive-entity/src/account_token.rs`、`crates/swarmhive-server/src/routes/{invite,password_reset,verify_email}.rs`、`crates/swarmhive-server/tests/account_token_smoke.rs`。

## 发布列车 / channel 指针模型（`add-app-release-artifact`）

App / Channel / Release / Artifact 业务实体 + 发布生命周期。核心是 **release 与 channel 解耦**：release 是 channel-independent 的版本化产物集合，channel 只是「当前指向哪个 release」的命名指针。配置档 `openspec/project.md` 锚定的约定就是「回滚不删历史，仅改 channel 指向」。

**实体关系**：

- `release`：`(app_id, version)` 唯一，channel 无关；`status` draft/published/yanked；`android_version_code: Option<i64>`（RN 单调比较，Tauri null）。
- `channel_release`：`channel_id` 为 PK（每 channel 至多 1 行）→ `release_id`，即「该 channel 当前服务的 release」。无行 = 该 channel 从未 promote 过。
- `channel_release_history`：append-only，每次 promote/rollback 一行（action enum + actor + reason）。**永不删 release**。
- `artifact`：本 proposal 内**只读**（creation 在 `add-storage-and-presign-upload` 的 complete 回调）；`storage_backend_id` 先是裸 `Uuid` 列，FK 关系等存储 proposal 补。

**正确做法**：

- promote / rollback 走 `routes/releases.rs::apply_pointer_move`（TX 内 upsert `channel_release` 指针 + append history），audit 在 commit 后 `write_swallowing`。同一 release 可被多 channel 同时指向，promote 不复制产物。
- rollback 无显式 `version` 时取 `channel_release_history` 中当前指向之前最近一条 distinct release；无历史 → `ApiError::Typed` `nothing-to-rollback`（422）。
- `POST /apps` 在一个 TX 内建 app + seed dev/beta/stable（stable default）。`app.slug` 不可变；`DELETE /apps/:slug` 有 release 时返 `app-has-releases`（409，Typed）。
- channel 操作**无独立权限**，复用 `app:update`（`PermissionName` 无 `channel:*`）。角色矩阵有意分散：developer 能 `release:create`（建 draft）但无 `release:publish`；release-manager 能 publish/promote 但无 `release:create`/`app:create` —— `swarmhive publish` 全流程权限需 owner/admin 或 scoped CI token。
- `release` 表名是 SQL 关键字，sea-orm 默认引用标识符，Postgres 下 `"release"` 合法，无需改名。
- `app.platforms` 是 `Json`（`Vec<api::Platform>` 序列化），`From<&Model>` 用 `serde_json::from_value(...).unwrap_or_default()`。
- 对象路径**去 channel、版本寻址**（`apps/{slug}/versions/{version}/{platform}/{target}/{filename}`）—— 与指针模型一致，promote 零存储动作。详见 `dev-notes/explore-summaries/2026-05-28-upload-and-cli-stack.md` + `memory/project-storage-model.md`。

**不要做**：

- 不要把 channel 嵌进 release 或对象路径（破坏「同一 release 跨 channel promote 不重传」）。
- 不要在 promote/rollback 删除 release（只移指针 + 写历史）。
- **全非空**列的复合唯一约束（`(org_id,slug)` / `(app_id,version)` / `(app_id,name)`）用 sea-orm 2 `#[sea_orm(unique_key="...")]` 同标签字段对，**不**用 raw `CREATE UNIQUE INDEX`（rc.38 schema-sync bug，见 mail/account_token 同款）。
- ⚠️ **含可空列的复合唯一约束是例外**（`harden-publish-flow` 2026-06-24 推翻旧做法）：artifact 五元组 `(release_id, platform, target, arch, abi)` 里 `target`/`arch`/`abi` 可空，`#[sea_orm(unique_key]` 建出的是普通 NULL-distinct 索引——**对 NULL 行无效**（Postgres 默认 NULL≠NULL，两条 arch/abi=NULL 的同 target 行被当不同行，既挡不住重复也让 `ON CONFLICT (5 列)` 推断不命中）。正解见下「artifact 唯一索引：NULLS NOT DISTINCT + migration」。**判据**：复合唯一键里有可空列 → 走 migration raw SQL `NULLS NOT DISTINCT`，去掉 entity 的 `unique_key`；全非空才用 `unique_key`。

**相关文件**：`crates/swarmhive-entity/src/{app,channel,release,artifact,channel_release,channel_release_history}.rs`、`crates/swarmhive-server/src/routes/{apps,releases}.rs`、`crates/swarmhive-server/tests/app_release_smoke.rs`。

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

### S3 trait + presign + complete（`add-storage-and-presign-upload`）

详见 [architecture.md](architecture.md) "存储抽象" 段。SwarmHive 唯一 storage 后端是 S3-compatible（`aws-sdk-s3`），`storage/mod.rs` 定义 `Storage` trait，`storage/s3.rs` 是唯一实现 `S3Storage`。

**完整性双层：Content-MD5 通用闸门 + sha256 机会主义叠加**：

- **Content-MD5 永远绑定**：presign 用 `put_object().content_md5(hex_to_b64(md5))` 把标准 `Content-MD5` 头绑进签名。这是**所有 S3 兼容存储（含阿里云 OSS）都认的唯一通用完整性闸门**——存储侧收字节自算 MD5，不符直接 4xx 拒，server 全程不碰字节。客户端必须先算 md5 填 `PresignFile.expected_md5`（hex on wire，到 SDK 边界 `hex_to_b64`）。
- **`x-amz-checksum-sha256` 机会主义叠加**：仅当 `backend.supports_sha256_checksum`（`/test` probe 探测）时 presign **额外**绑 sha256（AWS / MinIO / RustFS 更强）。OSS 这类不支持的后端只走 Content-MD5。两者都在 `PresignedPut.headers` 里，客户端**原样回放**。
- **complete 校验分支**（`HeadObject`，不二次下载）：先比 size；再看 `head()` 回传的 `checksum_sha256()`——**有**（AWS/MinIO）就比 sha256；**没有**（OSS 不回传 sha256）则用单段 PUT 的 **ETag = hex MD5** 与 planned md5 比对（`etag_as_md5` 归一化引号+大小写；multipart/SSE 的非 MD5 ETag 跳过，靠写时 Content-MD5 兜底）。任一不符 → `422 upload-checksum-mismatch` + audit + 不写 artifact。`ObjectMeta` 因此带 `etag` 字段。
- **为什么用 MD5 而非 sha256 当通用闸门**：AWS additional checksum（`x-amz-checksum-sha256`）OSS **不支持**；OSS 原生完整性只有 Content-MD5 + CRC64（`x-oss-hash-crc64ecma`）。CRC64 是 OSS 专有头、aws-sdk-s3 presign 绑不进签名、HeadObject 也读不回，且破坏"一套 S3 SDK 通吃"抽象，故弃用。MD5 弱碰撞不影响——它只防**传输损坏**；**防篡改**交给 DB 里的 sha256 + 未来 minisign。详见 `memory/` OSS checksum 讨论。
- **回归测试**：`storage_smoke::corrupt_upload_is_rejected_by_object_storage` 用 MinIO 实测"改字节→存储侧 4xx 拒"；`presign_and_put` 断言 Content-MD5 进了签名头（锁死"aws-sdk-s3 presign 确实签 Content-MD5"这个 load-bearing 假设）。
- **md5 计算**：CLI / 测试用 `md-5` crate（RustCrypto，与 sha2 同 `digest`）。注意 **`digest` 0.11 移除了 hasher 的 `std::io::Write` impl** → 不能再 `std::io::copy(file, hasher)`，改手动 `read` 分块喂 `Digest::update`（`client.rs::hash_file` 泛型 helper）。

**幂等 + 并发安全**（`harden-publish-flow` 2026-06-24 重写）：`upload_session.status` 标记；重复 complete 同 `upload_id` 若已 `completed` 直接返回当前 release 状态。artifact 写入是 **DB 层原子 `INSERT ... ON CONFLICT (5 列) DO UPDATE`**（`artifact::Entity::insert(model).on_conflict(OnConflict::columns([..]).update_columns([..])).exec_without_returning`），取代了原先的 SELECT-then-INSERT（`eq_opt` + 查在不在 → update/insert，多 target 并发下有写-写竞争、会静默丢 artifact，已删除）。两个 caveat：① `on_conflict + exec_*` 跳过 `before_save`，`created_at` 必须显式 `Set`；② `signature_metadata` 仅在本次带签名时才进 `update_columns`（否则无签名重传会把既有签名覆盖成 null）。冲突仲裁靠下面的 `NULLS NOT DISTINCT` 唯一索引。

**hot-swap backend**（复刻 mail `refresh_mailer`）：`AppState.storage: Arc<RwLock<Option<StorageHandle>>>`。`storage::refresh(&state)` 在 activate / patch active backend 后 + bin 启动时调（`server.rs` 的 `storage::refresh` 紧跟 `wire_active_mailer`）。无 active backend → 上传端点 `409 storage-not-configured`。单 active 不变式靠 activate 的 TX（先 `update_many` 置全 false 再置自身 true），**不**装 partial unique index（rc.38 schema-sync bug，与 mail/account_token 同款）。

**secret 加密**：`access_key_secret_encrypted` 复用 `crypto::SecretKey`（同 `SWARMHIVE_SECRET_KEY`）。`ApiError` 现实现 `From<CryptoError>`（映射到 `Internal`），storage handler 直接 `?` 传播 encrypt 错误（mail.rs 早期用本地 `crypto_to_api`，现可逐步收敛到这个 From）。GET 永不回密文，只返 `secret_set: bool`。

**发布与上传解耦 + 幂等 finalize**（`harden-publish-flow`，**取代**原 complete×publish 副作用）：发布不再是 complete 的副作用,收敛成独立端点 `POST /api/v1/apps/{slug}/releases/{version}/finalize`。

- **`releases::finalize_publish(txn, app_id, slug, release_id) -> FinalizeOutcome`** 是「发布」副作用的**唯一来源**：release 行 `lock_exclusive`(单次、release 级,**不是** per-target)→ 锁内幂等判定(Published 原样返回 `newly_published=false` / Yanked 拒 409 / Draft 继续)→ 校验 artifact ≥ 1(否则 422)→ `mark_published` → emit `ReleasePublished`。`finalize_release` handler 与过渡期的 `complete(publish=true)` 共用它。调用方负责事务边界 + 按 `newly_published` 决定提交后审计。
- **多 target 推荐流程**：N 个 target 各自 `complete`(默认上传到 draft,只需 `artifact:upload`)→ 末步一次 `finalize`(需 `release:publish`)。从「O(并发数) 抢发布」降为「N 次无副作用上传 + O(1) 幂等 finalize」。
- **complete(publish=true) 已 DEPRECATED**:仍接受(api-types 字段 doc + OpenAPI 描述 + `tracing::warn` 标注),内部委托给同一个 `finalize_publish`;artifact 先提交,故发布因权限/校验失败也不回滚已传产物。待下游(SwarmDrop/-RN)升级后移除。
- **悲观锁已移除**:原 `complete` 内对 release 行 `lock_exclusive` 把所有 artifact 写入串行化的临时补丁删掉了,由原子 upsert + 唯一索引(并发安全)+ finalize 的 release 级单次锁(防双发布)取代。
- **权限分散仍在**:`artifact:upload`(传产物)与 `release:publish`(finalize)是两个权限;`swarmhive publish` 全流程需 `release:create + artifact:upload + release:publish`(+ 重发改 notes 需 `release:update`,见 `harden-publish-flow` CI token 预设),单一内建角色都不全,是有意的职责分离。

**artifact 唯一索引:NULLS NOT DISTINCT + migration**（`harden-publish-flow`,**纠正**早期「artifact 五元组用 `unique_key`」的做法）：唯一约束 `(release_id, platform, target, arch, abi)` 由 `swarmhive-migration` 的 raw SQL 索引 `uq_artifact_release_variant`（`NULLS NOT DISTINCT`,PG15+）拥有,entity **去掉** `#[sea_orm(unique_key]`。

- **两个根因叠加**(见 migration 文件 doc):① 生产 `auto_sync=false` → schema-sync 整个不跑 → `unique_key` 建的索引在生产**从未创建**(`unique_key` 宏本身没 bug,只是被 sync 开关 gate 住);② `target`/`arch`/`abi` 可空,普通 NULL-distinct 索引对 NULL 行无效。migration 经 `run_migrations` **无条件执行**(不受 `auto_sync` 影响)同时解决两者。
- **为什么必须 NND**:sea-orm `OnConflict::columns([..])` 只按列推断冲突目标,要让含 NULL 的行也命中冲突 → 必须 PG15+ 的 `NULLS NOT DISTINCT`(COALESCE 表达式索引 sea-orm 列式 OnConflict 表达不了)。
- **migration 写法**:`to_regclass` 守卫 + `DROP INDEX IF EXISTS "idx-artifact-release_variant"`(schema-sync 旧索引,含连字符要双引号)+ `CREATE UNIQUE INDEX ... NULLS NOT DISTINCT`。去掉 entity 注解是必须的:否则 dev schema-sync 每次启动重建旧 NULL-distinct 索引 → 与 NND 索引并存 → `ON CONFLICT` 推断歧义。
- **testcontainers 必须 PG15+**:`Postgres::default()` 默认 tag `11-alpine` 不支持 NND,会让每个 boot server 的集成测试在 migration 处语法报错。全仓 18 个 test 文件统一 `Postgres::default().with_tag("17-alpine")` + `use testcontainers::ImageExt;`(dev DB 是 PG17,生产 ≥15)。
- **回归**:`storage_smoke::same_target_reupload_is_idempotent_upsert`(NULL 列重传仍 1 行)+ `concurrent_multi_target_complete_then_finalize_keeps_all_artifacts`(移除锁后 4 target 全留存)。

**下载分发**：`GET /download/:app/:version/:artifact_id` 公开，按 backend `url_mode` 生成 public（`public_base_url` 拼接）或 signed（presigned GET）URL → `302`，不代理字节；yanked release → 404；当前 download_intent 只记 structured log（`tracing::info!`），遥测 proposal 落地后改最小表。

**浏览器直传:CORS + 签名落库(`add-web-artifact-upload`)**:Web Admin 也能上传产物(浏览器复用 presign/complete 直传,见 [admin-spa.md](admin-spa.md))。后端两处增量:

- `Storage::put_cors(&[String])` + `S3Storage` 用 `aws-sdk-s3` 的 `put_bucket_cors` 写规则(`PUT/GET/HEAD` + `AllowedHeaders=["*"]` + `ExposeHeaders=["ETag"]`)。新端点 `POST /storage/backends/:id/cors`(`storage:manage`)按 id 查 backend → `from_backend` 建 client → `put_cors`;**失败不 5xx**,返回 `CorsConfigResult{ok:false, detail}`(OSS 的 S3 兼容层可能不支持 `PutBucketCors`,给手动指引)。
- `CompletePart` 加 `#[serde(default)] signature: Option<String>`(CLI 不传,向后兼容)。`upsert_artifact` 收到非空 signature 时写 `signature_metadata = {"tauri_signature": <sig>}`;为空则保持不动(insert 为 null,update 不覆盖既有签名)。Tauri `.sig` 文本由浏览器读出内联进 complete,不作为独立对象上传。
- 回归:`storage_smoke::{complete_persists_tauri_signature, configure_cors_is_permission_gated_and_typed}`(MinIO 的 PutBucketCors 行为不稳,CORS 测试只断言 200 + 结果形状 + RBAC,不赌 ok 值)。

**`.gitignore` 坑**：根级运行时数据目录用 `/data/`、`/storage/`、`/tmp/`（带前导 `/` 锚定到仓库根），否则裸 `storage/` 会误伤源码模块 `crates/swarmhive-server/src/storage/`。

**模块组织**(`uploads` 超 400 LOC 后的拆分,符合本文「拆分阈值」):被 `routes/uploads` 与 `routes/download` 复用的 `handle`/`active_backend`(取活跃 handle / backend 行,返回 409)提到横切层 `services/storage.rs`——避免 route 文件互相 import;`uploads` 的纯业务 helper(object_key / plan_part / verify_part / upsert_artifact / endpoints_for 等)下沉到 Rust 2018 sibling `routes/uploads/service.rs`,`uploads.rs` 只剩薄 handler。注意 `services/storage.rs` 与对象存储抽象 `crate::storage` 同名但不同层。

**相关文件**：`crates/swarmhive-server/src/storage/{mod,s3}.rs`、`crates/swarmhive-server/src/services/storage.rs`、`crates/swarmhive-server/src/routes/{storage,uploads,download}.rs` + `routes/uploads/service.rs`、`crates/swarmhive-server/tests/storage_smoke.rs`、`crates/swarmhive-entity/src/{storage_backend,upload_session}.rs`。

### CLI publish/verify 上传链路（`swarmhive-cli`）

CLI 不依赖 entity / sea-orm / aws-sdk（CI `cargo tree` 守护）；只用 `swarmhive-api-types` DTO + `reqwest` 直传。

- `publish <tauri|android>`：读 `swarmhive.toml`（`config.rs`，`--app` 覆盖；Tauri version 自动读 `tauri.conf.json`，Android `--version`/`--version-code` 显式）→ ensure draft release（`post_ensure` 容忍 409）→ presign → 每文件流式 PUT（`tokio_util::io::ReaderStream` + indicatif 进度；`backon` 指数退避只重试 5xx/timeout/connect，4xx 立即失败；单文件失败只重该文件，retry 内 `pb.set_position(0)` + 重开文件）→ complete（默认 publish=true）→ 可选 `--channel` promote → 打印 endpoints。
- `verify`：产物存在 + sha256 + Tauri 解析 `latest.json` + 查 server 重复版本（`--dry-run` 跳过）；Android 信任 `--version`/`--version-code`，**不**解析 APK AXML / build.gradle。
- 鉴权：`auth::resolve(config_server)` 统一 token（`SWARMHIVE_TOKEN` env > credentials.toml）+ server（`SWARMHIVE_SERVER` env > swarmhive.toml `server` > credentials.toml）；CI 走 env，官方 GitHub Action（独立仓库 `swarm-apps/swarmhive-action`，2026-06-09 从原 `.github/actions/publish` 抽出）注入 env 后 `npx @swarm-hive/cli`。
- 网络栈：reqwest `rustls-tls-native-roots`（尊重 OS 根证书）+ `--ca-cert`/`SWARMHIVE_CA_CERT` 加私有 CA。
- clap 坑：自定义 `--version` 字段与 `propagate_version` 自动 flag 冲突 → 在 Args 结构上 `#[command(disable_version_flag = true)]`。

**管理命令 + AI 友好契约(`add-cli-management-commands`)**:CLI 不止发布,还能管理 `apps {get,create,update,delete --yes}` / `channels {list,create,set-default,promote,rollback}`(收编原 top-level promote/rollback 桩)/ `releases {get,create,update,publish,yank --yes}` —— 与 Web Admin 同一批 endpoint,零后端 / api-types 改动。关键设计:

- **错误结构化**:`client.rs` 的 `ApiProblem`(`pub`,`thiserror`)在非 2xx 时由 `build_problem(status, body)` 解析 RFC 9457 problem+json(非 JSON body 合成 `{status,detail}`);`anyhow` 包裹后 `main::render_error` 用 `downcast_ref::<ApiProblem>()` 取出。
- **输出契约**(配套 skill / AI 依赖):成功 → `--output json` 打对象/数组到 **stdout**(`emit` / `emit_one` / `emit_ack`);失败 → problem+json(API)或 `{"error":...}`(本地)到 **stderr** + `process::exit(1)`。`main` 改成 `async fn main()`(非 `-> Result`),`dispatch()` 返回 `anyhow::Result`,顶层按 `--output` 渲染。
- **HTTP helper**:`client.rs` 补 `patch_json` / `delete_no_content` / `post_empty_json`(publish/yank 无 body)。
- **破坏性 `--yes`**:`apps delete` / `releases yank` 用 `anyhow::ensure!(yes, …)` 守门(非交互,配最小权限 token 双保险)。
- **边界**:CLI 仍只引 api-types(`cargo tree -p swarmhive-cli | grep sea-orm` 必须空);管理命令全走 HTTP。
- **测试**:纯逻辑单测(`build_problem`/`message`/`parse_platforms`,在 bin crate `#[cfg(test)]`);CLI-binary e2e 暂缺 harness(bin crate 无法被集成测试 import + CLI 走 reqwest 需真实 server),与 admin e2e 同样 deferred;endpoint 行为已由 `app_release_smoke`(in-process)覆盖。
- **storage / mail CLI 管理**走后续 `add-cli-storage-mail-admin`(mail DTO 需先提升到 api-types)。
- **release 灰度 / 强更 policy CLI parity(`add-cli-release-policy`)**:`releases update` 加 `--rollout-percent`/`--min-version`/`--android-min-version-code`、`create` 加 `--android-min-version-code`,直接填进既有 `Update/CreateReleaseRequest`(零 server / api-types 改)。**CLI 清空语义比 UI 简单**:flag 直接映射 `Option<field>`(省略=不改、传值=设),清空靠用户**显式传 sentinel**(`--rollout-percent 100` / `--min-version 0.0.0`),`#[arg]` help 注明——**不**复刻 admin 的 `policyUpdateFields` compare-to-initial(命令式接口无「初值」概念,显式即正确)。`ReleaseRow` 加 rollout/min ver 列(去 `0.0.0` sentinel 显示)。

**相关文件**:`crates/swarmhive-cli/src/commands/{apps,channels,releases,client}.rs`、`src/main.rs`(`dispatch` / `render_error`)。

**相关文件**：`crates/swarmhive-cli/src/config.rs`、`crates/swarmhive-cli/src/auth.rs`、`crates/swarmhive-cli/src/commands/{client,publish,verify,storage}.rs`、`dist-workspace.toml`、`.github/workflows/release.yml`、官方 Action 独立仓库 `swarm-apps/swarmhive-action`（原 `.github/actions/publish` 已迁出）。

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

**mail DTO 提升到 api-types(`add-cli-storage-mail-admin`)**:为让 CLI(不依赖 entity/sea-orm)管理 mail,把原内联在 `routes/mail.rs` 的 HTTP DTO 迁到 `api-types/src/mail.rs`(`MailProviderView` / `CreateProviderReq` / `UpdateProviderReq` / `MailTemplateView` / `UpdateTemplateReq` / `PreviewReq` / `PreviewResp` / `MailLogView` / `MailStatusResp` / `TouchedResp` / `TestSentResp` + 枚举 `ProviderKind` / `SmtpEncryption` / `MailLogStatus`)。

- **转换归属 entity**:`mail_provider`/`mail_template`/`mail_log` 各 `impl From<&Model> for api::*View`;枚举两个方向都要(`From<entity enum> for api enum` 给 View 响应,`From<api::SmtpEncryption> for entity` 给 create/update 请求写 ActiveModel)。`routes/mail.rs` 只留 server 本地的 `LogsQuery`,handler 用 `api::*`,`encryption` 写库处 `.into()`。
- **枚举统一 lowercase**:三个枚举都 `#[serde(rename_all = "lowercase")]`。`MailLogStatus` 历史上漏了 rename(wire 曾是 PascalCase `Sent`/`Failed`),本次**统一成 `sent`/`failed`**(用户拍板:优先一致性,接受这点破坏性 wire 变更)。entity `mail_log::MailLogStatus` 也补了 rename。
- **schema 取舍 A(精确枚举)**:DTO 字段直接引用枚举(不再 `#[schema(value_type=String)]`),OpenAPI 因此呈现字面量枚举(`SmtpEncryption: "starttls"|"tls"|"none"` 等),`schema.gen.ts` 随之收紧、admin 类型更紧(typecheck 仍过)。`MailStatusResp.transport` 从 `&'static str` 改 `String`(api-types 不能放 `&'static str` 共享),server 构造处 `.to_string()`。
- **`storage` DTO 无需迁移**(早在 api-types),故 storage CLI 零后端改动;只有 mail 这条线动后端。

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

## OAuth provider（`add-oauth-github-and-provider-config`）

GitHub OAuth 登录 + 绑定/解绑 + admin 后台 provider 运行时配置（不是 config.toml 重启，同 mail provider 形态）。`oauth2 5.x` crate + 自定义 `IdentityProvider` trait（`auth/oauth/{mod,github}.rs`）。未来 Google / GitLab / OIDC 只加 provider 适配器 + `OAuthProviderKind` variant。

**正确做法**：

- **`IdentityProvider` trait**：`authorize(redirect_uri) -> AuthorizeRequest{url, state, pkce_verifier}` + `async exchange(code, pkce_verifier, redirect_uri) -> ExternalIdentity`。`ExternalIdentity{subject, email, display_name, avatar_url, raw}` 解耦「外部身份」与「内部账号」（一个 user 可多个 identity_link）。`provider_factory(row, &SecretKey)` 从 DB row 构造（解密 client_secret）。
- **oauth2 5.0 typestate**：`BasicClient::new(ClientId).set_client_secret().set_auth_uri().set_token_uri().set_redirect_uri()` → `authorize_url(CsrfToken::new_random).add_scope().set_pkce_challenge().url()` → `exchange_code(code).set_pkce_verifier(PkceCodeVerifier::new(s)).request_async(&reqwest::Client).await`。client 内联构建（typestate 类型名太长不便抽 helper）。
- **依赖**：`oauth2 = { version="5", default-features=false, features=["reqwest","rustls-tls"] }`(rustls 对齐全仓)。oauth2 用 reqwest 0.12,与 server 直接依赖的 `reqwest.workspace`（带 json feature）**cargo 统一为同一 crate**——所以 `request_async(&reqwest::Client)` 与 `.json()` 都可用，且 GitHub `/user`+`/user/emails` 复用同一 client。token 交换 client 必须 `redirect(Policy::none())`(SSRF 防护,oauth2 强制)+ `user_agent`(GitHub API 必需)。
- **只信 verified email**：`/user/emails` 取 `primary && verified`，否则任一 `verified`，都没有 → `email=None` → callback 返 422 `oauth_no_verified_email`。**不**信 `/user` 的公开 email(可能未验证)。
- **flow 端点**(`routes/oauth.rs`,全在 sensitive 子树受 governor)：`GET /auth/oauth/providers`(公开,enabled-only,无 secret)· `GET /oauth/{kind}/start`(**bootstrap 410** + 用 tower-session 存 `{state,pkce_verifier,next,mode,kind,user_id?}` + open-redirect 防护 `safe_next`)· `GET /oauth/{kind}/callback`(state 校验 → exchange → 分支:已链接登录 / 邮箱冲突 **302 →`/login?oauth_conflict`**〔浏览器导航,不泄 email〕/ 无 verified email 422 / 未注册 401 `oauth_registration_disabled`〔⑤ 接管〕)· `GET /oauth/providers/link/{kind}/start`(authenticated,**GET 非 POST**——浏览器顶层导航才能跨域跳 GitHub;真正 link 发生在 callback)· `DELETE /oauth/links/{kind}`(无 user_credentials → 409 `cannot_unlink_only_auth_method`)· `GET /auth/me/identity-links`。callback link mode 复用同一 handler(`flow.mode` 分支)。redirect 用 `Redirect::to`(303 See Other,POST→GET 正确)。
- **provider CRUD**(`routes/oauth_providers.rs`,全 `auth:manage`)：list/create(GitHub URL/scope 空则预填默认)/update(空 secret=保留,同 mail/storage)/delete/test(仅 client_id+secret 非空 + authorize_url 可达探测,**不**真 token 交换)。
- **entity `oauth_provider`**：`kind` 用 `#[sea_orm(unique)]`(=`UNIQUE(kind)` 全唯一,一种 kind 一个 provider)——**不是** partial unique index(rc.38 schema-sync bug)。`client_secret_encrypted` AES-GCM(复用 `crypto::SecretKey`);`scopes` 存 `Json`(仿 app.platforms,`scopes_vec()` helper)。View 只回 `secret_set: bool`。
- **permission**：`PermissionName::AuthManage`(`auth:manage`)新增;seed owner via `all()`、admin 显式绑定(同 mail:manage)。idempotent seed,存量部署下次启动自动获得。

**不要做**：

- 不要给 oauth_provider 装 partial unique index;`#[sea_orm(unique)]` on kind 即可。
- 不要信 GitHub `/user.email`(未验证);只走 `/user/emails` 的 verified。
- 不要把 link_start 做成 POST(浏览器顶层导航跳不了跨域 GitHub)。
- callback 邮箱冲突不要返 problem+json(浏览器导航看不到);用 302 redirect 到 `/login?oauth_conflict=`。

**⚠️ utoipa operationId 全局唯一**：utoipa 用 handler **函数名**作 operationId,跨所有 route 模块必须唯一。oauth provider CRUD handler 一开始命名 `list_providers`/`create_provider`/... 与 mail 的同名 handler 撞 → `schema.gen.ts` 生成重复标识符 TS2300。改名 `list_oauth_providers`/`create_oauth_provider`/... 解决。**新 route 模块的 handler 名要避开既有模块同名**。

**⚠️ token 交换的 `502 "OAuth provider error: Request failed"` —— github.com 链路抖动 + oauth2 吞真因**(2026-06-10 生产实测):国内机房(阿里云深圳)直连 `github.com` 的 token 交换 POST 易被 reset / 超时,而 `测试` 按钮的 authorize GET(同主机、小响应)却可能照过——HTTPS 下 GFW 只看 SNI(authorize/token 同主机),**不可能**确定性地只挡其一,所以这是**链路抖动**不是封锁。两个根因叠加:

- **oauth2 5.0 `RequestTokenError::Request` 用 `#[source]` 把真正的 reqwest 传输错误藏起来,Display 只剩 `"Request failed"`**;`e.to_string()` 丢掉 source 链 → 502 detail 与日志都看不到根因。**修法**:`error_chain(err)` helper 走 `std::error::Error::source()` 把链拼成 `"Request failed: <reqwest>: connection reset"`(`github.rs`,reqwest 错误同理也漏底层 io/hyper 因,一并用它)。
- **client 无超时 + 无重试**:`GithubProvider::new` 的 reqwest client 现加 `connect_timeout(10s)+timeout(20s)`;token 交换对 `RequestTokenError::Request`、`/user`+`/user/emails` GET 对 `is_connect()||is_timeout()||is_request()` 各做 3 次、300/800ms 退避重试(`github_get` 复用)。**只重瞬时(没拿到响应)**,`ServerResponse`/`Parse`/状态码错误立即返回(链路是通的,重试无益)。

**运维侧**:reqwest 默认认 `HTTPS_PROXY`/`ALL_PROXY` env(两处 client 都没调 `.no_proxy()`),给国内 server 注代理即让 token 交换 + userinfo 一起走代理,是比重试更彻底的解。失败点都加了 `tracing::warn!(target:"swarmhive_server::oauth")`——`From<OAuthError> for ApiError` 的 BAD_GATEWAY 路径本身不 log,靠这两条 warn 兜底。

**不要做**:不要在 token 交换重试里把 `ServerResponse`(坏 code/secret)也重试——code 单次性,重试只会拿到 `bad_verification_code`;只重 `Request` 变体。

**相关文件**：`crates/swarmhive-server/src/auth/oauth/{mod,github}.rs`、`routes/{oauth,oauth_providers}.rs`、`crates/swarmhive-entity/src/oauth_provider.rs`、`crates/swarmhive-api-types/src/oauth.rs`、`crates/swarmhive-server/tests/oauth_smoke.rs`(wiremock GitHub)、`docs/13-rbac.md` "Identity Providers" 段。

## Registration Policy + 自助注册（`add-registration-policy-and-self-register`）

`registration_policy` singleton(`id=1` i32 PK,启动 seed 默认全关 + verify/approval 全开 + viewer 默认角色)运行时控制 email / OAuth 两路自助注册。CRUD 在扁平 `routes/registration_policy.rs`(`auth:manage`);注册在 `routes/register.rs`(公开,挂 sensitive 限流);审批扩展 `routes/users.rs`(`user:manage`)。

**user.status 状态机(4 态,2026-06-10 重构)**:`{Active, Disabled, Provisioned, PendingApproval}`。`Invited`→`Provisioned` **改名**(统称"已建档待确认",罩住 invite + self-register 两条流);verify 信号是**正交的** `email_verified_at: Option<ts>`,不是 status——**没有 pending_verify 状态**。

**正确做法**:

- **rename 数据迁移在 `swarmhive-migration` crate**(`m20260610_000001_rename_invited_to_provisioned`,2026-06-10 从 `db.rs::migrate_data` 重构):string_value 改名后 sea-orm 读旧值会反序列化失败 → 启动崩,migration 必须先于任何 User entity SELECT(`db::sync_schema` 内联 `run_migrations` 保证 dev/测试顺序;生产 `auto_sync=false` 走 bin 的 `db::run_migrations` 无条件执行)。详见上文「schema 走 schema-sync,data migration 走 swarmhive-migration」段。
- **`load_principal` 放行 `Active | PendingApproval`**:待审批用户 permission 集为空(所有 `require_permission!` 403),但 `/me` 必须可用——SPA 靠 `me.status==='pending_approval'` 收口到 `/awaiting-approval`。**代价**:无 permission 门的 session 端点要自查——`device.rs::require_session` 已显式加 Active 检查(待审批用户绝不能替 CLI 批 PAT)。新增无权限门的敏感端点时记得这个坑。
- **verify-email 消费分支靠 status 消歧**:`Provisioned` → 自助注册者(转 PendingApproval/Active + 写 session + 返 `next`);`Active` → ④ banner verify(只写时间戳,`next=null`)。invite-accept 走 `/invite/accept` 不碰本端点,天然不撞。role 在 `/register` 时就绑(verify 不重绑)。
- **公开 resend**(`POST /auth/verify-email/resend { email }`):自助注册者无 session 用不了 `me/send`;枚举防御恒 200,限速窗口内**静默吞**(429 会泄露账号存在性,与 authed send 的 429 不同)。
- **公开可见性端点** `GET /auth/registration-options` 只暴露三个布尔(email 开关/verify/approval),**不下发域白名单**——/login 注册链接与 /register 提示靠它(policy 本体要 `auth:manage`,匿名页拿不到)。
- **三态终态在 INSERT 前算好**(Provisioned / PendingApproval / Active),不要插入后再 UPDATE;免验证注册 `email_verified_at` 保持 NULL(邮箱确实没验证过,留给 banner 流补验,**不伪造 verified**)。
- **reject 级联是显式 TX 逐表删**(user_role/credentials/identity_link/account_token/session/login_attempts/api_token/device_authorization → user):schema-sync 的 FK 不保证 ON DELETE CASCADE。audit_log.actor_id 无 FK,保留作历史。
- **OAuth 自助分支**(`routes/oauth.rs::self_register_via_oauth`):域白名单 → 建号(`email_verified_at=now()`)+ identity_link + 默认角色 TX → 写 session → 302;race 靠 `user.email` 唯一约束兜底(`e.sql_err()` 匹配 `UniqueConstraintViolation` → 302 `/login?oauth_error=race_conflict`)。default org 按 `seed::DEFAULT_ORG_SLUG`(已 pub)查询。
- **operationId 撞名重演**:handler `register` 与 `setup.rs::register` 撞 → TS2300。改名 `register_account`。新 handler 名先 `rg "fn <name>"` 一遍。
- **成员管理三端点**(2026-06-10 用户扩展,同在 `routes/users.rs`,全 `user:manage`):`PUT /users/{id}/role`(整体替换绑定,禁选 owner)、`POST /users/{id}/disable`(仅 Active;置 Disabled 后 `revoke_user_sessions` 立即踢下线)、`POST /users/{id}/enable`(仅 Disabled)。共同护栏 `guard_not_owner_not_self`:不可操作 owner(防降级唯一 owner)与自己(防自降权锁死),422 typed `cannot-manage-{owner,self}`,**self 检查先于 owner 检查**。audit:`user_role_changed`/`user_disabled`/`user_enabled`。

**已知限制**:PendingApproval 用户登出后**密码重登会 401**(login 的 `VerifyOutcome::Inactive` 不区分 Disabled/PendingApproval)——主 UX 靠注册/verify/OAuth 当场写的 session;要支持重登需改 login 分支,暂记为后续增量。

**相关文件**:`crates/swarmhive-entity/src/{user,registration_policy}.rs`、`crates/swarmhive-server/src/{db.rs,routes/{registration_policy,register,verify_email,users,oauth}.rs,auth/service.rs,services/seed.rs}`、tests `{registration_policy,register,approval}_smoke.rs` + `oauth_smoke.rs` 自助注册段、`docs/13-rbac.md` "Registration Policy" 段。

## 遥测采集与聚合(`add-telemetry-events`)

更新链路埋点:server 天然事件落 `update_event`(可信),SDK 经公开 `POST /api/v1/events` 落 `client_event`(不可信,物理分表);rollup 双日表 + 周期任务;Admin `/telemetry` 统计页。完整口径见 `docs/10-telemetry.md`。

**正确做法**:

- **`update_available` 不是事件,是 `update_check` 的 `result` 维度**(`up_to_date|available|rollout_held`)——行数减半,且 `rollout_held` 让灰度观察成立。`download_intent` 同理(`redirected|failed`)。check 路由的 ~8 个早退出口统一归并 `up_to_date`(客户端视角"没更新";细分原因留 warn 日志)。
- **rollup 拆「可加/不可加」双表**:`event_rollup_day`(count,全维度,任意 SUM)与 `device_rollup_day`((app,day,version)→distinct client_id;version=NULL 行=当日总活跃)。**绝不要 SUM device_rollup 的行**——distinct 不可加,这是拆表的全部意义。设备数只从可信 `update_check` 统计(每设备 ≥12h 节流,频率天然归一;不混自报事件防伪造)。
- **swallow 写入**(`services/telemetry.rs::record_*`,复刻 audit 模式):遥测失败只 warn,check/download 主流程零影响;events 端点写库失败也仍返 200(fire-and-forget,客户端不重试)。
- **重算式 rollup 免水位**:每小时 TX 内 delete+insert 重算「今天+昨天」UTC bucket,幂等;清理任务删 `raw_retention_days`(并入既有 `[telemetry]` 配置段,与 log_level 同段)前的 raw——重算窗口只两天,旧 bucket 早固化,删 raw 不丢聚合。聚合用 raw SQL `INSERT...SELECT`(server 第三处刻意 raw SQL;sea-orm 不支持 INSERT...SELECT)。**`CREATE EXTENSION IF NOT EXISTS pgcrypto` 前置**——`gen_random_uuid()` PG13+ 才内置,测试容器可能更旧。
- **SUM 零行返回 NULL**:sea-orm `column_as(col.sum())` + `into_tuple::<Option<i64>>()` + `.flatten()`,直接 into_tuple::<i64>() 会在空表时 500(踩过)。
- **`SUM(bigint)` 返回 numeric,decode 成 `i64` 会 500(`add-dashboard-overview` 踩到)**:`event_rollup_day.count` 是 `i64`(bigint),Postgres `SUM(bigint)` 返回 **numeric**,sqlx 无法把 numeric 解码成 `i64`——**只有非零真实数据时才暴露**(SUM 全 NULL/空表时走 None 分支不解码,所以 summary/funnel/distribution 的既有测试一直没触发,潜伏到 overview 第一个用 SUM-with-data 断言才 500)。修法:`Func::cast_as(Column::Count.sum(), Alias::new("bigint")).into()` = `CAST(SUM(count) AS BIGINT)`,统一抽 `sum_count_bigint()` helper,所有按 count 求和处都走它。**注意 `cast_as` 是 sea-query 1.0-rc 的 `ExprTrait` 方法(`Expr` 上无 inherent),用静态 `Func::cast_as(expr, iden)` 免 trait 导入。**
- **隐私硬约束**:`update_event`/`client_event` **没有 ip/user_agent 列**(不是开关,是列不存在);client_event 自由文本(error_message 等)route 层截断。
- 周期任务在 bin 启动 spawn(`telemetry::spawn_tasks`,tokio interval + select),启动先跑一次 rollup;失败 warn 等下个周期。

**不要做**:

- 不要给 rollup 表装复合唯一键(重算模式天然无冲突,还躲开 Option 列 NULL-unique 坑)。
- 不要在 device_rollup 里混入 client_event(自报数据可伪造设备数)。
- 不要把"安装失败"做成采集事件(进程死透报不出)——查询层推断(download_completed 后版本长期未变)。

**相关文件**:`crates/swarmhive-entity/src/{update_event,client_event,event_rollup_day,device_rollup_day}.rs`、`crates/swarmhive-server/src/{services/telemetry.rs,routes/{events,telemetry,updates,download}.rs}`、tests `telemetry_smoke.rs` + `update_check_tauri_smoke::update_check_records_update_events_with_results`。

## Self-service 账户（`add-self-service-account`）

当前登录用户改自己的资料 / 密码，独立 vertical-slice `routes/account.rs`，**与 `routes/users.rs`（`user:manage` 门控的他人列表）泾渭分明**：这里作用域恒为「我自己」。

**自助端点的鉴权范式（区别于权限门控端点）**：

- 只取 `principal: Principal`（extractor 拒 Disabled/Provisioned;**自 ⑤ 起放行 PendingApproval**,见上节「load_principal 放行」条），**不**写 `require_permission!`——self-service 的作用域天然是调用者本人，没有「对别人的权限」一说。要改他人走 `user:manage`（`routes/users.rs`）。
- 允许 Session 与 Bearer(PAT 所有者即本人)；改密码的真正闸门是「current_password 校验(已有密码时)」或「已认证为该用户(OAuth-only 设密)」，不是 permission。
- `PATCH /users/me`：只可改 `display_name`（改邮箱要重验流程，单列 change）。**trim 后按 `chars().count()` 手动校验 1..=100**——不用 garde `length`，因为它的字节-vs-字符语义对 CJK 名不可靠（会把 34 个汉字当 102 字节误拒）。
- `PUT /users/me/password`：取 `user_credentials` 行判分支——有 → `current_password` 必填且 `password::verify`，错 `422 current-password-incorrect`；无（OAuth-only）→ 「设密」忽略 current。新密走 `validate_strong_password`。**TX 内 `service::upsert_credentials` + `service::revoke_user_sessions`，commit 后 `establish_session` 重发当前 session**（本设备留登录、其它踢掉），与 password_reset 完全同款语义。

**`upsert_credentials` / `revoke_user_sessions` 已从 `password_reset.rs` 私有提升到 `auth/service.rs`**（`pub(crate)`，泛型 `C: ConnectionTrait`）——满足「≥2 route 文件复用 → 提到共享层」规则，与 `establish_session` / `verify_password` 同住 auth/service。

**`MeResponse.has_password: bool`**：`/api/v1/auth/me` 新增此字段（`user_credentials` 的 `count > 0`，**不把 argon2 hash 读进内存**）。**有意不加到纯 `User` DTO**——它只是前端 Profile 页「改密码 vs 设密码」表单分支需要的信号，`User` 保持纯净。这是 oauth change 里「待真有 UI 分支需求再加」预言的兑现点。

**挂载**：`routes::account::router()` merge 进 `sensitive_routes()`（governor 限流——改密码可被对 current_password 在线暴力）；`openapi_router` 自动继承。handler 名 `update_me` / `change_password` 全局唯一（避开既有模块）。

**不要做**：

- 不要给自助端点加 `require_permission!`（作用域本就是自己，加了反而把 Owner 自己挡在外面或语义错乱）。
- 不要用 garde `length` 校验显示名长度（CJK 字节误判）；trim + `chars().count()`。
- 不要把 `has_password` 塞进 `User` DTO（只 `MeResponse` 需要）。

**相关文件**：`crates/swarmhive-server/src/routes/account.rs`、`auth/service.rs`（`upsert_credentials`/`revoke_user_sessions`/`establish_session`）、`routes/auth.rs`（`MeResponse.has_password`）、`crates/swarmhive-server/tests/account_smoke.rs`、`docs/13-rbac.md` "Self-service account" 段。

## Tauri 更新检查端点（`add-update-check-tauri`）

`GET /api/v1/updates/tauri/:app_slug`——公开、不限流、无 Principal，返回 Tauri v2 updater「dynamic update server」兼容响应。`routes/updates.rs` 单文件 vertical-slice（`add-update-check-rn-android` 后续在同文件加 `android` handler）。下载入口复用 `add-storage-and-presign-upload` 的 `download::download_url`。

**协议契约（务必照搬，否则与真实客户端不兼容；调研出处见 proposal Sources）**：

- **flat shape**：有更新 → `200` + `{version, pub_date?, url, signature, notes?, swarmhive{...}}`（顶层 url+signature，**不是** static 文件的 platforms map）；无更新 → `204 No Content` **空 body**（不要返回带空 version 的 JSON）。
- `version`/`url`/`signature` 必填；`pub_date` 用 `published_at.to_rfc3339_opts(SecondsFormat::Secs, true)`（出 `...Z`，贴近 Tauri 示例）；`notes` 可选。
- `signature` = `.sig` 文件**完整原文**（多行，含 untrusted/trusted comment），存在 `artifact.signature_metadata` 的 `tauri_signature` key。sea-orm `Json` 是 `serde_json::Value` 别名，**直接 `.get()` 不用 `.0`**。**无签名 artifact 在匹配阶段排除**（返回会让客户端验签失败）。
- 自定义字段放 `swarmhive` 命名空间（updater serde 忽略未知字段）。

**target/arch 错配根因（本 change 最大坑）**：

- updater 注入**分离**的 `{{target}}`（纯 OS 名 `darwin`/`windows`/`linux`）+ `{{arch}}`（`x86_64`/`aarch64`/`i686`/`armv7`），**不是** `darwin-aarch64` 合并串。
- 但 CLI 上传时 `artifact.target` 存 **Rust target triple**（`aarch64-apple-darwin`），`arch` 列恒 `None`（`publish.rs::plan_artifacts`）。
- 解法：**server 端 `parse_tauri_triple` 把 triple 解析成 (os, arch)** 再匹配，不改 CLI、不动已上传数据。优先级：精确 → `universal-apple-darwin`（darwin 任意 arch）→ 单 untargeted fallback → 204。

**灰度分桶（`rollout_percent`）**：

- `blake3(key) 前8字节 LE % 100 < percent`；key 三级回退 `client_id`(query) → IP(`x-forwarded-for`) → **都无则命中 + `tracing::warn!`**。`rollout=100/NULL` 整段短路。
- **直连部署约束**：bundled 单机直连无反代注入 XFF → IP 恒 None。SDK 不传 `client_id` 时灰度被旁路（50% 变 100%），故 warn 可观测。要直连灰度生效，SDK 必须传 `client_id`。
- **命名映射**：wire query/响应字段 `client_id`，telemetry 落库列 `anonymous_client_id`（同一匿名标识两端）。

**schema 扩展（schema-sync 安全）**：`release` 加 `min_version: Option<String>` + `rollout_percent: Option<i16>`，**都 nullable**（不用 `NOT NULL DEFAULT`，rc.38 schema-sync 回填不可靠，与 mail/account_token 同类坑），语义靠 `.unwrap_or(100)` 兜底。PATCH 校验 `rollout ∈ 1..=100`、`min_version` 合法 semver；**单层 `Option` 不支持回 NULL**（`null`/缺省都视作不改），清空走边界值（`min_version="0.0.0"` / `rollout=100`）。`create_release` 入口也加 semver 校验，杜绝坏 version 在分发时静默 204。

**semver**：引入 `semver = "1"`（workspace）。比较前两边都 `s.strip_prefix('v').unwrap_or(s)`（只削一个，不用 `trim_start_matches('v')` 以免误削多个）。`current_version` 解析失败 → 400 typed `invalid-current-version`；`rel.version` 失败 → warn + 204。

**204 / 404 / 400 决策**：204 = 无默认 channel / 无指针 / 非 published / 版本不更高 / 无 tauri artifact / 无匹配 / 无签名 / 不在灰度桶；404 = 未知 app slug / 指定 channel 不存在；400 = `current_version` 非 semver。`find_default_channel` 返回 `Option`（运维可取消默认）→ `None` 显式 204，不 unwrap。公开端点无 org_id，用新加的 `find_app_by_slug`（纯 `WHERE slug`，单组织下 slug 全局唯一）。

**埋点占位**：`tracing::info!(target:"telemetry", event="update_check"|"update_available", ...)`，字段对齐 `add-telemetry-events` 的 `update_event` 列（`anonymous_client_id`/`platform`/`release_id`/`storage_backend_id`），telemetry proposal 落库零改 emit 点（同 `download.rs::download_intent` 范式）。

**横切适配**：`error.rs::ApiErrorResponses` 补了 `400` 变体（之前只 401/403/404/409/410/422/500），否则 OpenAPI doc 缺 400 → SPA codegen 漂移。`ApiError::typed(BAD_REQUEST, ..)` 走 `Typed` 变体即可，无需新 `ApiError` enum 分支。admin codegen 走 `pnpm openapi`（dump-openapi → `/tmp/swarmhive-openapi.json` → `schema.gen.ts`），**不是** CLAUDE.md 顶部过时描述里的 `> apps/admin/src/lib/api/openapi.json`（那是不被使用的路径）。

**相关文件**：`crates/swarmhive-server/src/routes/updates.rs`、`routes/apps.rs`（`find_app_by_slug`）、`routes/releases.rs`（create/update 校验）、`error.rs`（400 变体）、`crates/swarmhive-entity/src/release.rs`、`crates/swarmhive-api-types/src/update.rs`、`crates/swarmhive-server/tests/update_check_tauri_smoke.rs`。
