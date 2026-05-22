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

推荐命令形态：

```bash
docker compose --profile bundled-storage up -d
```

或：

```bash
swarmhive storage init rustfs
```

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

SwarmHive 应提供统一下载入口，例如：

```text
GET /download/:app/:version/:artifact_id
```

Server 处理：

1. 记录 `download_intent`。
2. 判断 artifact 是否可用。
3. 根据 storage backend 生成 S3 public URL 或 S3 signed URL。
4. 记录 `download_redirected`。
5. 返回 302 跳转。

MVP 推荐 302 跳转，简单、稳定、节省服务器带宽。

## 统计采集

下载统计可分为两层：

- DownloadIntent：用户点击或客户端开始下载。
- DownloadResult：下载完成或失败。

MVP 先记录 DownloadIntent。后续 SDK 可主动上报成功或失败。

## 镜像策略

MVP：

- 统一 S3-compatible backend。
- bundled RustFS 作为单服务器推荐模式。
- 阿里云 OSS 作为国内分发推荐示例。
- GitHub Releases 作为人工 fallback。

后续：

- 多存储后端。
- 区域路由。
- 镜像测速。
- CDN 配置。

## 安全建议

- CI/CD 上传使用服务端 API Token。
- 客户端更新检查使用只读 app key 或公开 endpoint。
- S3 secret 仅保存于 server，不下发到客户端。
- 私有 bucket 可使用短期签名 URL。
- Tauri 安全仍依赖 minisign 签名验证。
- Web Admin 不应默认执行任意系统命令；RustFS 启动以 CLI / Compose 指引为主。

## 文件路径规范

建议对象路径：

```text
apps/{app_slug}/channels/{channel}/versions/{version}/{platform}/{arch}/{filename}
```

示例：

```text
apps/swarmdrop/channels/stable/versions/0.4.5/tauri/windows-x86_64/SwarmDrop_0.4.5_x64-setup.exe
apps/swarmnote-rn/channels/stable/versions/0.2.0/android/arm64-v8a/swarmnote-0.2.0-arm64.apk
```
