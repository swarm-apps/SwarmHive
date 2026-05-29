# CLI 设计

## 定位

SwarmHive CLI 是开发者本地发布、CI/CD 自动发布和日常运维操作的统一入口。

它不是 CI/CD 的附属品，而是 SwarmHive 的一等入口。Web Admin 适合查看、配置和排障；CLI 更适合上传产物、校验发布和执行自动化操作。

## 使用场景

### 本地手动发布

用户本地构建完成后，直接上传产物：

```bash
swarmhive publish tauri
swarmhive publish android
```

适合：

- 个人项目。
- 小团队手动发布。
- 临时热修版本。
- CI/CD 尚未配置完成的早期阶段。

### CI/CD 自动发布

GitHub Actions、GitLab CI、Gitea Actions、Jenkins 调用同一套 CLI：

```bash
swarmhive publish tauri --channel stable --notes-file CHANGELOG.md
```

### 运维操作

- 查看版本。
- promote。
- rollback。
- yanked 某个版本。
- 验证产物完整性。

## 配置文件

项目根目录可放置 `swarmhive.toml`：

```toml
server = "https://updates.example.com"
app = "swarmdrop"
default_channel = "stable"

[tauri]
artifact_dir = "src-tauri/target/release/bundle"

[android]
apk = "android/app/build/outputs/apk/release/app-release.apk"
```

CLI 参数优先级：

1. 命令行参数。
2. 环境变量。
3. `swarmhive.toml`。
4. CLI 全局配置。

## 认证

支持（`add-pat-and-api-token` 落地）：

- `swarmhive login [server]` —— 交互式 prompt email + 密码（不回显），调 `POST /api/v1/auth/cli-token`，server mint 一个 PAT 写回；CLI 把 `{server, email, token}` 写入 `~/.config/swarmhive/credentials.toml` 并 chmod `0600`（unix；Windows 走默认 ACL）。
- `swarmhive logout` —— 服务端 DELETE 当前 PAT（按 prefix 匹配自动定位 token id），再删本地文件。server 离线只 warn + 清本地。
- `SWARMHIVE_TOKEN` 环境变量 —— 优先级最高，覆盖 credentials 文件。CI/CD 注入 secret 即用。
- CI 推荐使用 API Token（scoped）而非 PAT；本地开发用 PAT。

示例：

```bash
# 本地登录（默认 http://localhost:3030；后续命令直接读 credentials.toml）
swarmhive login

# 远程 server + 显式 email
swarmhive login https://updates.example.com --email release@swarm-apps.dev

# CI：纯 env，不写文件
export SWARMHIVE_TOKEN=swhv_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
swarmhive publish tauri --app swarmdrop --version 0.4.5
```

Token 字符串格式：`swhv_pat_<43>`（个人）/ `swhv_api_<43>`（机器）。前缀公开仅是 metadata 便于日志 grep，无安全意义。

凭证文件位置（按 OS）：

- macOS：`~/Library/Application Support/dev.swarmhive.swarmhive/credentials.toml`
- Linux：`~/.config/swarmhive/credentials.toml`
- Windows：`%APPDATA%\swarmhive\swarmhive\config\credentials.toml`

参数优先级（与配置文件相同）：

1. 命令行参数 / 显式 env
2. `SWARMHIVE_TOKEN`
3. `~/.config/swarmhive/credentials.toml`
4. CLI 全局配置

## 命令设计

### init

```bash
swarmhive init
```

交互式生成 `swarmhive.toml`。

### verify

```bash
swarmhive verify tauri
swarmhive verify android
```

校验：

- 版本号。
- 产物存在性。
- Tauri latest.json。
- Tauri signature。
- target / arch。
- APK versionName / versionCode。
- 重复发布。

### storage init rustfs

```bash
swarmhive storage init rustfs
```

用于 single-server 模式，输出或执行官方 bundled RustFS 部署指引。

能力：

- 生成 RustFS 所需环境变量。
- 输出 Docker Compose profile 命令。
- 检测 RustFS endpoint 健康状态。
- 测试 bucket、上传和下载。
- 将结果写入 SwarmHive storage 配置。

### 上传形态：presign 直传 + complete 回调

`publish tauri` 与 `publish android` 共用一套上传流程，CLI 不走 server 中转，直接把字节 PUT 到对象存储：

```text
CLI                                      Server                          S3 / RustFS / OSS
 │  POST /api/v1/apps/{slug}/releases/{ver}/uploads/presign  ───▶  │
 │    { files: [{ relative_path, size,   │  artifact:upload check
 │      expected_sha256, platform,       │  release 须已存在（draft）
 │      target?, arch?, abi? }] }        │  生成 per-file presigned PUT
 │  ◀───  { upload_id, parts: [          │  绑 x-amz-checksum-sha256
 │           { object_key, presigned_url, │
 │             headers } ] }              │
 │                                                                       │
 │  PUT  <presigned_url>  (stream bytes + 回放 headers)  ───────────────▶│
 │  ◀────────────────────────────────────  S3 自算 sha256，不符 4xx 拒  │
 │                                                                       │
 │  POST .../uploads/{upload_id}/complete  ───▶  server HeadObject 校验   │
 │    { parts: [{ object_key, sha256 }], publish? } 仅读 checksum+size   │
 │                                       │            upsert artifact     │
 │                                       │   publish=true → 置 published   │
 │  ◀──  { release_id, status, endpoints }│
```

设计要点：

- presign 接口按文件粒度签名，`expires` 短（5–10 min）；release 须已存在，presign 不自动建。
- 完整性靠 S3 原生 checksum：presign 绑 `x-amz-checksum-sha256`，PUT 回放该头，S3 收完自算 sha256 不符直接拒；complete 仅 `HeadObject` 读回 checksum + size 确认，**不二次下载**。
- complete 接口幂等：同 `upload_id` 重复调用返回相同 release（artifact 走 `(release_id, platform, target, arch, abi)` 唯一键 upsert）。
- `publish=true` 额外需 `release:publish`（缺 403）+ release ≥1 artifact → 置 `published`；developer（无 publish）跑 `publish=false` 留 draft。
- server 仅承担鉴权、scope 检查、metadata 写入；不走产物字节，单 binary 不被带宽拖累。
- 失败重试：CLI 持有 `upload_id` 与 `parts[]`，单文件 PUT 失败只重发该文件（`backon` 指数退避，仅重试 5xx/timeout/connect）。
- 不引 S3 multipart 客户端分片：composite checksum 与整体 sha256 强校冲突，MVP 单文件单 PUT。

### publish tauri

```bash
swarmhive publish tauri \
  --app swarmdrop \
  --version 0.4.5 \
  --channel stable \
  --artifacts ./src-tauri/target/release/bundle \
  --notes-file CHANGELOG.md
```

自动处理：

- 扫描 Tauri bundle 目录。
- 读取 latest.json。
- 识别 updater artifact 和安装包。
- 对每个产物计算 sha256 → 调 `/uploads/presign`。
- 按返回的 presigned URL 直传 S3 / RustFS / OSS（带进度条）。
- 调 `/uploads/{id}/complete` 提交 hash 与 ETag。
- 输出 endpoint。

### publish android

```bash
swarmhive publish android \
  --app swarmnote-rn \
  --version 0.2.1 \
  --version-code 21 \
  --channel stable \
  --apk ./android/app/build/outputs/apk/release/app-release.apk
```

自动处理：

- 读取 APK metadata。
- 校验 versionName / versionCode。
- 计算 APK sha256 → presign → 直传 → complete（同上）。
- 输出 Android check endpoint。

### promote

```bash
swarmhive promote --app swarmdrop --version 0.4.5 --from beta --to stable
```

### rollback

```bash
swarmhive rollback --app swarmdrop --channel stable --to-version 0.4.4
```

### list

```bash
swarmhive apps list
swarmhive releases list --app swarmdrop
swarmhive artifacts list --app swarmdrop --version 0.4.5
```

## 用户体验要求

- 上传大文件必须有进度条。
- 所有发布命令支持 `--dry-run`。
- 错误信息要明确指出缺哪个文件、哪个签名、哪个字段。
- 发布成功后输出更新检查 URL 和下载 URL。
- CLI 输出应适合 CI 日志阅读。
- 支持 JSON 输出：`--output json`。

## 与 Web Admin 的关系

MVP 推荐：

- CLI / CI/CD 负责上传产物。
- Web Admin 负责查看、配置、统计和排障。

后续再考虑 Web 上传产物。

这样可以降低后台复杂度，也更符合开发者发布习惯。

## 与 GitHub Action 的关系

官方 GitHub Action 应该尽量薄，只包装 CLI：

- 下载 CLI。
- 注入 server / token。
- 调用 verify。
- 调用 publish。

这样可以保证本地发布和 CI/CD 发布行为一致。

