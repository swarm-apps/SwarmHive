# design

## Context

`add-persistence-foundation` 只落了鉴权所需实体（org / user / role / permission / session / audit）。更新发布的核心业务实体——App / Channel / Release / Artifact——还没碰，而存储（`add-storage-and-presign-upload`）、上传、更新检查（`add-update-check-tauri` / `-rn-android`）三条下游链路全部依赖它。本 proposal 落 **业务实体 + 元数据 CRUD + 发布/promote/rollback 生命周期**，但**不碰字节流**（上传拆到存储 proposal）。

可复用的既有事实（避免重复发明）：

- `api::Platform`（`TauriDesktop` / `ReactNativeAndroid`，serde kebab-case）已在 `swarmhive-api-types/src/platform.rs` 定义。
- `api::Channel` 值枚举（`Dev` / `Beta` / `Stable` / `Custom(String)`，kebab-case）已存在——但它是「通道选择器」值类型，不是通道资源 DTO（见 Decision 5）。
- 权限 `app:{create,read,update,delete}` / `release:{create,read,update,publish,promote,rollback,yank}` / `artifact:{upload,read,delete}` 已在 `api::PermissionName` 定义并由 `services/seed.rs` 注入各内建角色。**本 proposal 不新增权限。**
- `Principal { user_id, org_id, scope, permissions }` + `require_permission!(p, PermissionName::X, Scope::App(app_id))` + `Scope::{None, App(Uuid)}` 鉴权基建就绪。
- `services::audit::{write, write_swallowing}(db, AuditEntry { action, resource_type, resource_id, .. })` 审计基建就绪。
- 项目约定（`openspec/project.md`）：**回滚不删历史，仅改 channel 指向**——指针模型是既定约定。

## Goals / Non-Goals

### Goals

- App / Channel / Release 实体 + RESTful CRUD，permission-gated、写 AuditLog。
- Release 生命周期：`draft → published → yanked`。
- Channel = 指向「当前 release」的命名指针（发布列车模型）；promote / rollback 只移指针 + append 历史，**永不删 release**。
- Artifact 实体 schema（本 proposal 内**只读**，creation 在存储 proposal）。
- CLI 只读命令：`apps list` / `releases list --app` / `artifacts list --app --version`。

### Non-Goals

- **不碰字节流**：presign / 上传 / artifact 创建 → `add-storage-and-presign-upload`。
- **不做更新检查 / 版本比较 / 强制更新策略字段**（`upgrade_type` / `min_version` / `rollout_percent`）→ `add-update-check-*`。这些字段在被 updater 消费前没有意义，schema-sync 后加 nullable 列成本极低。
- **不做 Admin SPA 页面** → 独立下游 proposal（`add-apps-page-ui` / `add-releases-page-ui`）。本 proposal 范围是 entity + server + CLI。
- **不建 `provider_config` 实体**：OTA 预留、MVP 无消费者，按「不为假想需求建表」原则推迟到 OTA provider 层。
- **不做 CLI 写命令**（publish / promote / rollback）：CLI publish 需要存储链路，本 proposal 只落只读 list；写命令随 `add-storage-and-presign-upload` 一起。

## Decisions

### 1. Channel = 指针，不是 release 容器（发布列车模型）

每个 channel 是一个命名指针，指向它当前服务的那个 release：

```
channel_release          channel_id (PK) ──▶ release_id      每 channel 至多 1 行（无行 = 该 channel 还没 promote 过任何 release）
channel_release_history  每次 promote / rollback append 一行（id PK, channel_id, release_id, action, reason?, actor_id, created_at）
```

- `version` 在 `(app_id)` 内唯一，**不**按 channel 区分——同一个 release 可被 `dev → beta → stable` 逐级 promote，产物只上传一次。
- promote / rollback 在**一个事务**内：upsert `channel_release` 指针 + append `channel_release_history` + 写 `audit_log`。
- 一个 release 可同时被多个 channel 指向（promote 到 stable 后 beta 仍指向它）。

### 2. Release 与 Channel 解耦；draft 不属于任何 channel

- `POST .../releases` 创建 `status=draft` 的 release，只挂在 app 下，不绑 channel。
- `publish` 把 `draft → published`（仅置状态 + `published_at`，不动任何 channel 指针）。
- 「这个 release 在哪些 channel 上线」= 查哪些 `channel_release` 行指向它（0..N）。
- `yank` 把 `published → yanked`（下架；指向它的 channel 指针**不自动回退**——回退是 rollback 的显式职责，yank 只标记不可分发，后续 update-check proposal 据此跳过）。

### 3. 实体清单（sea-orm 2 `#[sea_orm::model]`，主键 uuid v7）

| 实体 | 关键列 | 唯一约束 |
|---|---|---|
| `app` | id, org_id, slug, display_name, platforms `Json<Vec<api::Platform>>`, created_at, updated_at | `(org_id, slug)` |
| `channel` | id, app_id, name, is_default bool, created_at, updated_at | `(app_id, name)` |
| `release` | id, app_id, version, android_version_code `Option<i64>`, status `ReleaseStatus`, release_notes `Option<String>`, published_at `Option<DateTimeUtc>`, created_at, updated_at | `(app_id, version)` |
| `artifact` | id, release_id, platform `Platform`, target `Option<String>`, arch `Option<String>`, abi `Option<String>`, filename, size_bytes i64, sha256, storage_backend_id `Uuid`, object_key, signature_metadata `Option<Json>`, created_at | `(release_id, platform, target, arch, abi)` |
| `channel_release` | channel_id (PK), release_id, updated_at, updated_by | PK=channel_id |
| `channel_release_history` | id, channel_id, release_id, action `ChannelAction`, reason `Option<String>`, actor_id, created_at | — |

复合唯一约束用 sea-orm 2 `#[sea_orm(unique_key = "...")]` 同标签字段对表达，**不**用 raw `CREATE UNIQUE INDEX`（rc.38 schema-sync 对 `pg_indexes` ↔ `pg_constraint` 有 bug，见 mail/account_token 同款 workaround）。

`From<&Model> for api::*` 转换写在 entity crate。

### 4. version 唯一性 + Android versionCode

- `release.version` 是规范展示串（semver），`(app_id)` 内唯一。
- `release.android_version_code: Option<i64>`——RN Android updater 比较的是单调整数 versionCode，与展示版本号正交；Tauri release 该列为 null。在 create / update release 时可填。
- **版本比较语义**（semver 大小、versionCode 单调）属于 updater 逻辑，推迟到 `add-update-check-*`；本 proposal 只存不比。

### 5. 枚举：实体自定义 DeriveActiveEnum + serde 对齐

沿用 `UserStatus` 范式（实体定义 `DeriveActiveEnum` + `From<entity> for api` 双向转换），新增三个枚举：

- `ReleaseStatus`：`draft` / `published` / `yanked`
- `ChannelAction`：`promote` / `rollback`
- `Platform`（entity 侧）：`string_value` 与 `api::Platform` 的 kebab wire 对齐（`tauri-desktop` / `react-native-android`）+ `From` 互转

**所有「既落库又上 wire」的枚举必须显式 `#[serde(rename_all = "...")]` 对齐 `string_value`**，否则 serde 默认 PascalCase 与 DB 小写分叉、`POST` 必 422（mail_provider 踩过的坑，见 backend.md）。`ReleaseStatus` / `ChannelAction` 单词全小写用 `lowercase`；`Platform` 用 `kebab-case`。

`app.platforms` 是 `Json<Vec<api::Platform>>`（JSONB，serde 直存）；`artifact.platform` 是可查询单列，用 entity `Platform` DeriveActiveEnum。

通道资源 DTO 叫 `ChannelView { id, app_id, name, is_default, created_at, updated_at }`（`name` 为自由 `String`），与既有 `api::Channel` 值枚举区分——后者留给 update-check 的「通道选择器」复用（见 Open Questions）。

### 6. Artifact 在本 proposal 内只读

- 定义完整 schema + `GET .../releases/:version/artifacts`（`artifact:read`），**不开** create/delete 端点（presign complete 在存储 proposal 落）。
- `storage_backend_id` 先作为裸 `Uuid` 列存在，**不**在本 proposal 连 `belongs_to storage_backend` 关系（该实体在存储 proposal 才出现）；关系等存储 proposal 再补。
- `publish` **暂不**校验「至少一个 artifact」——artifact 还无法被创建。该校验推迟到存储 proposal（产物能上传后再加），本 proposal 的 publish 只翻状态。Acceptance 的发布链路（create→publish→promote→rollback）不依赖 artifact。

### 7. 权限 gating + scope

- 所有写操作用 `require_permission!(p, PermissionName::X, Scope::App(app_id))`；读用对应 `*:read`。
- **channel 操作无独立权限**，复用 `app:update`（`api::PermissionName` 无 `channel:*`）。
- 角色行为（由既有 seed 决定，本 proposal 不改）：
  - `developer` 有 `release:create` 但无 `release:publish` → 能建 draft、不能发布（Acceptance「developer 发 stable 被拒」成立）。
  - `release-manager` 有 publish/promote/rollback/yank 但无 `release:create` / `app:*write` → 发布开发者建的 draft、管 channel 指针，但不建 app / 不建 release。
- 敏感操作写 `audit_log`：app create/delete、release publish / promote / rollback / yank。

### 8. routes 组织（vertical slice）

- `routes/apps.rs`：app CRUD（5）+ channel 子资源（list/create/patch，3）= 8 端点。
- `routes/releases.rs`：release CRUD（list/create/get/patch，4）+ publish/yank（2）+ promote/rollback（2）+ artifact read（1）+ channel 当前 release 查询（1）= 10 端点。promote / rollback 共享一个「TX 内移指针 + append 历史 + audit」私有 helper，留在 `releases.rs`（同文件复用，未达「跨 route 文件复用」抽 `services/` 的阈值）。
- 两文件都 ~250 LOC 边缘，单一 feature 内聚，先不拆 service；后续真出现「两类不相关业务流同文件」再拆。

### 9. slug 不可变 + 删除语义

- `app.slug` 创建后不可变（进 URL 与未来对象键前缀）；`PATCH app` 只改 `display_name` / `platforms` / 默认 channel。
- `DELETE /apps/:slug`：仅当该 app **无任何 release** 时允许（否则 `409 type=app_has_releases`）——防误删导致孤儿存储对象 + 历史丢失。有 release 的 app 需先逐个 yank/清理（清理产物字节是存储 proposal 的事）。
- channel 无 DELETE 端点（dev/beta/stable 随 app 自动 seed，删通道是罕见操作，MVP 不开）。

### 10. app 创建自动 seed 三 channel

`POST /apps` 在同一事务内创建 app + seed `dev` / `beta` / `stable` 三个 channel，`stable.is_default = true`。MVP 不强制三个，但默认给齐（与 docs/03「按 app 注册时初始化」一致）。

## Risks / Trade-offs

- **version 全 app 唯一**：不能在不同 channel 复用同一 version 串——这是发布列车模型的有意取舍（一个 version = 一份产物，跨 channel 共享）。
- **publish 不校验 artifact**：本 proposal 可能 publish 出「空」release。缓解：存储 proposal 落地后补「publish 前必须 ≥1 artifact」校验；本阶段在 Open Questions 标注。
- **DELETE app 限制**：有 release 即拒删，可能让 dev 觉得繁琐，但优先防数据/对象丢失。
- **storage_backend_id 裸列无 FK**：存储 proposal 之前该列逻辑上悬空（无 artifact 行存在，无实际悬空数据）；存储 proposal 补 FK 关系即可。

## Migration Plan

- schema-sync 增量建 6 张新业务表，**无 backfill**（全新表，无存量数据）。
- 不改动任何既有鉴权表，无破坏性变更。
- 推进顺序：entity crate（6 实体 + api-types DTO + From）→ server routes（apps / releases）→ CLI 只读命令 → 集成测试。
- OpenAPI：新增 `/api/v1/apps/**` 全套触发 drift gate；admin `schema.gen.ts` 随 `dump-openapi` 重生成。

## Open Questions

- `api::Channel` 值枚举（Dev/Beta/Stable/Custom）与 `channel.name: String` 的关系：本 proposal 保持 `name` 自由串 + `ChannelView` DTO，不强塞枚举；等 `add-update-check-*` 真正需要「通道选择器」入参时再决定是否收敛。
- 「publish 前必须 ≥1 artifact」校验：推迟到 `add-storage-and-presign-upload`。
- rollback 无显式 `version` 时的目标：取 `channel_release_history` 中当前指向之前的最近一条；无历史则 `422 type=nothing_to_rollback`（见 spec）。
