# 产品需求

## 产品目标

SwarmHive 第一阶段要解决六件事：

1. 让 Tauri 应用拥有自托管更新控制面。
2. 让 React Native Android 应用拥有自托管 APK 更新能力。
3. 统一通过 S3-compatible storage 管理发布产物。
4. 让没有对象存储经验的单服务器用户可以通过 bundled RustFS 模式完成部署。
5. 让开发者既可以通过 CLI 本地上传构建产物，也可以通过 CI/CD 自动发布。
6. 让开发者看到更新检查、下载、安装启动等关键链路数据。
7. 从 MVP 起提供单组织 RBAC 和 scoped API Token，保护发布、存储、强制更新等敏感操作。

## 核心用户故事

### 独立开发者

作为 Tauri 应用开发者，我希望在本地构建完成后，可以直接运行 CLI 上传产物并发布版本，而不必打开后台手动点上传。

### 单服务器部署者

作为只拥有一台服务器的用户，我希望启动 SwarmHive 后，可以在后台初始化向导中选择“bundled RustFS”，让系统引导我在同一台服务器上启动本机 S3-compatible 存储。

### CI/CD 使用者

作为维护者，我希望在发布 GitHub tag 后，CI 自动构建产物并调用同一套 CLI / GitHub Action 同步到 SwarmHive，使旧版本客户端可以检查到新版本。

### React Native 开发者

作为 RN Android 应用开发者，我希望发布 APK 后，客户端能检查版本、展示更新弹窗、下载 APK 并跳转系统安装器。

### 项目维护者

作为开源项目维护者，我希望下载流量可以走阿里云 OSS、RustFS、R2 等 S3-compatible 存储，后台能看到每个版本的下载量、失败率和升级漏斗。

### 运维/发布者

作为发布者，我希望可以将 beta 版本提升为 stable，也可以把 stable 回滚到上一个版本。

### OTA 使用者

作为 React Native 开发者，我希望未来可以把 Expo Updates 或 CodePush-compatible OTA 接入 SwarmHive 的统一后台、存储、CI/CD 和统计中，而不是再维护一套孤立控制台。

## 功能需求

### 应用管理

- 创建和管理多个应用。
- 每个应用可配置名称、slug、平台类型、默认 channel。
- 每个应用可以绑定 scoped API Token。
- MVP 采用单组织模型，但核心表预留 `org_id`。

### 版本管理

- 创建版本记录。
- 配置版本号、更新日志、发布时间、channel。
- 支持最低可用版本或版本范围强制更新。
- 支持版本状态：draft、published、yanked。

### 产物管理

- Tauri：管理安装包、updater artifact、签名、target、arch。
- React Native：管理 APK、versionName、versionCode、ABI。
- 产物统一保存到 S3-compatible storage。
- 产物可由 Web Admin、CLI 或 CI/CD 上传；MVP 优先实现 CLI / CI/CD，Web 上传可以后置。

### RBAC 与权限

MVP 即支持单组织 RBAC。

基础角色：

- Owner：系统所有者，管理用户、存储、token、所有应用。
- Admin：管理应用、版本、策略，但不能管理 Owner 和系统级敏感设置。
- Release Manager：发布、promote、rollback、yank。
- Developer：上传 draft / beta 产物，不能发布 stable。
- Viewer：只读查看版本、下载量、埋点。

API Token 不等同用户角色，应支持 app / channel / permission scope，例如：

```text
app = swarmdrop
channel = beta
permissions = artifact:upload, release:publish
```

敏感权限：

- `storage:manage`：管理 S3 / RustFS / OSS 配置。
- `token:manage`：创建和撤销 API Token。
- `release:publish`：发布版本。
- `release:promote`：提升 channel。
- `release:rollback`：回滚 channel。
- `release:yank`：撤回版本。
- `analytics:read` / `telemetry:read`：查看统计和埋点。

### 存储初始化

- 首次启动后，后台应提供存储初始化向导。
- 用户可以选择已有 S3-compatible storage，填写 endpoint、bucket、region、AK/SK。
- 用户可以选择阿里云 OSS 预设表单，底层仍走 S3-compatible backend。
- 用户可以选择 single-server RustFS 模式，由 SwarmHive 提供 Docker Compose profile 或 CLI 命令引导启动 RustFS。
- SwarmHive 不在 Web 后台默认执行任意 Docker 命令；后台可以展示命令、检测健康状态、测试上传和下载。

### CLI 本地发布

CLI 是开发者本地发布与 CI/CD 发布的统一入口。

MVP 需要支持：

- 登录或配置 server / token。
- 初始化 `swarmhive.toml`。
- 扫描 Tauri 构建目录。
- 扫描 Android APK。
- 上传产物。
- 显示上传进度。
- 支持 dry-run。
- 支持 `swarmhive storage init rustfs` 输出或执行官方 bundled RustFS 部署指引。
- 发布前校验版本号、签名、平台和重复发布。

### 更新检查

- Tauri 支持 `latest.json` 兼容格式和动态 endpoint。
- React Native Android 支持自定义 JSON 响应。
- 更新响应包含版本、策略、下载 URL、更新日志、强制更新信息。

### 下载分发

- 下载入口由 SwarmHive 统一生成。
- S3-compatible backend 返回公开 URL 或短期签名 URL。
- bundled RustFS 也通过同一套 S3-compatible backend 访问。
- 下载事件应记录应用、版本、平台、架构、channel、存储后端。

### 更新链路埋点

埋点只服务自动更新链路，不做通用用户行为分析。

MVP 必做服务端事件：

- `update_check`：客户端检查更新。
- `update_available`：服务端判断存在可用更新。
- `download_intent`：客户端请求统一下载入口。
- `download_redirected`：服务端已返回 S3-compatible 下载地址。

SDK 预留事件：

- `download_started`。
- `download_completed`。
- `download_failed`。
- `install_started`。
- `install_failed`。
- `app_started_after_update`。

### 下载统计

- 记录总下载量。
- 记录按版本、平台、架构、时间分布的数据。
- 记录下载失败与镜像命中情况。
- 记录升级漏斗转化率。

### CI/CD

- 提供 CLI 与 GitHub Action。
- 支持发布前校验。
- 支持发布、提升 channel、回滚。
- 支持注入 changelog 和更新策略。

### OTA Providers

OTA 不进入第一阶段核心闭环，但架构需要预留 provider 扩展点。

后续可支持：

- Expo Updates provider。
- CodePush-compatible provider。
- 第三方 OTA server 的控制面集成。

SwarmHive 对 OTA 的定位是统一管理 metadata、channel、storage、CI/CD 和 analytics，尽量复用现有开源 OTA 协议实现，而不是一开始从零重写 OTA 协议。

### SDK

- SDK 提供 API 客户端、状态机和 React hooks（`useUpdate()`），零 UI 依赖。
- `@swarm-hive/sdk-core` 是框架无关核心；`@swarm-hive/sdk-core/react` 子入口提供 hooks。
- `@swarm-hive/tauri` 与 `@swarm-hive/react-native` 是平台适配层。

### Registry

- UI 组件通过 shadcn registry 分发到用户项目，源码进入用户代码库可任意定制。
- 提供 registry-web（Tailwind + Radix）与 registry-rn（NativeWind + @rn-primitives）两套。
- 组件依赖 SDK 提供的状态机和 hooks，自身保持薄壳。
- 支持强制更新、可选更新、下载进度、错误重试。
- 文案通过 prop 注入，默认提供 en / zh-CN。

## MVP 验收标准

- 能创建应用和版本。
- 能通过 CLI 本地发布 Tauri 产物。
- 能通过 CLI 本地发布 Android APK。
- 能上传 Tauri 产物并返回 Tauri updater 兼容响应。
- 能上传 Android APK 并返回 RN SDK 可消费的响应。
- 能使用任意 S3-compatible storage 保存产物。
- 能使用阿里云 OSS 作为 S3-compatible 后端。
- 能通过 bundled RustFS single-server 模式跑通上传和下载。
- 能通过 GitHub Action 自动发布一个版本。
- 能在后台看到版本列表、基础下载量和更新检查量。
- 能配置强制更新。
- 能创建用户并分配角色。
- 能创建 scoped API Token，并限制到 app / channel / permission。

## 暂不做

- 真正多租户和多组织切换。
- 多租户计费。
- iOS 自动更新。
- OTA 协议完整自研。
- local filesystem storage backend。
- 多云厂商专用适配矩阵。
- 全球 CDN 智能调度。
- 复杂 A/B 实验。
- 通用用户行为分析。
