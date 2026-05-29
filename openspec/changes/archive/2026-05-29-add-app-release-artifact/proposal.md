# add-app-release-artifact

## Why

docs/03 中 App / Channel / Release / Artifact 是更新发布的核心业务实体。`add-persistence-foundation` 只落了鉴权所需实体（org / user / role / permission / session / audit），业务模型还没碰；存储（`add-storage-and-presign-upload`）、上传、更新检查（`add-update-check-*`）三条下游链路全部依赖这一块。

本 proposal 落 **业务实体 + 元数据 CRUD + 发布/promote/rollback 生命周期**，遵循 `openspec/project.md` 既定约定「回滚不删历史，仅改 channel 指向」（发布列车 / 指针模型）。**不碰字节流**——上传拆到 `add-storage-and-presign-upload`。

设计细节见 [design.md](design.md)。

## What Changes

### 1. 实体（`swarmhive-entity/src/`，主键 uuid v7）

- `app`：id、org_id、slug、display_name、platforms（`Json<Vec<api::Platform>>`）、created_at、updated_at。唯一 `(org_id, slug)`。
- `channel`：id、app_id、name、is_default、created_at、updated_at。唯一 `(app_id, name)`。`POST /apps` 时自动 seed `dev` / `beta` / `stable`（`stable` 为 default）。
- `release`：id、app_id、version、android_version_code（`Option<i64>`，RN 单调比较用；Tauri 为 null）、status（`ReleaseStatus`：`draft`/`published`/`yanked`）、release_notes、published_at、created_at、updated_at。唯一 `(app_id, version)`。
- `artifact`：id、release_id、platform（`Platform`）、target/arch/abi（`Option<String>`）、filename、size_bytes、sha256、storage_backend_id（裸 `Uuid`，FK 关系待存储 proposal 补）、object_key、signature_metadata（`Option<Json>`）、created_at。唯一 `(release_id, platform, target, arch, abi)`。**本 proposal 内只读**（creation 在存储 proposal 的 complete 回调）。
- `channel_release`：channel_id（PK）→ release_id、updated_at、updated_by。每 channel 至多 1 行 = 当前指向的 release。
- `channel_release_history`：id、channel_id、release_id、action（`ChannelAction`：`promote`/`rollback`）、reason、actor_id、created_at。promote/rollback append-only。

复用既有 `api::Platform`（`TauriDesktop`/`ReactNativeAndroid`）；枚举沿用 `UserStatus` 范式（实体 `DeriveActiveEnum` + `From<entity> for api` 双向转换 + `#[serde(rename_all=...)]` 对齐 `string_value`）。复合唯一约束用 sea-orm 2 `#[sea_orm(unique_key=...)]`，不用 raw `CREATE UNIQUE INDEX`（rc.38 schema-sync bug）。`From<&Model> for api::*` 写在 entity crate。

### 2. Server endpoints（RESTful，permission-gated）

```
GET    /api/v1/apps                                   app:read
POST   /api/v1/apps                                   app:create     ← 同 TX seed dev/beta/stable
GET    /api/v1/apps/:slug                              app:read
PATCH  /api/v1/apps/:slug                              app:update     ← 仅 display_name / platforms / default channel；slug 不可变
DELETE /api/v1/apps/:slug                              app:delete     ← 仅当无 release 时；否则 409 app_has_releases

GET    /api/v1/apps/:slug/channels                     app:read
POST   /api/v1/apps/:slug/channels                     app:update
PATCH  /api/v1/apps/:slug/channels/:name               app:update     ← rename / set default
GET    /api/v1/apps/:slug/channels/:name/release       release:read   ← 该 channel 当前服务的 release（0..1）

GET    /api/v1/apps/:slug/releases                     release:read
POST   /api/v1/apps/:slug/releases                     release:create ← 建 draft
GET    /api/v1/apps/:slug/releases/:version            release:read
PATCH  /api/v1/apps/:slug/releases/:version            release:update
POST   /api/v1/apps/:slug/releases/:version/publish    release:publish ← draft → published
POST   /api/v1/apps/:slug/releases/:version/yank       release:yank    ← published → yanked
GET    /api/v1/apps/:slug/releases/:version/artifacts   artifact:read

POST   /api/v1/apps/:slug/channels/:name/promote       release:promote  body { version }
POST   /api/v1/apps/:slug/channels/:name/rollback      release:rollback body { version? }
```

权限用既有 `api::PermissionName` + `require_permission!(p, X, Scope::App(app_id))`；**不新增权限**。channel 操作复用 `app:update`（无 `channel:*`）。`routes/apps.rs`（app + channel）/ `routes/releases.rs`（release + 生命周期 + artifact read），promote/rollback 共享 `releases.rs` 内 TX helper。

### 3. Channel 切换写历史（指针模型）

promote / rollback 在**一个事务**内：upsert `channel_release` 指针 + append `channel_release_history` + 写 `audit_log`。**永不删 release**。同一 release 可被多 channel 同时指向。rollback 无 `version` 时取 history 中当前指向之前的最近一条；无历史则 `422 nothing_to_rollback`。

### 4. CLI 只读命令

`swarmhive apps list` / `releases list --app <slug>` / `artifacts list --app --version`。读 GET 端点，`tabled` 人类表格 + `--output json`。**写命令（publish/promote/rollback）不在本 proposal**——publish 需存储链路，随 `add-storage-and-presign-upload` 一起。

### 5. 审计

app create/delete、release publish / promote / rollback / yank 写 `audit_log`（复用 `services::audit`）。

## Capabilities

### New Capabilities

- `app-release-artifact`：App / Channel / Release 实体 + CRUD + 发布列车生命周期（draft/publish/yank + promote/rollback 指针 + 历史）的可观测行为契约。

## Impact

- **Code**：entity crate +6 实体 + api-types DTO + From；server `routes/{apps,releases}.rs`；CLI 3 只读命令。
- **DB**：+6 业务表，无 backfill，不动鉴权表。
- **API**：`/api/v1/apps/**` 全套，触发 OpenAPI drift gate。
- **Deps**：CLI +`tabled`（list 表格）。server 无新增。
- **不影响**：鉴权 / mail / storage（storage_backend_id 裸列，FK 待存储 proposal）。

## Non-goals

- **不碰字节流**：presign / 上传 / artifact 创建 → `add-storage-and-presign-upload`。`publish` 暂不校验「≥1 artifact」（artifact 还无法创建），该校验随存储 proposal 补。
- **不做更新检查 / 版本比较 / 强制更新策略字段**（`upgrade_type` / `min_version` / `rollout_percent`）→ `add-update-check-*`（被 updater 消费前无意义，schema-sync 后加 nullable 列成本极低）。
- **不建 `provider_config` 实体**：OTA 预留、MVP 无消费者，推迟到 OTA provider 层。
- **不做 Admin SPA 页面** → 独立下游 proposal（`add-apps-page-ui` / `add-releases-page-ui`）。
- **不做 CLI 写命令**（publish/promote/rollback）→ 随存储 proposal。
- **不开 channel DELETE 端点**：dev/beta/stable 随 app 自动 seed，删通道罕见，MVP 不开。

## Depends on

- `add-auth-and-rbac`（archived）—— org / user / permission 集（app:* / release:* / artifact:* 已 seed）+ `Principal` + `require_permission!` + `services::audit`。
- `add-persistence-foundation`（archived）—— entity crate 基建 + schema-sync。

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) 业务实体。
- [docs/02-product-requirements.md](../../../docs/02-product-requirements.md) 应用 / 版本 / 产物管理。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 1。
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) sea-orm 2 entity + 权限 gating + audit。

## Acceptance

- Owner / Admin 能 CRUD app + channel；`POST /apps` 自动建 dev/beta/stable。
- Developer 能建 draft（`release:create`）但 publish 被拒（无 `release:publish`，403 problem+json）。
- Release Manager 能 publish / promote / rollback / yank，且都写 AuditLog。
- promote / rollback 在 `channel_release_history` 留 row，`channel_release` 指针更新，release 不被删。
- rollback 无目标且无历史 → `422 nothing_to_rollback`。
- DELETE 有 release 的 app → `409 app_has_releases`。
- 集成测试覆盖：创建 app → 建 release(draft) → publish → promote stable → rollback。
- `cargo clippy` / `cargo test --workspace` / `pnpm lint` 全绿；OpenAPI drift gate 通过。
