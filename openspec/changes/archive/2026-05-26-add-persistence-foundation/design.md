# design

## Crate 边界（与 add-crate-restructure 对齐）

```text
┌────────────────────────────────────────────────────────────┐
│ swarmhive-api-types (lib)                                  │
│  serde DTO + utoipa::ToSchema                              │
│  本 proposal 加：ApiUser / ApiIdentityLink / ApiRole /     │
│                 ApiUserRole / ApiAuditLog 等               │
└────────────────────────────────────────────────────────────┘
              ▲
              │ used by
              │
┌────────────────────────────────────────────────────────────┐
│ swarmhive-entity (lib)                                     │
│                                                            │
│  src/                                                      │
│    lib.rs              — pub mod 声明 + EntityRegistry     │
│    organization/mod.rs                                     │
│    user/mod.rs         — Model + ActiveModel + From<&Model>│
│    identity_link/mod.rs                                    │
│    role/mod.rs                                             │
│    permission/mod.rs                                       │
│    role_permission/mod.rs    — junction (composite PK)     │
│    user_role/mod.rs                                        │
│    session/mod.rs                                          │
│    audit_log/mod.rs                                        │
│    common/                                                 │
│      datetime.rs       — DateTimeUtc 别名                  │
│      json.rs           — JsonValue / JSONB helpers         │
└────────────────────────────────────────────────────────────┘
              ▲
              │ depends on
              │
┌────────────────────────────────────────────────────────────┐
│ swarmhive-server (lib + bin)                               │
│                                                            │
│  src/                                                      │
│    lib.rs              — pub mod 声明 + build_router()     │
│    config/mod.rs       — figment 配置加载                  │
│    state.rs            — AppState { db, config }           │
│    db.rs               — DatabaseConnection 工厂 + sync    │
│    error.rs            — ApiError + RFC 9457 IntoResponse  │
│    routes/health.rs    — GET /healthz                      │
│    bin/server.rs       — tokio main + Axum                 │
└────────────────────────────────────────────────────────────┘
```

业务逻辑、handler、auth 都不在本 proposal 范围；只把 plumbing 通到能查 DB + 暴露健康检查 + 跑 schema-sync。

## Entity 关系图（首批）

```text
Organization (id, slug, name, created_at)
    │ 1
    │
    │ N
User (id, org_id, email, display_name, avatar_url, status, created_at, updated_at)
    │ 1                                                 │ 1
    │                                                   │
    │ N                                                 │ N
IdentityLink                                       UserRole
  (user_id, provider, subject, metadata, created_at) (user_id, role_id, scope_app_id, created_at)
                                                        │ N
                                                        │
                                                        │ 1
                                                    Role (id, name, description)
                                                        │ N
                                                        │
                                                        │ N (via RolePermission)
                                                    Permission (id, name, description)

Session (id, user_id, expires_at, ip, user_agent, created_at, last_seen_at)
    │ 1
    └ N → 跟 tower-sessions session-store crate 的 row 一一映射

AuditLog (id, actor_type, actor_id, org_id, app_id, action, resource_type, resource_id,
          ip, user_agent, metadata_jsonb, created_at)
```

**关键索引**：

- `user(email)` UNIQUE。
- `identity_link(provider, subject)` UNIQUE。
- `user_role(user_id, role_id, scope_app_id)` UNIQUE（scope_app_id NULL = org-level）。
- `audit_log(created_at DESC, org_id)`、`audit_log(actor_id, created_at DESC)`。
- `session(expires_at)`（清理用），`session(user_id)`。

## SeaORM Entity 写法

按 sea-orm-2 skill 中"新格式"统一用 `#[sea_orm::model]`，位置在 `crates/swarmhive-entity/src/user/mod.rs`：

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
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: UserStatus,             // sea_orm::DeriveActiveEnum
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    #[sea_orm(belongs_to, from = "org_id", to = "id")]
    pub organization: Option<crate::organization::Entity>,
    #[sea_orm(has_many)]
    pub identity_links: HasMany<crate::identity_link::Entity>,
    #[sea_orm(has_many)]
    pub user_roles: HasMany<crate::user_role::Entity>,
}
impl ActiveModelBehavior for ActiveModel {}

impl From<&Model> for api::User {
    fn from(m: &Model) -> Self {
        api::User {
            id: m.id,
            email: m.email.clone(),
            display_name: m.display_name.clone(),
            avatar_url: m.avatar_url.clone(),
            status: m.status.into(),     // api::UserStatus implements From<entity::UserStatus>
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
```

`api::User` 在 `swarmhive-api-types` 中定义（带 `utoipa::ToSchema`），entity 不反向依赖。

## Schema 同步策略

- 唯一策略：`swarmhive_entity` 暴露 `pub const REGISTRY_GLOB: &str = "swarmhive_entity::*";`。
- 启动期：`server::db::sync` 调 `get_schema_registry(REGISTRY_GLOB).sync(&db).await?`，仅在 `database.auto_sync = true` 时执行（配置默认 dev profile 开、prod profile 关）。
- 不引入 sea-orm-migration crate。生产升级路径走人工 SQL 或 `sea-orm-cli generate migration`（外部工具）即可，由部署者执行。
- 多人协作 dev 库不一致时直接 `docker compose down -v && up -d` 重建。

## 配置文件结构

```toml
# config/dev.toml
[server]
bind = "0.0.0.0:3030"
log_format = "pretty"

[database]
url = "postgres://swarmhive:swarmhive@localhost:5432/swarmhive_dev"
auto_sync = true              # dev only; prod profile 默认 false

[telemetry]
log_level = "info,swarmhive_server=debug,swarmhive_entity=debug"
```

环境变量覆盖示例：`SWARMHIVE_DATABASE__URL=postgres://...`（figment `__` 嵌套约定）。

## Risks

- sea-orm `=2.0.0-rc.38` 是 RC 版本。Risk: API 可能在正式版微调。Mitigation: pin 死版本；每次升级走单独 proposal。
- `schema-sync` 在多人协作时可能产生不一致的 dev DB 状态。Mitigation: dev DB 用 docker volume 可重建；CI 始终从空库跑 sync。
- 索引设计可能在写埋点事件后需要调整。Mitigation: AuditLog 用 BRIN 或 partition 是 phase 2 的事，这里只先建普通 btree。

## Open questions

- `session` 表是用 tower-sessions 自带 schema 还是我们自管？倾向自管（一张表能复用为 audit + 在线人数）。需要在 `add-auth-and-rbac` 决策。
- `IdentityLink.metadata` 用 JSONB？倾向是的（GitHub profile 数据放这里）。
