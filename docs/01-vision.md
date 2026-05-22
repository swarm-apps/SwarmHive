# 愿景与定位

## 背景

SwarmDrop、SwarmNote 和 SwarmNote-RN 都需要自动更新能力。现状依赖 UpgradeLink 与 GitHub Releases：

- UpgradeLink 存在流量限制，关键链路不可控。
- 国内下载仍然慢，不能稳定解决用户体验问题。
- GitHub Releases 适合作为产物备份，但不适合作为国内主下载源。
- 强制更新、灰度、统计、埋点、发布自动化等能力分散在多个地方。
- 现有方案很难同时覆盖 Tauri 桌面端和 React Native Android。

用户已经具备自建能力：有服务器，也可以使用阿里云 OSS 或自托管 RustFS。SwarmHive 的首要目标就是把这些资源组织成稳定的自托管更新基础设施。

## 项目愿景

SwarmHive 希望成为面向 Tauri 与 React Native 开发者的开源自托管更新发布基础设施。

它提供：

- 可自托管的更新控制面。
- 统一的 S3-compatible 存储能力。
- 单服务器 bundled RustFS 部署模式。
- 面向国内分发的阿里云 OSS 配置路径。
- 可接入 CI/CD 的自动发布链路。
- 官方 SDK 与 UI 组件，降低客户端接入成本。
- 可视化后台，管理版本、策略、下载统计和更新链路埋点。

## 核心价值

SwarmHive 的价值不在于“再写一个 update endpoint”，而在于补齐完整发布链路：

- 对客户端：稳定检查更新，拿到明确策略和下载地址。
- 对开发者：构建完成后自动发布，不再手动同步平台。
- 对用户：下载速度更快，失败率更低。
- 对团队：能看到下载量、版本分布、失败率、升级漏斗和存储后端情况。

## 目标用户

- 使用 Tauri 开发桌面应用的独立开发者。
- 使用 React Native / Expo 发布 Android APK 的开发者。
- 开源项目维护者，希望提供稳定自动更新能力。
- 小团队和私有部署产品，需要内部应用分发。
- 国内用户较多、受 GitHub 下载速度影响明显的应用。

## 非目标

- 不替代 Tauri 自身签名验证，minisign 仍由客户端验证。
- 不绕过 Android 系统安装限制，APK 安装仍需要用户确认。
- 不做 iOS 自动安装更新。
- 第一阶段不支持 Electron、Flutter、原生桌面、命令行应用等更多被更新平台。
- 不提供 local filesystem 作为正式存储后端；单机本地场景通过 bundled RustFS 解决。
- 不做商业化托管平台优先，先做好开源自托管版本。
- 不做通用用户行为分析平台，埋点只围绕更新发布链路。

## 成功判断

第一阶段成功的标志：

- SwarmDrop / SwarmNote 桌面端能从 SwarmHive 检查和安装更新。
- SwarmNote-RN Android 能从 SwarmHive 检查 APK 更新并下载。
- 国内下载主链路能走 S3-compatible 对象存储，例如阿里云 OSS。
- 单服务器私有部署能通过 bundled RustFS 跑通完整链路。
- 后台能看到基础下载量、版本数据和更新漏斗。
- GitHub Actions 能自动发布版本到 SwarmHive。
