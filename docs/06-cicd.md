# CI/CD 设计

CI/CD 是 SwarmHive 的一等能力。目标是让发布流程从“构建完成后手动同步第三方平台”变成“CI 自动注册版本、上传产物、生成更新元数据、发布策略生效”。

同时，SwarmHive CLI 也要支持开发者在本地手动发布。CI/CD 与本地发布使用同一套命令和配置，避免维护两套流程。

## 发布目标

- 自动识别构建产物。
- 自动上传文件到 SwarmHive 管理的存储后端。
- 自动注册 release 与 artifact。
- 自动注入 changelog。
- 自动设置 channel 和更新策略。
- 支持 promote 与 rollback。
- 支持本地手动发布和 CI/CD 自动发布。

## CLI 命令

### login

配置 server 和 token：

```bash
swarmhive login https://updates.example.com
```

### init

初始化项目配置：

```bash
swarmhive init
```

生成 `swarmhive.toml`：

```toml
server = "https://updates.example.com"

[app]
slug = "swarmdrop"

[app.tauri]
conf = "src-tauri/tauri.conf.json"
artifacts = ["src-tauri/target/release/bundle/.../installer", "latest.json"]

[app.android]
apk = "app/build/outputs/apk/release/app-release.apk"
```

### verify

发布前校验：

```bash
swarmhive verify tauri --artifacts ./src-tauri/target/release/bundle
```

校验内容：

- 版本号格式。
- Tauri latest.json 是否存在。
- 签名是否存在。
- target / arch 是否可识别。
- APK versionName / versionCode 是否可识别。
- 是否重复发布。

### publish

本地发布 Tauri 产物：

```bash
swarmhive publish tauri \
  --app swarmdrop \
  --version 0.4.5 \
  --channel stable \
  --artifacts ./src-tauri/target/release/bundle \
  --notes-file CHANGELOG.md
```

本地发布 Android APK：

```bash
swarmhive publish android \
  --app swarmnote-rn \
  --version 0.2.1 \
  --version-code 21 \
  --channel stable \
  --apk ./android/app/build/outputs/apk/release/app-release.apk
```

如果配置了 `swarmhive.toml`，可以简化为：

```bash
swarmhive publish tauri
swarmhive publish android
```

`publish` 内部统一走 **presign 直传 + complete 回调**：CLI 先向 server 申请 per-file presigned PUT URL，把产物字节直接传到 S3 / RustFS / OSS，再调 complete 提交 sha256 / ETag 由 server 校验并创建 release。详细流程见 [CLI 设计](12-cli.md#上传形态presign-直传--complete-回调)。CI/CD 与本地发布共用同一套流程，server 不走产物字节，单 binary 不被带宽拖累。

### promote

提升 channel：

```bash
swarmhive promote --app swarmdrop --version 0.4.5 --from beta --to stable
```

### rollback

回滚 channel：

```bash
swarmhive rollback --app swarmdrop --channel stable --to-version 0.4.4
```

## GitHub Action

官方 Action 包装 CLI：

```yaml
- uses: swarmhive/action-upload@v1
  with:
    server: ${{ secrets.SWARMHIVE_SERVER }}
    token: ${{ secrets.SWARMHIVE_TOKEN }}
    app: swarmdrop
    platform: tauri
    channel: stable
    version: ${{ steps.version.outputs.version }}
    artifacts: src-tauri/target/release/bundle
    notes-file: CHANGELOG.md
```

## Changelog 来源

支持：

- git-cliff 输出。
- GitHub release notes。
- CHANGELOG.md。
- 手写文本。
- CI input 参数。

## 策略参数

CLI 和 CI 可以直接设置：

- `upgrade-type`: prompt / force / silent。
- `min-version`。
- `rollout-percent`。
- `channel`。
- `publish`: true / false。

## 推荐流水线

### Tauri

1. checkout。
2. install deps。
3. build Tauri。
4. generate changelog。
5. swarmhive verify。
6. swarmhive publish to beta。
7. 可选人工审批。
8. swarmhive promote beta to stable。

### React Native Android

1. checkout。
2. install deps。
3. build APK。
4. extract versionName / versionCode。
5. swarmhive verify。
6. swarmhive publish。

## 回滚原则

回滚不删除历史版本，只修改 channel 指向。

这样可以：

- 保留审计记录。
- 避免已下载用户受影响。
- 保证回滚操作可追踪。

## SwarmHive 自身的发布（三条 release 命名空间）

以上讲的是**用户用 SwarmHive 发布自己 app 的更新**。SwarmHive 这套软件本身的发布按
`<name>/v<version>` tag 命名空间分成三条互不耦合的线（各自独立版本与节奏）：

| 产物 | tag | 工作流 | 产出 |
| --- | --- | --- | --- |
| CLI（`swarmhive`） | `cli/v*` | `cli-release.yml`（cargo-dist）+ `publish-crates.yml` | GitHub Release 多平台二进制 + npm wrapper + Homebrew + crates.io（`swarmhive-api-types` / `swarmhive-cli`） |
| SDK（`@swarm-hive/sdk`） | `sdk/v*` | `publish-sdk.yml` | npm 发包 |
| **server** | `server/v*` | `server-release.yml` | **GHCR 多架构镜像** `ghcr.io/swarm-apps/swarmhive-server`（`linux/amd64` + `linux/arm64`）+ **GitHub Release** 上 `x86_64` / `aarch64-unknown-linux-gnu` 单文件二进制 |

server 的镜像与二进制都用 `--features embed-spa` 把 admin SPA 经 `rust-embed` 内嵌进二进制
（构建前先 `pnpm admin:build`），所以一份镜像 / 一个二进制即同时服务 `/api` 与 admin 后台。
server **不**走 cargo-dist（在 `dist-workspace.toml` 里显式 `dist = false`），与 CLI 发布解耦。
镜像 tag：`server/v1.2.3` → `1.2.3` + `1.2` + `latest` + `sha-<short>`。

自托管部署形态与 `docker compose` 示例见 [自托管 Server](../apps/docs/content/docs/self-host/index.mdx)
与仓库 `deploy/`。
