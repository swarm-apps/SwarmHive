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
app = "swarmdrop"
default_channel = "stable"

[tauri]
artifact_dir = "src-tauri/target/release/bundle"

[android]
apk = "android/app/build/outputs/apk/release/app-release.apk"
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
