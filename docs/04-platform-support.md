# 平台支持

SwarmHive 第一阶段只支持 Tauri 和 React Native Android。这个限制是产品策略，不是技术能力不足。先把两个真实需求做深，再考虑扩平台。

## Tauri

### 支持范围

- Tauri v2 官方 updater 插件。
- 动态 updater endpoint：**已实现** `GET /api/v1/updates/tauri/:app_slug`（dynamic flat JSON / 204，见 `add-update-check-tauri`）。
- 静态 `latest.json` 兼容输出：roadmap，MVP 未实现（动态 endpoint 已覆盖）。
- Windows、macOS、Linux。
- target / arch 维度产物匹配。
- minisign signature metadata 管理。

### 响应能力

Tauri 更新响应需要包含：

- version。
- pub_date。
- url。
- signature。
- notes。

SwarmHive 可额外保存扩展字段，例如：

- upgrade_type：prompt / force（`silent` 为 roadmap，MVP 仅 prompt / force）。
- min_version。
- channel。
- rollout_percent。

扩展字段用于 SDK 或业务 UI 判断，不破坏 Tauri updater 兼容性。

### 客户端边界

SwarmHive 不绕过 Tauri updater 的安全机制。客户端下载后仍由 Tauri updater 验证签名和执行安装。

## React Native Android

### 支持范围

- React Native Android。
- Expo Android APK 分发。
- APK versionName / versionCode 判断。
- 强制更新、可选更新、稍后提醒。
- 下载进度与系统安装器跳转。

### 响应能力

RN SDK 需要消费：

- has_update。
- version_name。
- version_code。
- upgrade_type。
- download_url。
- release_notes。
- min_version_code。

### 客户端边界

Android 不允许普通第三方应用静默安装 APK。SwarmHive SDK 只能下载 APK 并调起系统 PackageInstaller，最终安装仍由用户确认。

### 发布约束与踩坑

- **发 APK，不发 AAB**：SwarmHive 侧载分发分发的是 **APK**（`gradle assembleRelease` / `eas build --platform android` 产出 APK）。AAB（Android App Bundle）只能走 Google Play，SwarmHive 不处理。CLI / 文档引导用户上传 APK。
- **per-ABI split APK 的 versionCode offset**：若按 ABI 分包（`armeabi-v7a` / `arm64-v8a` / `x86_64`），Google 推荐给每个 ABI 的 versionCode 加固定 offset（如 `+1`/`+2`/`+3`）以区分。这会影响 SwarmHive 端点的整数闸门比对——服务端按 artifact 行各自的 `android_version_code` 实际值判定，发布时需保证客户端自报的 `current_version_code` 与对应 ABI 包的 versionCode 同一套编号。优先发**单个 fat APK**（含全部 ABI）规避此坑。
- **keystore 漂移（高危）**：Android 安装器要求升级包与已装包**同一签名密钥**，否则 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`，老用户只能卸载重装、**丢数据**。签名私钥（keystore）全程在**开发者 / CI 侧**，绝不进 SwarmHive。换 keystore 必须走 v3 key rotation；EAS 远程凭据托管时注意别让 EAS 换签名。SwarmHive 只校验下载完整性（`sha256`），不参与签名——APK 真伪由 Android 安装器在安装时验 v2/v3 签名兜底，故 SwarmHive **不额外加 minisign**（与 Tauri 不同，Tauri 因桌面无 OS 级验真才需 minisign）。

## iOS

第一阶段不做 iOS 自动更新。

可保留两个扩展点：

- 返回 App Store / TestFlight 跳转 URL。
- 仅做版本提示，不做安装。

## 暂不支持平台

- Electron。
- Flutter。
- 原生桌面应用。
- 命令行应用作为被更新平台。
- Web 应用热更新。

这些平台只有在 Tauri 与 RN 两条链路稳定后再讨论。

