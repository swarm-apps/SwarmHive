# 存储与分发

## 设计目标

SwarmHive 的存储与分发目标是：控制面轻、下载快、存储接口统一。

SwarmHive Server 负责鉴权、策略、统计、埋点和下载入口。安装包、APK、OTA bundle 等大文件统一保存到 S3-compatible object storage。SwarmHive 不提供 local filesystem 作为正式存储后端；如果用户希望“一台服务器本地保存文件”，推荐使用 bundled RustFS single-server mode。

## 第一阶段存储策略

### S3-compatible Only

SwarmHive 第一阶段只维护一套正式存储后端：S3-compatible object storage。

这意味着：

- SwarmHive Server 不直接作为文件服务器。
- 产物最终都进入 S3-compatible bucket。
- 上传中转目录只用于临时缓存。
- 下载通过 public object URL 或 signed URL 完成。
- 从 RustFS 迁移到阿里云 OSS、R2、AWS S3 时只需要改 storage config。

### Bundled RustFS Single-server Mode

适合只有一台服务器、但又不想直接使用云对象存储的用户。

部署形态：

```text
single server
├─ swarmhive-server
├─ swarmhive-admin
├─ database
├─ rustfs
└─ nginx / caddy
```

SwarmHive 不把 RustFS 嵌入到自身进程，而是通过官方 Docker Compose profile 或 CLI 启动 RustFS 服务。SwarmHive 仍通过 S3 API 访问 RustFS。

推荐命令形态——仓库根的 `docker-compose.yml` 提供 `bundled-storage` profile：

```bash
docker compose --profile bundled-storage up -d
```

它起一个 RustFS（S3 API `:9000`、Web console `:9001`，默认凭证 `rustfsadmin/rustfsadmin`，数据存 named volume `swarmhive-rustfs-data`），并用一次性 init 容器**预建 bucket**——server 的 probe / 上传**不会自动建桶**，桶必须先存在。生产部署请用 `.env` 覆盖默认凭证（见 `.env.example`）。

起好后用 CLI 接入并激活：

```bash
swarmhive storage init rustfs \
  --bucket swarmhive \
  --access-key-id rustfsadmin \
  --access-key-secret rustfsadmin
```

`init rustfs` 会创建后端 → put/get/delete 探测 → 激活（热插拔 server 的活跃 handle），`force_path_style` 自动置 `true`。endpoint 默认 `http://localhost:9000`。

后台可以展示命令、检测健康状态、测试上传和下载，但不默认拥有任意执行 Docker 命令的能力。

### Existing S3-compatible Storage

适合已有对象存储的用户。

可接入：

- RustFS。
- MinIO。
- Garage。
- Cloudflare R2。
- AWS S3。
- 阿里云 OSS。
- 其他兼容 S3 核心 API 的对象存储。

### Aliyun OSS Preset

阿里云 OSS 是国内分发的重点示例，但仍通过 S3-compatible backend 接入。

需要注意：OSS 官方支持的是 S3 API 子集，且要求 virtual-hosted style。配置时通常应将 `force_path_style` 设为 `false`。

示例配置：

```toml
[storage.release_assets]
type = "s3"
endpoint = "https://oss-cn-hangzhou.aliyuncs.com"
bucket = "swarmhive"
region = "oss-cn-hangzhou"
access_key_id = "${SWARMHIVE_S3_ACCESS_KEY_ID}"
access_key_secret = "${SWARMHIVE_S3_ACCESS_KEY_SECRET}"
prefix = "apps"
force_path_style = false
public_base_url = "https://swarmhive.oss-cn-hangzhou.aliyuncs.com"
url_mode = "public"
```

### RustFS Preset

RustFS 是推荐的自托管 S3-compatible 后端。

示例配置：

```toml
[storage.release_assets]
type = "s3"
endpoint = "http://rustfs:9000"
bucket = "swarmhive"
region = "us-east-1"
access_key_id = "${SWARMHIVE_S3_ACCESS_KEY_ID}"
access_key_secret = "${SWARMHIVE_S3_ACCESS_KEY_SECRET}"
prefix = "apps"
force_path_style = true
public_base_url = "https://updates.example.com/assets"
url_mode = "signed"
```

具体 `force_path_style` 是否需要开启，应以 RustFS 部署方式和域名配置为准。

## 后台初始化向导

首次启动时，如果未配置 storage，Admin 应进入初始化向导：

```text
Storage Setup

1. Existing S3-compatible storage
2. Aliyun OSS
3. Single-server RustFS
```

向导需要支持：

- 连接测试。
- bucket 存在性检查。
- 自动创建 bucket（如后端支持）。
- test upload。
- test download。
- public URL / signed URL 检查。

## 下载入口

SwarmHive 提供统一下载入口，例如：

```text
GET /download/:app/:version/:artifact_id?source=oss|github
```

一个 artifact 可以有多个投递源：S3 对象（`oss`）和/或 GitHub Release 资源（`github` 镜像）。

**候选顺序三级**：

1. **显式 `?source=`** —— 最高优先，不被配置覆盖。
2. **per-platform 偏好** —— app 的 GitHub 源配置里的 `prefer_for_platforms`，列出哪些 platform 优先走 GitHub。
3. **缺省 `[oss, github]`** —— 未配偏好时的行为。

Server 处理：

1. 记录 `download_intent`（带 source 维度，结果为 redirected / failed）。
2. 按候选顺序解析投递源：`oss` 需要 artifact 有 S3 对象且后端活跃；`github` 需要镜像通过下方「镜像策略」里的可达性 / 摘要校验。
3. 命中第一个可用源：`oss` 生成 S3 public URL 或 signed URL，`github` 直接用记录的镜像 URL。
4. 返回 302 跳转；两个源都不可用时返回 409（引导先配置存储或注册镜像）。

自动 fallback：顺序只决定**先问谁**，可用性判定不受它影响，所以偏好**无法制造死链** —— 配了 GitHub 优先但镜像未通过校验（draft 窗口 / 摘要漂移）或源被停用时，自动回落到 S3；反之无 S3 对象或签名失败时回落到 GitHub 镜像。

### 何时该配 GitHub 优先

**阿里云 OSS 限制匿名下载 `.apk`**：它返回 XML 错误页而非安装包字节，客户端下载必然失败。此时应给该 app 配 `prefer_for_platforms: ["react-native-android"]`，让 APK 走 GitHub。

粒度是 **per-platform 而非 per-app**，因为该限制只针对 APK：桌面的 `.dmg` / `.exe` 在 OSS 上完全正常，且对国内用户比 GitHub 快得多，不该被一起推去 GitHub。

```bash
swarmhive source set --app my-app --owner my-org --repo my-repo \
  --prefer-platform react-native-android
# 清空回到全部 OSS 优先：--clear-prefer-platforms
```

也可在 admin 的 App > 来源页勾选。缺省是空（全部 platform 优先 OSS），**存量 app 行为不变**；翻转后用 `download_intent` 的 `source` 维度可查证实际投递源。

> 客户端 failover 是为**偶发**失败设计的（网络抖动、CDN 单点故障）。「阿里云 OSS × APK」是**结构性**不可用 —— 用 fallback 扛它，代价是每次更新都先完整下一遍错误页再重试。配置偏好才是对的修法：它把这个可预知的路由决策提前到服务端，且**存量客户端零改动**即可受益（它们只跟 302 走）。

公开下载目录 `GET /api/v1/downloads/:app_slug` 为每个 artifact 列出其当前可用源（`sources: [{ kind: "oss" | "github", url }]`），**按该 platform 的偏好排序**，推荐源在首位——无任何可用源的 artifact 不列出，避免下载页给出点了就 409 的死链。

Android 更新响应额外带 `mirror_urls`：它列的是**主源之外**的其余可用源（按 fallback 顺序），而非固定的 GitHub 候选。`download_url` 恒为裸间接入口、由 302 按偏好解析，所以偏好翻转对不认识 `mirror_urls` 的存量客户端完全透明。

MVP 推荐 302 跳转，简单、稳定、节省服务器带宽。

## 浏览器直传与 CORS

CLI 之外，Web Admin 也支持上传产物：浏览器复用同一套 presign / complete 契约，**直接 PUT 到对象存储**（不经 server 中转字节）。因为是跨域请求，桶必须配置 CORS 放行 Admin 源。

后台提供一键配置：`POST /api/v1/storage/backends/:id/cors`（需 `storage:manage`），body `{ allowed_origins: [...] }`（Admin 传自己的 `window.location.origin`）。server 用 `aws-sdk-s3` 的 `PutBucketCors` 写入规则：

- `AllowedMethods`: `PUT` / `GET` / `HEAD`
- `AllowedHeaders`: `*`（放行 `Content-MD5` / `x-amz-checksum-sha256` 等签名头的预检）
- `ExposeHeaders`: `ETag`
- `AllowedOrigins`: 调用方传入

后端不支持 `PutBucketCors` 时返回 `{ ok: false, detail }`（**非 5xx**），由前端提示手动配置。RustFS / MinIO / AWS S3 / Cloudflare R2 都支持一键配置。

### 阿里云 OSS 手动 CORS

**OSS 的 S3 兼容层不一定支持 `PutBucketCors`**，此时回退到 OSS 控制台手动配规则（等价上面四项）：

```text
来源 (AllowedOrigin):   https://<你的 Admin 域名>
允许 Methods:           PUT, GET, HEAD
允许 Headers:           *
暴露 Headers:           ETag
缓存时间:               3600
```

## 统计采集

下载统计可分为两层：

- DownloadIntent：用户点击或客户端开始下载。
- DownloadResult：下载完成或失败。

MVP 先记录 DownloadIntent。后续 SDK 可主动上报成功或失败。

## 镜像策略

现状：

- 统一 S3-compatible backend 仍是默认投递源。
- bundled RustFS 作为单服务器推荐模式。
- 阿里云 OSS 作为国内分发推荐示例。
- GitHub Release 升级为一等下载源（见下）。

### GitHub Release 作为一等下载源

GitHub Release 不再只是人工 fallback，而是可按 app 配置的正式投递源。它有两种用法：

- **镜像**：artifact 既有 S3 对象、又把同一份产物的 GitHub Release 资源 URL 记为镜像。下载入口在 `oss` / `github` 之间选源并自动 fallback。
- **无 S3 独立分发**：artifact 只记镜像 URL、不落 S3 对象（`storage_backend_id` / `object_key` 为空）。这样即使没有配置任何存储后端，也能纯靠 GitHub Release 对外分发。

**Per-app 配置**：`GET/PUT/DELETE /api/v1/apps/:slug/github-source` 维护每个 app 的 GitHub 源（`owner` / `repo`、tag 模板默认 `v{version}`、`enabled` 开关，以及只写的 `access_token`，仅用于服务端探测）。一个 app 一条源，PUT 为整体 upsert。

**镜像 URL 原样记录**：产物注册（`POST /api/v1/apps/:slug/releases/:version/uploads/register` 的 `mirror_url` 字段）时把外部 GitHub Release 资源 URL **逐字**存入 artifact。存入前做白名单校验：必须是 `github.com` 的 release-download URL；若该 app 配了 GitHub 源，则 URL 的 `owner/repo` 还必须与之匹配。

**读侧可达性 / 摘要闸门**：镜像在被暴露 / 导流前必须通过服务端校验（`services/mirror.rs`）——匿名可达（草稿 release 匿名 404 即挡）、资源已 `uploaded`、且摘要（`sha256`，缺则大小）与 artifact 一致。校验结果带 TTL 缓存、按 artifact 单飞、并做负缓存，以免草稿窗口轮询打爆 GitHub 匿名限流。源被禁用（`enabled=false`）或校验不过时，镜像既不进下载目录、也不作为 302 目标（绝不导流到会 404 的链接）。

> ⚠️ 桌面 Tauri updater 本轮不接入 GitHub 镜像：Tauri 的 `endpoints[]` 仍只从 S3 交付，`match_tauri_artifact` 要求 artifact 有 S3 对象，镜像对它是 no-op。

后续：

- 多存储后端。
- 区域路由。
- 镜像测速。
- CDN 配置。
- 桌面 Tauri updater 接入 GitHub 源。

## 安全建议

- CI/CD 上传使用服务端 API Token。
- 客户端更新检查使用只读 app key 或公开 endpoint。
- S3 secret 仅保存于 server，不下发到客户端。
- 私有 bucket 可使用短期签名 URL。
- Tauri 安全仍依赖 minisign 签名验证。
- Web Admin 不应默认执行任意系统命令；RustFS 启动以 CLI / Compose 指引为主。

## 文件路径规范

对象路径**去 channel、按版本寻址**（与发布列车指针模型一致——promote 只移 channel 指针，对象零动）：

```text
{prefix}/apps/{app_slug}/versions/{version}/{platform}/{target}/{filename}
```

- `{prefix}`：storage_backend 可选前缀，为空时省略该段。
- `{platform}`：`tauri-desktop` / `react-native-android`（`api::Platform` 的 kebab wire 值）。
- `{target}`：Tauri 取 target triple（如 `x86_64-pc-windows-msvc`），Android 取 abi（如 `arm64-v8a`），缺则回退 arch，再缺则 `any`。

示例：

```text
apps/swarmdrop/versions/0.4.5/tauri-desktop/x86_64-pc-windows-msvc/SwarmDrop_0.4.5_x64-setup.exe
apps/swarmnote-rn/versions/0.2.0/react-native-android/arm64-v8a/swarmnote-0.2.0-arm64.apk
```

> ⚠️ channel 不进对象路径：同一 release 被多个 channel 同时指向时，promote / rollback 不重传产物。详见 [03-architecture](03-architecture.md) 与 `add-storage-and-presign-upload`。
