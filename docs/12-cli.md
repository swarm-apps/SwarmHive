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

支持：

- `swarmhive login` 本地保存 token。
- `SWARMHIVE_TOKEN` 环境变量。
- CI secret 注入。

示例：

```bash
swarmhive login https://updates.example.com
```

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
- 上传文件。
- 创建 release / artifact。
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
- 上传 APK。
- 创建 release / artifact。
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

