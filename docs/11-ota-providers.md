# OTA Provider 设计

## 定位

SwarmHive 可以支持 OTA，但 OTA 不应该抢第一阶段主线。

SwarmHive Core 的核心定位是应用更新发布控制面：应用、版本、channel、storage、CI/CD、Admin、analytics。OTA 是 Update Provider 的一种，可以接入现有开源生态，而不是一开始从零实现所有协议。

## 设计目标

- 复用 SwarmHive 的应用、channel、storage、CI/CD、Admin 和 telemetry。
- Provider 负责具体 OTA 协议细节。
- 避免把 SwarmHive 直接定位成 CodePush 或 Expo Updates 的竞争者。
- 允许用户在同一个应用下同时管理 native package release 和 OTA bundle release。

## Provider 类型

### Expo Updates Provider

适合 Expo / React Native 项目。

可能集成方向：

- 参考或封装 Expo Updates 协议实现。
- 与现有开源项目协作或集成。
- 复用 S3-compatible storage。
- 管理 runtime version、branch、channel。

优点：

- Expo 生态较新。
- 协议明确。
- 适合已经使用 expo-updates 的项目。

限制：

- Bare RN 可能需要引入 expo-updates。
- 与 APK 更新链路不同，需要单独处理 runtime version。

### CodePush-compatible Provider

适合已有 CodePush 使用者迁移。

可能集成方向：

- 接入 Microsoft 独立 CodePush server。
- 接入其他开源 CodePush-compatible server。
- SwarmHive 只作为控制面和统计层。

优点：

- RN 老生态熟悉。
- 对 bare RN 更友好。
- App Center 退休后有迁移需求。

限制：

- 协议和客户端生态相对旧。
- 差分包、签名、rollout 等细节复杂。

### External OTA Sync Provider

SwarmHive 不直接承载 OTA 协议，而是同步 metadata 到外部 OTA server。

适合：

- 用户已经部署了 Expo OTA 或 CodePush server。
- SwarmHive 只统一 Admin、CI/CD、storage 和 analytics。

## 数据模型建议

OTA release 与 native release 分开建模，但可以关联。

核心字段：

- app_slug。
- provider：expo / codepush / external。
- runtime_version。
- native_version_range。
- bundle_version。
- channel。
- rollout_percent。
- storage_artifacts。
- release_notes。

## CLI 示例

```bash
swarmhive ota publish \
  --app swarmnote-rn \
  --provider expo \
  --runtime-version 1.0.0 \
  --channel stable \
  --bundle ./dist/expo-update
```

```bash
swarmhive ota promote \
  --app swarmnote-rn \
  --provider codepush \
  --release 2026.05.21-1 \
  --from beta \
  --to stable
```

## Admin 展示

后台应能区分：

- Native Release：APK / Tauri 安装包。
- OTA Release：JS bundle / assets / diff package。

OTA 页面展示：

- runtime version。
- native version range。
- channel。
- rollout。
- bundle size。
- download count。
- install / apply telemetry。

## 策略建议

第一阶段不实现 OTA provider，只预留架构和文档。

Phase 2 可以选择一个 provider 做 PoC：

1. 如果 SwarmNote-RN 继续基于 Expo 能力，优先 Expo Updates。
2. 如果目标是 bare RN / CodePush 迁移用户，优先 CodePush-compatible。
3. 如果想最稳，先做 External OTA Sync Provider，只集成现有开源 server。

## 非目标

- 不在 MVP 自研完整 OTA 协议。
- 不保证所有 RN 项目都能无改造接入 OTA。
- 不承诺 iOS 绕过 App Store 审核规则。
- 不让 OTA 改变 SwarmHive 第一阶段安装包更新主线。
