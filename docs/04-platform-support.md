# 平台支持

SwarmHive 第一阶段只支持 Tauri 和 React Native Android。这个限制是产品策略，不是技术能力不足。先把两个真实需求做深，再考虑扩平台。

## Tauri

### 支持范围

- Tauri v2 官方 updater 插件。
- 静态 `latest.json` 兼容输出。
- 动态 updater endpoint。
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

- upgrade_type：prompt / force / silent。
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

