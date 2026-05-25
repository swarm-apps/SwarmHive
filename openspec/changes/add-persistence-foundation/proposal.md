# add-persistence-foundation

## Why

[docs/03-architecture.md](../../../docs/03-architecture.md) 的核心实体（Organization / User / Role / Permission / UserRole / App / Channel / Release / Artifact / StorageBackend / UpdateEvent / DownloadEvent / ProviderConfig / ApiToken / AuditLog）是后续所有 proposal 的基础。本 proposal 落地：

1. PostgreSQL 接入（dev: coolify；prod: compose profile）。
2. sea-orm 2.0 在 `swarmhive-entity` 中接好（依赖 [add-crate-restructure](../add-crate-restructure/proposal.md) 已建好的 entity crate），完成 entity 首批（鉴权所需）。
3. schema 演进策略：**纯 `schema-sync`**，不引入 sea-orm-migration crate（生产升级压力出现时再拆，详见 [add-crate-restructure](../add-crate-restructure/proposal.md) 决策）。

## What

### 1. Workspace 依赖

在 `[workspace.dependencies]` 新增（migration crate 不引入）：

```toml
sea-orm = { version = "=2.0.0-rc.38", features = [
    "sqlx-postgres", "runtime-tokio", "with-uuid", "with-chrono",
    "macros", "debug-print", "entity-registry", "schema-sync"
] }
sqlx    = { version = "0.8", features = ["postgres", "runtime-tokio", "uuid", "chrono"] }
figment = { version = "0.10", features = ["toml", "env"] }
```

### 2. 首批 entity（在 `swarmhive-entity/src/`）

仅落地**鉴权所需**实体，业务实体（App / Release / Artifact 等）拆到 `add-app-release-artifact`：

- `organization`
- `user`
- `identity_link`
- `role`
- `permission`
- `role_permission`（junction）
- `user_role`
- `session`（tower-sessions 用 sea-orm session store）
- `audit_log`

每个 entity model 同时提供 `impl From<&Model> for swarmhive_api_types::*` 转换（API DTO 跟 entity 字段 1:1 对应时）。

### 3. 配置 + DB 连接（在 `swarmhive-server/src/`）

- `swarmhive-server::config`：figment 加载 `AppConfig { server, database, telemetry }`。
- `swarmhive-server::state`：`AppState { db: DatabaseConnection, config: Arc<AppConfig> }`。
- 启动期：用 sea-orm 连 Postgres → 当 `database.auto_sync = true` 且非 prod profile 时调 `get_schema_registry("swarmhive_entity::*").sync(&db).await?`，否则跳过（生产 DBA 用 sea-orm-cli 或人工 SQL 控制 schema）。

### 4. Health endpoint

`/healthz` 已存在；扩展为返回 DB ping 结果（不带数据细节）。

## Acceptance

- `cargo run -p swarmhive-server` 能连到 dev Postgres 并通过 `schema-sync` 自动建出 9 张表（org / user / identity_link / role / permission / role_permission / user_role / session / audit_log）。
- `curl localhost:3030/healthz` 返回 200 + `{ "status": "ok", "db": "connected" }`。
- entity 之间的关系（User → IdentityLink 1:N、User ↔ Role 经 UserRole + RolePermission M:N）能在 SeaORM 中正确加载（写一个集成测试覆盖）。
- testcontainers 集成测试：起一个临时 Postgres，跑 sync + 增删改查冒烟。
- `cargo tree -p swarmhive-cli | grep -i sea-orm` 应**无**输出（CLI 不应被 ORM 污染）。
- 不引入 sea-orm-migration crate。

## Non-goals

- 不实现 App / Channel / Release / Artifact / StorageBackend / UpdateEvent / ApiToken 等业务实体（拆到后续 proposal）。
- 不写任何业务 handler，只暴露 `/healthz`。
- 不接入 password hashing / session middleware（拆到 `add-auth-and-rbac`）。
- 不引入 OAuth（拆到 `add-oauth-github`）。

## Depends on

- `add-toolchain-bump`（必须）
- `add-crate-restructure`（必须；entity 落到 `swarmhive-entity` crate）

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) Database 段。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 1 任务前置。
- [docs/13-rbac.md](../../../docs/13-rbac.md) Identity Providers / User / IdentityLink 模型。
