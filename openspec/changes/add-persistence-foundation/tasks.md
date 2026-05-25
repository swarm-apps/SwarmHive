# tasks

## Plumbing

- [x] [code] 在 `[workspace.dependencies]` 新增 sea-orm（不引入 sea-orm-migration）+ sqlx + figment
- [x] [code] `swarmhive-entity/Cargo.toml` 引入 sea-orm + serde + chrono + uuid + swarmhive-api-types
- [x] [code] `swarmhive-server/Cargo.toml` 引入 sea-orm + sqlx + figment + swarmhive-entity + dev-deps testcontainers
- [x] [code] `swarmhive-server/src/config/mod.rs`：`AppConfig { server, database, telemetry }` + figment 加载函数（toml + env layered, `SWARMHIVE_PROFILE` 切换）+ `ConfigError`（Box figment::Error 避免 large-err）
- [x] [code] `swarmhive-server/src/db.rs`：`connect` + `sync_schema`（9 张表 + IF NOT EXISTS）+ `ping`
- [x] [code] `swarmhive-server/src/error.rs`：`ApiError` thiserror enum + RFC 9457 `application/problem+json` `IntoResponse`

## API types（薄共享层）

- [x] [code] `swarmhive-api-types/src/lib.rs` 增加 pub mod 声明（user / identity / role / audit）+ pub use re-exports
- [x] [code] `api-types/src/user.rs`：`User`、`UserStatus`
- [x] [code] `api-types/src/identity.rs`：`IdentityLink`、`IdentityProvider`
- [x] [code] `api-types/src/role.rs`：`Role`、`Permission`、`PermissionName`（21 个 verb-scoped permission + `all()` 用于 seed）
- [x] [code] `api-types/src/audit.rs`：`AuditLog` 公开形态 + `ActorType`
- [x] [code] 全部 struct 加 `#[derive(Serialize, Deserialize, ToSchema)]`

## Entity（在 swarmhive-entity crate）

- [x] [code] `entity/src/common/mod.rs`：`DateTimeUtc` 别名
- [x] [code] `entity/src/organization/mod.rs`
- [x] [code] `entity/src/user/mod.rs` + `UserStatus` ActiveEnum (active/disabled/invited) + `From<&Model> for api::User` + `From<UserStatus> for api::UserStatus`（双向）
- [x] [code] `entity/src/identity_link/mod.rs` + `IdentityProvider` ActiveEnum + JSONB metadata（`(provider, subject)` UNIQUE 需要 schema-sync 后人工/迁移加索引）
- [x] [code] `entity/src/role/mod.rs`
- [x] [code] `entity/src/permission/mod.rs` + `TryFrom<&Model> for api::Permission`（按 wire name 映射）
- [x] [code] `entity/src/role_permission/mod.rs`（junction，`(role_id, permission_id)` 复合 PK）
- [x] [code] `entity/src/user_role/mod.rs`（含 `scope_app_id: Option<Uuid>`）
- [x] [code] `entity/src/session/mod.rs`
- [x] [code] `entity/src/audit_log/mod.rs`（metadata: `Json` / JSONB）+ `ActorType` ActiveEnum
- [x] [code] `entity/src/lib.rs` re-export 全部 + `pub const REGISTRY_GLOB: &str = "swarmhive_entity::*";`

## Server wiring

- [x] [code] `swarmhive-server/src/state.rs`：`AppState { db, config: Arc<AppConfig> }`（`Clone`）
- [x] [code] `swarmhive-server/src/lib.rs`：导出 `build_router(state) -> Router` + 新增 `db` pub mod
- [x] [code] `swarmhive-server/src/routes/health.rs`：`GET /healthz` 真实 DB ping，返回 `{status, db}` + 503 on failure
- [x] [code] `swarmhive-server/src/bin/server.rs`：加载 config → 连 DB → 可选 sync → seed → 起 Axum，graceful shutdown

## Seed

- [x] [code] 启动 seed：插入默认 Organization（slug = "default"）+ 5 角色 + 全部 21 permission + role-permission 绑定
- [x] [code] seed 幂等（OnConflict do_nothing + `.exec_without_returning(db)` 适配 sea-orm 2 RC 新 API）

## Test

- [x] [test] `tests/db_smoke.rs`（testcontainers + Postgres）：schema-sync → 插 user + identity_link + user_role → 校验关联与 5/21 计数 ✓ 通过（~30s）
- [x] [test] role + permission seed 幂等：跑两次 seed，3 张表 count 完全相同 ✓ 通过
- [x] [test] `cargo tree -p swarmhive-cli | grep sea-orm` 无输出 ✓ 依赖隔离保持

## Docs

- [x] [docs] [docs/03-architecture.md](../../../docs/03-architecture.md) 数据库段补"Entity 首批清单"（区分已落地 vs 待落地）
- [x] [docs] [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 1 任务里勾选已落地的 entity + sea-orm 2.0 新格式 + schema-sync 完成项
- [x] [docs] [CLAUDE.md](../../../CLAUDE.md) 增加 `cargo run -p swarmhive-server` 启动前提（`SWARMHIVE_DATABASE__URL` + `SWARMHIVE_DATABASE__AUTO_SYNC=true`）+ `cargo test --workspace` 提示
