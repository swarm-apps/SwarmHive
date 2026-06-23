# SwarmHive 文档索引

本目录保存 SwarmHive 的早期产品与技术设计文档。文档按“为什么做、做什么、怎么做、先做什么”的顺序组织。

## 文档列表

1. [愿景与定位](01-vision.md)：项目背景、痛点、目标用户、非目标。
2. [产品需求](02-product-requirements.md)：核心场景、能力边界、MVP 验收标准。
3. [系统架构](03-architecture.md)：服务端、数据库、对象存储、客户端与 CI/CD 的关系。
4. [平台支持](04-platform-support.md)：Tauri 与 React Native Android 的接入边界。
5. [生态设计](05-ecosystem.md)：Server、CLI、SDK、Registry、Admin、CI/CD 的组合方式。
6. [CI/CD 设计](06-cicd.md)：GitHub Action、CLI 命令、发布、校验、回滚。
7. [存储与分发](07-storage-and-delivery.md)：S3-compatible、bundled RustFS、下载重定向与统计。
8. [后台与统计](08-admin-and-analytics.md)：后台页面、指标体系、权限与 API Token。
9. [MVP 路线图](09-mvp-roadmap.md)：阶段拆分、优先级、暂不做事项。
10. [埋点与观测](10-telemetry.md)：更新链路事件、漏斗指标、隐私边界。
11. [OTA Provider 设计](11-ota-providers.md)：Expo Updates、CodePush-compatible 等 OTA 接入边界。
12. [CLI 设计](12-cli.md)：本地发布、CI/CD 发布、校验、promote、rollback。
13. [RBAC 权限模型](13-rbac.md)：单组织、多用户、角色、权限、scoped API Token。
14. [SDK UI 设计](14-sdk-ui.md)：SDK（零 UI）与 shadcn registry 分发的 UI 组件、状态机、hooks、Tauri 与 RN 差异。
15. [通知系统](15-notifications.md)：发布列车事件、邮件/webhook 通道、Standard Webhooks 签名、重试与重投。

## 当前设计原则

- 先解决真实项目的迁移需求，再扩展成通用平台。
- 第一阶段只支持 Tauri 和 React Native Android 的安装包更新。
- OTA 做成 provider 扩展层，不抢 MVP 主线。
- SwarmHive 只维护一套 S3-compatible storage backend。
- 单服务器用户通过 bundled RustFS 模式获得本机对象存储，而不是直接使用 local filesystem backend。
- 阿里云 OSS 作为 S3-compatible 的重点国内云存储示例，不做云厂商专用主抽象。
- CLI 是一等入口，既服务本地手动发布，也服务 CI/CD 自动发布。
- CI/CD 是一等能力，不是后补脚本。
- 通知系统通过事务性 outbox 解耦发布路径与外发投递，webhook 遵循 Standard Webhooks。
- MVP 做单组织 + 完整 RBAC，不做真正多租户。
- 埋点只服务更新发布链路观测，不做通用用户行为分析。
