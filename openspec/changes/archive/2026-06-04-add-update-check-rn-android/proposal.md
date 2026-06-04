# add-update-check-rn-android

## Why

docs/04 把 React Native Android 列为 MVP 第二条主链路（阶段 7）。SwarmNote-RN 等**侧载**（非 Google Play）的 Expo/RN 应用需要稳定的 APK 更新 endpoint + 强制更新策略。

调研确认（见 design.md）：Expo 生态把更新分成**两层**——`expo-updates`（OTA，只换 JS bundle，被 `runtimeVersion` 锁死在"原生不变"前提，**结构性绝不安装 APK**）与 native 二进制（`runtimeVersion`/fingerprint 变就必须发新 APK）。EAS Build 只给侧载用户一个手动下载页，**没有应用内"检查→下载→安装新 APK"闭环**。本 change 填的正是这个缺口的 **server 侧**：一个对齐 Tauri 端点形态、但用 `versionCode` 整数闸门 + ABI 匹配的 Android 更新检查 endpoint。

本 change 是 RN 主线三段（server endpoint → SDK android check → registry-rn UI）的**第一段、阻塞项**，给后续 RN SDK contract test 提供锚点。

## What Changes

### 1. endpoint

新增 `GET /api/v1/updates/android/:app_slug`，与现有 `tauri()` handler 并列在 `routes/updates.rs`（`router()` 改 `routes!(tauri, android)`），共享 `find_app_by_slug` / channel→pointer→release→`Published` 链 / `in_rollout_bucket` / `forwarded_ip` / `download_url` / telemetry helper。

Query：

```
current_version_code: i64   (必填, 整数; 解析失败 → 400)
current_version_name: String
channel: Option<String>     (显式不存在 → 404; 缺省走默认 channel)
abi: Option<String>         (arm64-v8a 优先)
client_id: Option<String>   (灰度分桶 key; 直连单机无 X-Forwarded-For 时唯一可靠 key)
runtime_version: Option<String>  (占位; MVP 不消费; 为未来 OTA 接缝/"runtimeVersion 错配 → native 强更信号"预留, 避免日后 breaking)
```

返回（扁平、`has_update` boolean、统一 200，不用 Tauri 的 204 absence 语义）：

```json
{
  "has_update": true,
  "version_name": "0.2.1",
  "version_code": 21,
  "upgrade_type": "prompt",
  "min_version_code": 18,
  "download_url": "https://.../download/<artifact_id>",
  "release_notes": "...",
  "size_bytes": 52428800,
  "sha256": "..."
}
```

`has_update:false` 时省略其余字段、**绝不返 download_url**。

### 2. 三处与 Tauri 必须分离的逻辑

- **版本闸门**：`current_version_code < release.android_version_code`（i64 整数比较），**绝不**复用 Tauri 的 `strip_v` + `semver::Version::parse`。`android_version_code` 为 None 的 release 跳过（非 RN release）。
- **artifact 匹配** `match_rn_artifact`：filter `platform == ReactNativeAndroid` → 精确 abi → fat APK（`abi=None`）→ 单 untargeted fallback；**允许跨 ABI 降级**（arm64 设备可跑 armeabi-v7a）；**不做** Tauri 的 signature gating（APK 真伪由 Android 安装器在安装时验签兜底）。
- **upgrade_type**：`android_min_version_code > current_version_code → Force`，否则 `Prompt`（整数比较）。

### 3. 数据模型 / migration

`release` 表新增 `android_min_version_code: Option<i64>`。现有 `min_version: Option<String>` 是 **semver** 语义（给 Tauri），不能给 RN 用整数下限；两条独立下限避免逼 handler 把 semver 字符串 parse 成 versionCode。（`android_version_code: Option<i64>` 已存在，CLI 在用，无需新增。）

### 4. 轻 OTA 接缝（server 侧，仅注释/占位，不建 OTA）

- `release.rs` doc 注释记约束：OTA bundle 的可下发性靠 `runtime_version`/fingerprint 精确匹配（不复用 `android_version_code` 整数闸门）；Phase 2 OTA 另立兼容键，不在本 change 建列。
- `match_rn_artifact` 注释写明"native-package 不做 signature gating 因 Android 安装器兜底；未来 ota-bundle kind 另需应用层验签"——把规则锚在 kind 级而非 platform 级。
- endpoint 留 `runtime_version?` query 占位（见上）。
- OTA 走未来独立的 `GET /api/v1/updates/ota/:app_slug`（另起 change），本端点保持纯 native，wire 不需 `update_kind` 字段（端点路径即 kind 判别）。

### 5. 埋点

`update_check` / `update_available`（同 Tauri，经 `tracing target:"telemetry"`；字段对齐 `add-telemetry-events` 的 `update_event`，`platform="react-native-android"`）。**不**为 OTA 加 `update_kind` 专列——未来走现有 `platform` 列 + `metadata_jsonb`（守"第二个 consumer 再抽象"）。

## Capabilities

### New Capabilities
- `update-check-rn-android`: RN Android 侧载 APK 更新检查 endpoint——`versionCode` 整数闸门、ABI 匹配（含 fat APK + 跨 ABI 降级）、扁平 `has_update` 响应、`android_min_version_code` 强制更新、复用 channel/rollout/telemetry、轻 OTA 接缝占位。

### Modified Capabilities
<!-- 无 spec 级修改：复用 update-check-tauri 的 helper 但不改其 requirement；app-release-artifact / storage-and-presign-upload 的 spec 不变。 -->

## Impact

- `crates/swarmhive-entity`: `release` 加 `android_min_version_code` 列 + schema-sync。
- `crates/swarmhive-api-types`: 新增 `AndroidUpdateResponse` + `AndroidUpdateQuery`（serde + `utoipa::ToSchema`）。
- `crates/swarmhive-server`: `routes/updates.rs` 加 `android` handler + `match_rn_artifact`；`router()` 注册。
- 集成测试 + fixture JSON（给 RN SDK contract test 锚定）。
- 不触碰 admin SPA / CLI（Android release/artifact 发布已由 add-app-release-artifact 覆盖）。

## Non-goals

- 不实现 RN SDK / rnAdapter / registry-rn（拆 `add-sdk-android-check` + `add-registry-rn` 两条后续 change）。本 change 只保证 server 响应 schema 稳定 + fixture JSON。
- 不实现 Expo OTA / 自托管 Expo Updates 协议（OTA 是 provider 扩展层，Phase 2，拆 `add-ota-expo-updates-server` 占位；docs/11 的两个 provider 候选保持开放，本 change 不预选）。
- 不绕过 Android 安装确认（安装跳转是 registry-rn 的 `PackageInstaller.Session` 职责，不在 server）。
- 不在本 change 建 OTA 兼容键列 / `update_kind` telemetry 列（仅注释占位）。

## Depends on

- `add-storage-and-presign-upload`（download 302 + artifact）。
- `add-app-release-artifact`（Release/Artifact/Channel 模型 + `android_version_code`）。

## Maps to docs

- [docs/04-platform-support.md](../../../docs/04-platform-support.md) RN Android 段。
- [docs/03-architecture.md](../../../docs/03-architecture.md) 更新检查流程 / RN Android。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 7。
- [docs/11-ota-providers.md](../../../docs/11-ota-providers.md) OTA 接缝（Phase 2，本 change 仅留接缝）。
