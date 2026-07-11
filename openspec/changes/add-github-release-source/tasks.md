> **实现进度(本轮)**:服务端核心 + CLI flag + SDK/RN failover 已落地并全绿
> (cargo clippy + 66 unit tests / SDK tsc + 38 tests / registry-rn tsc / admin tsc + schema regen / biome)。
> 剩余(见下方未勾选项):CLI `register` 子命令、admin UI(badge + 配置表单)、
> registry-web 多按钮、docs、testcontainers 集成测试、外部 `swarmhive-action`。

## 1. 数据模型 & migration

- [x] 1.1 `swarmhive-entity/src/artifact.rs`:`storage_backend_id`/`object_key` 改 `Option`,新增 `pub mirror_url: Option<String>`;`From<&Model> for api::Artifact` 同步;`belongs_to storage_backend` 关系改可空
- [x] 1.2 新 `swarmhive-entity/src/github_source.rs`(仿 `oauth_provider`):`id`、`app_id`(FULL `#[sea_orm(unique)]`)、`owner`、`repo`、`tag_template`、`enabled`、`access_token_encrypted: Option<String>`、`created_at`/`updated_at`、`belongs_to app`;`pub mod github_source;` 注册进 `lib.rs`
- [x] 1.3 新 migration `m20260712_000001_github_source_and_artifact_delivery.rs`(raw SQL,不 import entity):`artifact ADD COLUMN mirror_url text`、`ALTER COLUMN storage_backend_id/object_key DROP NOT NULL`、`update_event ADD COLUMN source text`、`CREATE TABLE github_source(...)` + `app_id` 唯一约束 + FK;注册进 `migration/src/lib.rs` `migrations()`
- [x] 1.4 修 `routes/updates.rs` 对可空 `storage_backend_id` 的 tracing 日志(`?` Option 显示,不 panic)

## 2. api-types DTO 扩展

- [x] 2.1 `api-types/src/artifact.rs`:`storage_backend_id`/`object_key` 改 `Option`,加 `mirror_url: Option<String>`
- [x] 2.2 `api-types/src/upload.rs`:`CompletePart` 加 `#[serde(default)] pub mirror_url: Option<String>`;新增 `RegisterArtifactRequest`(平台/variant/kind/filename/size/sha256/signature?/mirror_url)
- [x] 2.3 `api-types/src/download.rs`:`DownloadArtifact` 加 `sources: Vec<DownloadSource{ kind, url }>`
- [x] 2.4 `api-types/src/update.rs`:`AndroidUpdateResponse` 加 `mirror_urls: Vec<String>`(`#[serde(default)]`)
- [x] 2.5 新 `api-types/src/github_source.rs`:`GithubSourceView{..token_set:bool}` / `CreateGithubSourceRequest` / `UpdateGithubSourceRequest`;注册进 `lib.rs` + `openapi.rs`

## 3. GitHub 源配置端点

- [x] 3.1 新 `routes/github_source.rs`:per-app GitHub 源 CRUD(`app:update` 门控),token AES-GCM 加密(`state.secret_key`),视图只出 `token_set`;blank-token-keeps-existing;挂进 `routes/mod.rs`
- [x] 3.2 openspec 校验:唯一 `app_id`、二次创建被拒

## 4. 写侧:mirror_url 记录 + 校验 + register 路径

- [x] 4.1 `uploads/service.rs upsert_artifact`:入参加 `mirror_url`,写入 ActiveModel + 纳入 `on_conflict.update_columns`;缺失时清空(与 signature 的"缺失保留"相反);store-time allowlist 校验(host=github.com + app 的 owner/repo),异源拒绝
- [x] 4.2 `routes/uploads.rs complete`:把 `part.mirror_url` 透传给 `upsert_artifact`
- [x] 4.3 新 register 端点(`artifact:upload`):无 presign/PUT,校验 sha256/size 存在,`mirror_url` 必填且过 allowlist,汇入同一 `upsert_artifact`(S3 两列为 None)
- [x] 4.4 finalize 对"仅 mirror、无 S3 对象"的 release 放行(artifact_count 仍 ≥1)

## 5. 读侧:源解析 + liveness/digest + telemetry

- [x] 5.1 `routes/download.rs`:`?source=oss|github` 解析;`github`/无 S3 → `mirror_url` 302;`oss`(缺省)→ active_backend;仅完全无源才 409;yank 仍 404
- [x] 5.2 新 `services/mirror.rs`:GitHub 资产 liveness + digest(HEAD/API 取 sha256 或 size,对比 `artifact.sha256`),`moka`/内存 TTL 缓存 + single-flight + 负缓存;可选 per-app token
- [x] 5.3 `download_catalog`:每 artifact 出 `sources[]`(S3 主 + 已校验 GitHub 镜像,URL 走 `?source=` 间接层);GitHub-only 只出 github 项
- [x] 5.4 `services/telemetry`:`download_intent` 落库加 `source` 维度(`update_event` 加列 + `event_rollup_day` 展开);`download.rs` 落库时传 source

## 6. update-check RN mirror_urls

- [x] 6.1 `routes/updates.rs android`:响应填 `mirror_urls`(已校验镜像的 `?source=github` 间接 URL);无则空数组

## 7. CLI & CI

- [x] 7.1 `swarmhive-cli publish`:`--mirror-url` flag(可多产物场景每次 publish 一个),塞进 `CompletePart`
- [ ] 7.2 `swarmhive-cli`:新 `register`/`publish --no-upload` 子命令走 register 端点(GitHub-only)
- [ ] 7.3 (外部仓库 `swarm-apps/swarmhive-action`)加 `github-mirror-url` input 转发 `release.yml:182` 的 URL —— 记录为跨仓 follow-up,本仓提供 CLI 契约

## 8. SDK & registry-rn failover

- [x] 8.1 `packages/sdk`:`ReleaseInfo` 加 `mirrorUrls?: string[]`;`normalizeAndroid` 从 `mirror_urls` 填充
- [x] 8.2 `packages/registry-rn` `rn-adapter.ts` + `expo-downloader.ts`:主源→镜像按序 failover;换源触发含错误页(既有 `assertApkDownload`)+ sha256 不符;全失败报 retryable error;无镜像时行为不变
- [ ] 8.3 (可选)`registry-web`/tauri download-panel 组件读 catalog `sources[]` 渲染多按钮

## 9. Admin

- [x] 9.1 regenerate `apps/admin/src/lib/api/schema.gen.ts`(server OpenAPI drift 后)
- [ ] 9.2 `releases/-shared.tsx` ArtifactsTable:只读 source badge(S3/GitHub)+ per-source 下载链
- [ ] 9.3 app 详情新增「GitHub 源」配置表单(owner/repo/token/enabled,blank-token-keeps,`key={id??'new'}` remount)+ Test 动作(dry-render tag + HEAD/digest 探测)

## 10. 测试

- [ ] 10.1 server 集成(testcontainers):GitHub-only 无 S3 后端下载 302 到 mirror(不 409)、download_intent `source=github`
- [ ] 10.2 verbatim/重命名:重命名 URL 原样落库原样 302;off-allowlist 被拒;重传去 stale
- [ ] 10.3 liveness:draft(匿名 404)不暴露、digest 不符不暴露、single-flight(mock GitHub via wiremock)
- [ ] 10.4 既有纯 S3 路径回归:下载/update-check/yank 行为不变;可空列不 panic
- [ ] 10.5 SDK:`normalizeAndroid` 填 mirrorUrls;rn-adapter 主源失败/ sha256 不符切镜像、全失败 retryable(vitest)

## 11. 收口

- [x] 11.1 OpenAPI drift gate:重生成 `schema.gen.ts` 并提交
- [ ] 11.2 docs:按实情修订 `docs/07`「镜像策略」「下载入口」;README fallback 段落
- [x] 11.3 `cargo test --workspace` / clippy / `pnpm lint` / typecheck 全绿
- [x] 11.4 `openspec validate add-github-release-source` 通过
