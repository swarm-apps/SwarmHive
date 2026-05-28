# tasks

## 1. Entity (`swarmhive-entity/src/`)

- [ ] 1.1 `app.rs`：Model（id/org_id/slug/display_name/platforms `Json<Vec<api::Platform>>`/created_at/updated_at）+ `#[sea_orm(unique_key="org_slug")]` on (org_id, slug) + `belongs_to organization` + `has_many channels` + `has_many releases` + `From<&Model> for api::App`
- [ ] 1.2 `channel.rs`：Model（id/app_id/name/is_default/created_at/updated_at）+ unique (app_id, name) + `belongs_to app` + `From<&Model> for api::ChannelView`
- [ ] 1.3 `release.rs`：Model（id/app_id/version/android_version_code `Option<i64>`/status `ReleaseStatus`/release_notes/published_at/created_at/updated_at）+ unique (app_id, version) + `belongs_to app` + `has_many artifacts` + `ReleaseStatus` DeriveActiveEnum（`#[serde(rename_all="lowercase")]` draft/published/yanked）+ `From` 双向 + `From<&Model> for api::Release`
- [ ] 1.4 `artifact.rs`：Model（id/release_id/platform `Platform`/target/arch/abi `Option<String>`/filename/size_bytes/sha256/storage_backend_id `Uuid`/object_key/signature_metadata `Option<Json>`/created_at）+ unique (release_id, platform, target, arch, abi) + `belongs_to release` + entity `Platform` DeriveActiveEnum（`#[serde(rename_all="kebab-case")]`，`From` ↔ `api::Platform`）+ `From<&Model> for api::Artifact`
- [ ] 1.5 `channel_release.rs`：Model（channel_id PK / release_id / updated_at / updated_by）+ `belongs_to channel`（1-1）+ `belongs_to release`
- [ ] 1.6 `channel_release_history.rs`：Model（id/channel_id/release_id/action `ChannelAction`/reason/actor_id/created_at）+ `ChannelAction` DeriveActiveEnum（`#[serde(rename_all="lowercase")]` promote/rollback）+ `From` 双向 + `belongs_to channel/release`
- [ ] 1.7 `lib.rs` 注册新 module；`before_save` 填 created_at/updated_at（参考既有 entity）
- [ ] 1.8 entity 单测：`ReleaseStatus` / `ChannelAction` / entity `Platform` 的 serde round-trip 锁 wire 值（防 DeriveActiveEnum+serde 分叉，参考 mail_provider::tests）

## 2. api-types (`swarmhive-api-types/src/`)

- [ ] 2.1 `app.rs`：`App`（含 platforms `Vec<Platform>`、default_channel name）+ `CreateAppRequest { slug, display_name, platforms }` + `UpdateAppRequest { display_name?, platforms?, default_channel? }`，全 `ToSchema`
- [ ] 2.2 `channel.rs`：新增 `ChannelView { id, app_id, name, is_default, created_at, updated_at }` + `CreateChannelRequest` + `UpdateChannelRequest`（保留既有 `Channel` 值枚举不动）
- [ ] 2.3 `release.rs`：`Release`（version/android_version_code/status/release_notes/published_at/...）+ `ReleaseStatus`（wire enum）+ `CreateReleaseRequest { version, android_version_code?, release_notes? }` + `UpdateReleaseRequest` + `PromoteRequest { version }` + `RollbackRequest { version? }`
- [ ] 2.4 `artifact.rs`：`Artifact`（platform/target/arch/abi/filename/size_bytes/sha256/...）+ `ChannelAction`（wire enum）+ `ChannelReleaseHistoryEntry`
- [ ] 2.5 `lib.rs` re-export 全部新类型

## 3. Server — `routes/apps.rs`（app + channel）

- [ ] 3.1 `GET /apps`（`app:read`）list + `POST /apps`（`app:create`）：同 TX 建 app + seed dev/beta/stable（stable default）+ audit
- [ ] 3.2 `GET /apps/:slug`（`app:read`）+ `PATCH /apps/:slug`（`app:update`，仅 display_name/platforms/default_channel，slug 不可变）
- [ ] 3.3 `DELETE /apps/:slug`（`app:delete`）：有 release → `409 app_has_releases`；否则删 + audit
- [ ] 3.4 `GET/POST /apps/:slug/channels` + `PATCH /apps/:slug/channels/:name`（`app:read`/`app:update`）：set default 时同 TX 取消旧 default
- [ ] 3.5 全部挂 utoipa::path 注解 + tag `apps`；`router()` 暴露并在 `lib.rs` 的 `openapi_router()` + `build_router()` 挂载

## 4. Server — `routes/releases.rs`（release 生命周期 + artifact read）

- [ ] 4.1 `GET/POST /apps/:slug/releases`（`release:read`/`release:create`）+ `GET/PATCH /apps/:slug/releases/:version`
- [ ] 4.2 `POST .../publish`（`release:publish`）draft→published + published_at + audit
- [ ] 4.3 `POST .../yank`（`release:yank`）published→yanked + audit（不动 channel 指针）
- [ ] 4.4 `promote_or_rollback` 私有 TX helper：upsert `channel_release` + append `channel_release_history` + audit；rollback 无 version 取 history 前一条，无历史 → `422 nothing_to_rollback`
- [ ] 4.5 `POST .../channels/:name/promote`（`release:promote`）+ `POST .../channels/:name/rollback`（`release:rollback`）调 helper
- [ ] 4.6 `GET .../releases/:version/artifacts`（`artifact:read`）+ `GET .../channels/:name/release`（`release:read`，空 channel 返空而非 404）
- [ ] 4.7 utoipa::path + tag `releases`；`router()` 挂载到 `openapi_router()` + `build_router()`

## 5. Error 类型

- [ ] 5.1 `error.rs` 加 Typed 子类型 `app_has_releases`（409）、`nothing_to_rollback`（422）；release/version not-found 走既有 NotFound

## 6. CLI 只读命令（`swarmhive-cli`）

- [ ] 6.1 `commands/apps.rs`：`apps list` → GET /apps → `tabled` 表格 + `--output json`
- [ ] 6.2 `commands/releases.rs`：`releases list --app <slug>`
- [ ] 6.3 `commands/artifacts.rs`：`artifacts list --app <slug> --version <v>`
- [ ] 6.4 clap：在 `commands/mod.rs` 挂 `apps` / `releases` / `artifacts` 子命令；全局 `--output {table|json}`（default table）
- [ ] 6.5 Cargo.toml：CLI 加 `tabled.workspace = true`（workspace root 先 pin）

## 7. OpenAPI / 前端类型

- [ ] 7.1 `openapi_surface.rs` 加新 paths / tags（apps / releases）/ schemas 断言
- [ ] 7.2 跑 `pnpm --filter @swarmhive/admin openapi` 重生成 `schema.gen.ts`（drift gate）

## 8. 集成测试（`crates/swarmhive-server/tests/`）

- [ ] 8.1 `app_release_smoke.rs`（testcontainers Postgres + `build_router`）：创建 app → 验三 channel seed
- [ ] 8.2 release 生命周期：建 draft → publish（验 status + published_at + audit）→ promote stable（验指针 + history）→ rollback（验回退 + release 仍在）
- [ ] 8.3 RBAC：developer 建 draft OK、publish 403；release-manager publish/promote OK、建 app 403
- [ ] 8.4 边界：重复 slug 409、重复 version 409、删有 release 的 app 409、空历史 rollback 422、同 release promote 到两 channel

## 9. 文档 / 知识库

- [ ] 9.1 `docs/03-architecture.md` 业务实体段标注 App/Channel/Release/Artifact 已落地 + 指针模型（channel_release / history）
- [ ] 9.2 `dev-notes/knowledge/backend.md` 加「发布列车 / channel 指针 + promote/rollback TX + 历史」段
- [ ] 9.3 `openspec/changes/README.md` 状态行更新
