# design

## Context

`add-app-release-artifact` 落了 release/artifact 元数据 + 发布列车生命周期，但 artifact 只读、字节无法进存储。本 proposal 补「字节进对象存储 + artifact 创建 + 下载分发」。技术栈在 2026-05-28 explore 已逐个拍板（见 [explore-summary](../../../dev-notes/explore-summaries/2026-05-28-upload-and-cli-stack.md)），本 design 把决策落成实体 / trait / 端点契约，并理清几处 explore 没覆盖的耦合（complete×publish、object_key、单 active backend、scope 边界）。

可复用既有事实：`crypto::SecretKey`（AES-256-GCM，mail 已用）、`MailerHandle` 的 hot-swap RwLock 模式、`crypto` 加密落盘格式、`Principal`/`require_permission!`/`services::audit`、`add-app-release-artifact` 的 release/artifact 实体与 `artifact.storage_backend_id` 裸列。

## Goals / Non-Goals

### Goals

- `Storage` trait + `S3Storage`（aws-sdk-s3）唯一实现；`AppState` hot-swappable active backend。
- storage_backend 配置 CRUD / test / activate（server API，供向导页消费）。
- presign（S3 原生 sha256 checksum）+ complete（幂等、写 artifact、可选 publish）。
- `GET /download/:app/:version/:artifact_id` 302 分发 + download_intent。
- CLI verify / publish / storage init 实化；cargo-dist 分发 + composite Action。

### Non-Goals

- **Admin 存储向导 UI** → `add-storage-wizard-page`（独立 Admin 页 proposal，本 proposal 只落 server API）。
- multipart 客户端分片、客户端 minisign 验签、APK AXML 解析、DownloadResult 漏斗、CDN/区域调度、orphan 清理、RustFS 进程托管。

## Decisions

### 1. 完整性：S3 原生 SHA256 checksum（explore 决策 1）

presign 用 aws-sdk-s3 `PutObject().checksum_sha256(expected_b64).presigned(...)`，签名内绑 `x-amz-checksum-sha256`。CLI PUT 带该头 → S3 收完自算 sha256，不符直接 4xx 拒。complete 时 server `HeadObject` 读回 `ChecksumSHA256` + `ContentLength` 确认，**不再次下载**。`artifact.sha256` 落的是已被 S3 验过的整体 sha256，客户端下载时再自校 → 端到端闭环。

老 OSS 的 S3-compat 对 checksum 支持度需在 `/test` 探测（写 `storage_backend.supports_sha256_checksum`）；不支持时 presign 退回普通 PutObject + 仅 size 确认（sha256 存 CLI 自报值）——本 proposal 默认按支持处理，回落作为 `/test` 标记后的运行时分支（见 Open Questions）。

### 2. 单 PUT、parts = 每文件一个（explore 决策 2）

一个 release 多个产物文件（Tauri：updater bundle + 安装包 + latest.json；Android：apk）。presign 请求 `files[]`，响应 `parts[]` 一一对应——**这里的 "part" 是「一个文件一个 presigned PUT」，不是 S3 multipart 的 part**。单文件单 PUT，retry 粒度 = 单文件。不引 multipart（composite checksum 与整体 sha256 冲突）。

### 3. object_key 生成（去 channel、版本寻址，explore 续探）

```
{prefix}/apps/{slug}/versions/{version}/{platform}/{target}/{filename}
```

- `platform` = `tauri-desktop` / `react-native-android`（api::Platform kebab）；`target` 取 artifact 的 target（Tauri triple）或 abi（Android）或 arch，缺则省略该段。
- 不含 channel → promote 只移指针、对象零动。server 内部生成，CLI 从 presign 响应拿 `object_key`，不自己拼。
- ⚠️ `docs/07` 文件路径规范段仍是旧的带 `channels/{channel}`，本 proposal apply 时一并改（diff gate）；`memory/project-storage-model.md` 已先行更新。

### 4. complete × publish 耦合 + 落实「≥1 artifact 才能 publish」

`complete { parts, publish?: bool }` 在**一个 TX** 内：

1. 对每个 part `HeadObject` 确认 checksum + size；任一不符 → 422 `upload_checksum_mismatch`（写 audit）。
2. 写/upsert `artifact` 行（`ON CONFLICT (release_id, platform, target, arch, abi)` 幂等）+ 标 `upload_session=completed`。
3. `publish=true` 时：校验该 release 现有 ≥1 artifact（落实 `add-app-release-artifact` 推迟的校验）→ 置 `release.published` + `published_at` + 写 publish audit。

权限：步骤 1-2 需 `artifact:upload`；`publish=true` 额外需 `release:publish`，缺则 **403**（不静默留 draft——避免「以为发布了其实没发」）。这呼应角色矩阵：developer（upload 无 publish）跑 `publish=false` 留 draft，release-manager / CI token（含 publish）跑 `publish=true`。

幂等：同 `upload_id` 重复 complete → 返相同 `release_id`（artifact upsert + session 已 completed 直接返回）。

### 5. release 必须先存在（draft）

presign 路径含 `:ver`，要求 release 已存在。CLI `publish` 先 `POST /releases`（idempotent：已存在的 draft 返 200/409-tolerated）建 draft，再 presign。presign **不**自动建 release（保端点单一职责）。因此 `swarmhive publish` 全流程需 `release:create` + `artifact:upload` + `release:publish`——单一内建角色都不全（developer 缺 publish、release-manager 缺 create），是 owner/admin 或按需 scoped 的 CI API token 的流程；这是有意的职责分离，不是 bug。

### 6. 单 active storage_backend（应用层 TX，非 partial unique）

与 `mail_provider` 同款：`activate` 在 TX 内先把其他行 `active=false` 再置自身 true，不装 `WHERE active` 的 partial unique index（rc.38 schema-sync bug）。`AppState.storage: Arc<RwLock<Option<StorageHandle>>>` 启动 wire active，activate/patch 后 `refresh_storage()` hot swap；无 active 时上传端点返 `409 storage_not_configured`。

### 7. secret 加密复用

`storage_backend.access_key_secret_encrypted` 用 `crypto::SecretKey`（同 `SWARMHIVE_SECRET_KEY`，mail 已建）。GET 不回写密文，只返 `secret_set: bool`。

### 8. 下载分发：302 redirect

`GET /download/:app/:version/:artifact_id`（公开 / 只读 app key）：查 artifact → 记 `download_intent`（telemetry 表，若遥测 proposal 未落则先 audit/log，见 Open Questions）→ 按 backend `url_mode` 生成 public 或 signed URL → `302`。不代理字节。yanked release 的 artifact → 404（不可分发）。

### 9. routes 组织

`routes/storage.rs`（backend CRUD/test/activate）、`routes/uploads.rs`（presign/complete）、`routes/download.rs`（redirect）。`storage/` 顶层放 trait + S3 impl（横切，被 uploads/download 复用）。

### 10. CLI 网络 / 分发栈（explore 决策 3/4/7）

reqwest `rustls-tls-native-roots` + `--ca-cert`/`SWARMHIVE_CA_CERT`；`sha2` 流式算 hash（边读边算边传）；`backon` 重试（5xx/timeout/conn-reset，4xx 直接报）。cargo-dist 一份配置产出二进制 + 安装脚本 + npm + Action composite 包装。

## Risks / Trade-offs

- **proposal 偏大**：跨 roadmap 阶段 3（存储）+ 5（CLI publish）。若 apply 时过载，可切 `add-storage-foundation`（trait+entity+backend API）与 `add-cli-publish`（presign/complete+CLI）两步——见 Open Questions。
- **OSS checksum 支持不确定**：决策 1 的前提；靠 `/test` 探测 + 运行时回落兜底。
- **complete 跨 part HeadObject N 次**：文件少（≤ 数个）可接受；不并发优化。
- **publish 全流程权限分散**：单角色不全是有意设计，CI 用 scoped token 解决，但文档需讲清。

## Migration Plan

- schema-sync +2 表 + artifact FK；无 backfill。
- 顺序：entity（storage_backend/upload_session + artifact FK）→ `storage` trait + S3 impl → `routes/storage` CRUD → `routes/uploads` presign/complete → `routes/download` → CLI verify/publish/storage-init → cargo-dist 配置 → 测试（testcontainers MinIO 跑 S3 行为）。
- `docs/07` 文件路径规范段去 channel（与本 proposal 一起 commit，过 diff gate）。

## Open Questions

- **是否拆两个 proposal**（storage-foundation / cli-publish）——若单 PR 过大则拆，apply 启动时定。
- **OSS 不支持 checksum 的回落**默认行为：本 proposal 倾向「探测到不支持 → 普通 PutObject + size 确认 + sha256 存 CLI 自报值 + warn」，但回落分支是否进 MVP 还是仅 AWS/MinIO/RustFS 先行，apply 时按 OSS 实测定。
- **download_intent 落点**：遥测实体（`add-telemetry-events`）未落，本 proposal 先写最小 `download_intent` 行还是先 structured log？倾向最小表，遥测 proposal 再扩。
- **verify 重复发布检查**需 release:read 查 server——离线 `--dry-run` 时跳过该项。
