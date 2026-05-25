# add-update-check-rn-android

## Why

docs/04 把 React Native Android 列为 MVP 第二条主链路。SwarmNote-RN 等产品需要稳定 APK 更新 endpoint + 强制更新策略。

## What

### 1. endpoint

```
GET /api/v1/updates/android/:app_slug
    Query: current_version_code, current_version_name, channel?, abi?
```

返回：

```json
{
  "has_update": true,
  "version_name": "0.2.1",
  "version_code": 21,
  "upgrade_type": "prompt",
  "min_version_code": 18,
  "download_url": "https://updates.example.com/download/swarmnote-rn/0.2.1/<artifact_id>",
  "release_notes": "...",
  "size_bytes": 52428800,
  "sha256": "..."
}
```

### 2. ABI 匹配

`arm64-v8a` 优先；`armeabi-v7a`、`x86_64` fallback；多 ABI APK（fat APK）应被认为兼容所有 ABI。

### 3. SDK 占位

SDK 实现拆到 `packages/sdk-core` + `packages/react-native`（不在 server 本 proposal 范围）。本 proposal 只保证 server 响应 schema 稳定，并附一份 fixture JSON 让 RN SDK contract test 锚定。

### 4. 埋点

`update_check` / `update_available`（同 Tauri，由 telemetry proposal 落聚合）。

## Acceptance

- POST 一个 Android release + APK，调 check endpoint 拿到正确响应。
- 客户端 `version_code < min_version_code` → `upgrade_type=force`。
- 三种 ABI 各自能拿到正确 APK download_url（mock SDK 调用）。
- `has_update=false` 路径不返回 download_url（避免 SDK 误下载）。
- 集成测试：发 APK → /api/v1/updates/android 返回完整 schema → /download 302 到 S3。

## Non-goals

- 不实现 RN SDK 自身（拆 `packages/*` 那条线）。
- 不实现 Expo OTA / CodePush（OTA 是 provider 扩展层，phase 2）。
- 不绕过 Android 安装确认（SDK 仍要跳转 PackageInstaller）。

## Depends on

- `add-storage-and-presign-upload`

## Maps to docs

- [docs/04-platform-support.md](../../../docs/04-platform-support.md) RN Android 段。
- [docs/03-architecture.md](../../../docs/03-architecture.md) 更新检查流程 / RN Android。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 7。
