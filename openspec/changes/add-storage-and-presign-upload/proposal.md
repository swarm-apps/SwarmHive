# add-storage-and-presign-upload

## Why

docs/07 把存储抽象统一为 S3-compatible；docs/12 把 CLI publish 上传形态定为 **presign 直传 + complete 回调**（CLI 不走 server 中转字节）。`add-app-release-artifact` 落了 App/Channel/Release/Artifact 元数据 + 发布列车生命周期，但 artifact 是只读的——本 proposal 补上「字节怎么进对象存储 + artifact 怎么被创建」这条链，让 release 真正可发布、可下载。

技术栈决策见 [dev-notes/explore-summaries/2026-05-28-upload-and-cli-stack.md](../../../dev-notes/explore-summaries/2026-05-28-upload-and-cli-stack.md)（2026-05-28 explore 拍板）；本地细节见 [design.md](design.md)。

## What Changes

### 1. 实体（`swarmhive-entity/src/`）

- `storage_backend`：id、name、kind（`s3`）、active、endpoint、bucket、region、access_key_id、`access_key_secret_encrypted`（复用 `crypto::SecretKey` AES-256-GCM）、force_path_style、prefix、public_base_url、url_mode（`public`/`signed`）、signed_url_ttl_secs、supports_sha256_checksum（向导探测一次）、connectivity_status（`Option<Json>`）、created_at、updated_at。单 active 不变式靠**应用层 TX**（activate 先置其他行 false），不装 partial unique index（rc.38 schema-sync bug，与 mail_provider 同款 workaround）。
- `upload_session`：id、release_id、created_by、parts（`Json`，每项 object_key/relative_path/size/expected_sha256/platform/target/arch/abi/etag?/completed_at?）、status（`pending`/`completed`/`expired`）、expires_at、created_at。
- 补 `artifact.storage_backend_id` 的 `belongs_to storage_backend` 关系（`add-app-release-artifact` 留的裸列）。

### 2. Storage 抽象（`swarmhive-server/src/storage/`）

- `Storage` trait：`presign_put(object_key, sha256, size) -> PresignedPut`、`head(object_key) -> ObjectMeta`、`public_url / signed_url(object_key) -> Url`、`delete(object_key)`、`probe()`（put/get/delete `.swarmhive-probe`）。
- `S3Storage`（`aws-sdk-s3`）唯一实现：presign 用 `PutObject` + `presigned()`，绑定 `ChecksumSha256(expected)` → S3 收字节自算 sha256，不符直接拒。`force_path_style` 适配 RustFS(true)/OSS(false)。
- `AppState.storage: Arc<RwLock<Option<StorageHandle>>>`，启动期 wire active backend，无 active 则 None（上传端点返 409 storage_not_configured）；activate/patch 后 hot swap（同 mail refresh 模式）。

### 3. Server endpoints

存储后端管理（`storage:manage`）：

```
GET    /api/v1/storage/backends
POST   /api/v1/storage/backends
PATCH  /api/v1/storage/backends/:id
POST   /api/v1/storage/backends/:id/test        ← list bucket + put/get/delete .swarmhive-probe + 探测 checksum 支持
POST   /api/v1/storage/backends/:id/activate    ← TX 置单 active + hot swap
```

上传（在 `add-app-release-artifact` 已建的 release 下）：

```
POST   /api/v1/apps/:slug/releases/:ver/uploads/presign            artifact:upload
       req:  { files: [{ relative_path, size, expected_sha256, platform, target, arch?, abi? }] }
       resp: { upload_id, parts: [{ object_key, presigned_url, headers }] }   ← parts = 每文件一个

POST   /api/v1/apps/:slug/releases/:ver/uploads/:upload_id/complete  artifact:upload (+ release:publish if publish=true)
       req:  { parts: [{ object_key, sha256, etag }], publish?: bool }
       resp: { release_id, status, endpoints: { tauri?, android? } }
```

下载分发（公开 / 只读 app key，docs/07 下载入口）：

```
GET    /download/:app/:version/:artifact_id   ← 记 download_intent → 302 到 public/signed URL
```

- **object_key 生成**（去 channel、版本寻址）：`{prefix}/apps/{slug}/versions/{version}/{platform}/{target}/{filename}`。
- **presign**：按文件粒度签名，`expires` 5–10 min，绑 `x-amz-checksum-sha256`。
- **complete**：server `head` 读 checksum + size 确认（**不再次下载**）→ 一致后在一个 TX 写 `artifact` 行 + 标记 `upload_session=completed`；`publish=true` 时校验「≥1 artifact」后置 `release.published` + 写 publish audit（落实 `add-app-release-artifact` 推迟的校验）。`publish=true` 需调用者持 `release:publish`，否则 403（不静默留 draft）。幂等：同 upload_id 重复 complete 返相同 release_id。
- **失败重试**：CLI 持 upload_id + parts；单文件 PUT 失败只重发该文件（backon 指数+jitter，只重试 5xx/timeout/conn-reset）。

### 4. CLI（`swarmhive-cli`）

- `verify tauri|android`：文件存在 + version 重复检查（查 server）+ latest.json 可解析 + 算 sha256；versionName/Code 信任 `--flag`（不解析 build.gradle/APK AXML，与 explore 决策 6 一致）。
- `publish tauri|android`：读 `swarmhive.toml`（单 app + `--app` 覆盖；Tauri version 自动读 `tauri.conf.json`，Android 显式 `--version`/`--version-code`）→ 确保 draft release 存在 → 扫描产物算 sha256 → presign → 直传（reqwest stream + indicatif 进度 + backon 重试 + `x-amz-checksum-sha256` 头）→ complete（默认 `publish=true`）→ 输出 endpoint。
- `storage init rustfs`：输出 `docker compose --profile bundled-storage up -d` 指引 + health-check，调 server 建/激活 storage_backend（不主动跑 docker）。
- 网络栈：reqwest `rustls-tls-native-roots`（尊重企业/自签 CA）+ `--ca-cert`/`SWARMHIVE_CA_CERT` 逃生口；新依赖 `sha2`、`backon`。

### 5. 分发（cargo-dist）

引入 `dist`（cargo-dist）：一份配置产出各平台二进制（GH Releases + checksums）+ `curl|iex` 安装脚本 + `@swarm-hive/cli` npm 包；官方 GitHub Action 走 composite action（`npx @swarm-hive/cli` + inputs→flags）。

## Capabilities

### New Capabilities

- `storage-and-presign-upload`：S3-compatible 存储配置 + presign 直传（S3 原生 sha256 强校）+ complete 幂等回调（写 artifact / 选发布）+ 302 下载分发 + CLI verify/publish 的可观测行为契约。

## Impact

- **Code**：entity +2（storage_backend / upload_session）+ artifact FK；server `storage/` 模块 + `routes/storage.rs` + `routes/uploads.rs` + `routes/download.rs`；CLI verify/publish/storage-init 命令实化。
- **DB**：+2 表 + artifact FK 关系。
- **API**：`/api/v1/storage/**` + `/uploads/**` + `/download/**`，触发 OpenAPI drift gate。
- **Deps**：server +`aws-sdk-s3`；CLI +`sha2` +`backon` + reqwest features 切 `rustls-tls-native-roots`；构建侧 +`dist`（cargo-dist）。
- **不影响**：鉴权 / mail / RBAC entity。

## Non-goals

- **不做 Admin 存储向导 UI**：按既定「Admin 业务页独立 proposal」约定，向导页拆到 `add-storage-wizard-page`（本 proposal 只落它消费的 server CRUD/test/activate API）。
- **不做 S3 multipart 客户端分片**：单 PUT（multipart 的 composite checksum 与整体 sha256 强校冲突，见 explore 决策 2）。
- **不做客户端 minisign 验签 / APK AXML 解析**（explore 决策 6，留 verify 增强）。
- **不做 DownloadResult 上报 / 下载成功失败漏斗**（MVP 只记 download_intent；SDK 主动上报留遥测 proposal）。
- **不做 CDN URL 重写 / 区域调度 / orphan object 定期清理 / RustFS 进程托管**。

## Depends on

- `add-app-release-artifact`（pending）—— release/artifact 实体 + 生命周期 + artifact 裸 storage_backend_id 列。
- `add-pat-and-api-token`（archived）—— CLI publish / CI 的 scoped token。
- `add-mail-infrastructure`（archived）—— 复用 `crypto::SecretKey`（access_key_secret 加密）。

## Maps to docs

- [docs/07-storage-and-delivery.md](../../../docs/07-storage-and-delivery.md) 全文（文件路径规范段待更新为去 channel）。
- [docs/12-cli.md](../../../docs/12-cli.md) 上传形态 + publish/verify/storage init。
- [docs/06-cicd.md](../../../docs/06-cicd.md) GitHub Action 薄包装。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 3 + 4（部分）+ 5。
- [dev-notes/explore-summaries/2026-05-28-upload-and-cli-stack.md](../../../dev-notes/explore-summaries/2026-05-28-upload-and-cli-stack.md)

## Acceptance

- Owner 配置 S3 backend（填表 + `/test` 真实 put/get/delete probe + 探测 checksum 支持）→ activate → 上传链路解锁。
- CLI 发布 100 MB Tauri 安装包：有进度条；单文件失败只重传该文件；成功后 `artifacts list` 能看到 artifact，release 变 published。
- 同 upload_id 重复 complete 返回相同 release_id（幂等）。
- presign URL 过期后复用旧 URL 上传 → S3 返 `SignatureDoesNotMatch`；篡改字节 → S3 checksum 不符返 4xx。
- complete 的 `publish=true` 但调用者无 `release:publish` → 403。
- `GET /download/:app/:version/:artifact_id` → 记 download_intent → 302 到 public/signed URL。
- `force_path_style` 切换正确（OSS=false / RustFS=true）。
- `cargo clippy` / `cargo test --workspace` 全绿；OpenAPI drift gate 通过。
