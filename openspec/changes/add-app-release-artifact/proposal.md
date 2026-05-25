# add-app-release-artifact

## Why

docs/03 中 App / Channel / Release / Artifact / ProviderConfig 是更新发布的核心业务实体。`add-persistence-foundation` 只落了鉴权所需实体，业务模型还没碰；存储 / 上传 / 更新检查全部依赖这一块。

## What

### 1. 实体（在 `swarmhive-entity/src/`）

- `app`：id、org_id、slug (UNIQUE within org)、display_name、platforms (`Vec<Platform>` JSONB)、default_channel、created_at。
- `channel`：id、app_id、name、is_default、created_at。MVP 不强制三个默认 channel，按 app 注册时初始化（dev/beta/stable）。
- `release`：id、app_id、version、channel_id_at_publish、status (`draft`/`published`/`yanked`)、release_notes、upgrade_type、min_version、rollout_percent、published_at、created_at。
- `artifact`：id、release_id、platform、target、arch、abi、filename、size_bytes、sha256、storage_backend_id、object_key、signature_metadata JSONB（Tauri minisign sig 等）、uploaded_at。
- `channel_release`：channel_id、release_id（当前 channel 指向哪个 release；保留历史用 `channel_release_history` 表）。
- `channel_release_history`：channel_id、release_id、reason、actor_id、created_at（promote / rollback 全部留痕）。
- `provider_config`：app_id、provider_name、config_jsonb（OTA 预留，MVP 不消费）。

### 2. Server endpoints

按 RESTful 风格、permission gating：

```
GET    /api/v1/apps                              app:read
POST   /api/v1/apps                              app:create
GET    /api/v1/apps/:slug                        app:read
PATCH  /api/v1/apps/:slug                        app:update
DELETE /api/v1/apps/:slug                        app:delete

GET    /api/v1/apps/:slug/channels               app:read
POST   /api/v1/apps/:slug/channels               app:update
PATCH  /api/v1/apps/:slug/channels/:name         app:update

GET    /api/v1/apps/:slug/releases               release:read
POST   /api/v1/apps/:slug/releases               release:create        ← draft
PATCH  /api/v1/apps/:slug/releases/:version      release:update
POST   /api/v1/apps/:slug/releases/:version/publish   release:publish
POST   /api/v1/apps/:slug/releases/:version/yank      release:yank
POST   /api/v1/apps/:slug/channels/:c/promote         release:promote
POST   /api/v1/apps/:slug/channels/:c/rollback        release:rollback
```

upload 与 `complete` 端点拆到 `add-storage-and-presign-upload`。

### 3. Channel 切换写历史

promote / rollback 都不删 release，只更新 `channel_release` 当前指向并 append `channel_release_history` 行。

### 4. CLI 端

`swarmhive apps list` / `swarmhive releases list --app <slug>` / `swarmhive artifacts list --app --version` 命令落地。

## Acceptance

- Owner / Admin 能 CRUD app + channel。
- Release Manager 能 publish / promote / rollback / yank，且都写 AuditLog。
- 普通 Developer publish stable 被拒（403 problem+json）。
- promote / rollback 在 `channel_release_history` 留 row。
- 集成测试覆盖：创建 app → 创建 release（draft） → publish → promote → rollback。

## Non-goals

- 不实现 upload / artifact 字节流程（拆到 `add-storage-and-presign-upload`）。
- 不实现 Tauri / RN 更新检查（拆到对应 proposal）。
- 不实现 OTA provider 配置 UI（实体留 `provider_config`，但 endpoint 不开）。

## Depends on

- `add-auth-and-rbac`

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) 业务实体。
- [docs/02-product-requirements.md](../../../docs/02-product-requirements.md) "应用管理 / 版本管理 / 产物管理"。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 1。
