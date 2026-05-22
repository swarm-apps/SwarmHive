# SwarmHive

SwarmHive 是一个开源、自托管的应用更新发布中枢，第一阶段专注服务 **Tauri 桌面应用** 和 **React Native Android 应用**。

它统一管理版本、策略、产物、镜像、下载入口、统计数据、埋点事件和 CI/CD 发布链路，帮助独立开发者与小团队摆脱对第三方更新平台和慢速下载源的强依赖。

## 核心定位

SwarmHive 不是单一 update API，而是一套轻量应用发布基础设施：

- Server：版本、策略、产物、下载与更新链路观测。
- CLI：本地发布、CI/CD 发布、产物校验、promote 与 rollback 的统一入口。
- SDK：Tauri 与 React Native 客户端接入。
- CI/CD：从构建产物到发布版本的自动化流水线。
- Admin：应用、版本、渠道、下载量、埋点漏斗、RBAC 和存储配置管理。
- SDK + Registry：SDK 只暴露 API、状态机和 hooks（零 UI 依赖），UI 组件通过 shadcn registry 分发到用户项目源码，可任意定制。
- Storage：统一使用 S3-compatible storage；单服务器部署可通过 bundled RustFS 获得本机对象存储。
- OTA Providers：后续可集成 Expo Updates、CodePush-compatible 等开源 OTA 实现。

## 第一阶段支持

- Tauri v2 desktop updater control plane。
- React Native / Expo Android APK updater。
- S3-compatible object storage。
- bundled RustFS single-server mode。
- 阿里云 OSS、RustFS、MinIO、Garage、Cloudflare R2、AWS S3 等 S3-compatible 后端。
- 官方 CLI、GitHub Action 与基础 workflow 模板。
- 本地手动发布和 CI/CD 自动发布共用同一套 CLI。
- RBAC、scoped API Token、下载统计、更新链路埋点、强制更新、channel、版本范围策略。

## 文档

- [愿景与定位](docs/01-vision.md)
- [产品需求](docs/02-product-requirements.md)
- [系统架构](docs/03-architecture.md)
- [平台支持](docs/04-platform-support.md)
- [生态设计](docs/05-ecosystem.md)
- [CI/CD 设计](docs/06-cicd.md)
- [存储与分发](docs/07-storage-and-delivery.md)
- [后台与统计](docs/08-admin-and-analytics.md)
- [MVP 路线图](docs/09-mvp-roadmap.md)
- [埋点与观测](docs/10-telemetry.md)
- [OTA Provider 设计](docs/11-ota-providers.md)
- [CLI 设计](docs/12-cli.md)
- [RBAC 权限模型](docs/13-rbac.md)
- [SDK UI 设计](docs/14-sdk-ui.md)

## 一句话卖点

10 分钟自托管一个 Tauri/RN 更新中心，带 bundled RustFS / S3-compatible 存储、本地发布 CLI、下载统计、更新漏斗、强制更新和 GitHub Actions 发布。

## 与 Swarm 系列的关系

SwarmHive 会先服务 SwarmDrop、SwarmNote 和 SwarmNote-RN：

- 桌面端走 Tauri updater 兼容接口。
- Android 端走 React Native SDK。
- UpgradeLink 从主链路中移除。
- GitHub Releases 降级为构建产物备份源或 fallback 下载源。
- 国内下载可优先走阿里云 OSS 的 S3-compatible 接口。
- 单服务器私有部署可使用 SwarmHive 官方 bundled RustFS 模式。

