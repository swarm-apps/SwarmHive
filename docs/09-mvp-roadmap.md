# MVP 路线图

## 阶段 0：项目骨架

目标：建立可运行的基础项目。

任务：

- Rust workspace（edition 2024、MSRV 1.90）。
- Axum server。
- PostgreSQL 数据库（dev 接 coolify 实例；single-server 走 compose profile）。
- sea-orm 2.0 接入 + `schema-sync` 起步（阶段 1 后切到 `sea-orm-migration`）。
- 基础配置文件（figment：toml + env layered）。
- 健康检查接口。
- 简单 Admin 前端骨架。

验收：

- 本地能启动 server。
- 能访问健康检查。
- 能连接到 Postgres 并完成首次 schema 同步。

## 阶段 1：核心模型与管理 API

目标：能管理应用、版本和产物 metadata。

任务：

**已完成（add-persistence-foundation, 2026-05-25）**：

- ☑ Organization 模型（MVP 默认 `slug = "default"`）。
- ☑ User / IdentityLink / Role / Permission / RolePermission / UserRole 模型（含 5 角色 + 21 permission seed）。
- ☑ Session 模型。
- ☑ AuditLog 模型（JSONB metadata）。
- ☑ sea-orm 2.0 `#[sea_orm::model]` 新格式 + schema-sync `get_schema_registry(...).sync(db)`。

**待落地**：

- App 模型。
- Release 模型。
- Artifact 模型。
- Channel 模型。
- StorageBackend 模型。
- UpdateEvent 模型。
- ProviderConfig 模型。
- API Token 模型（scoped token）。
- 管理 API。

验收：

- 能创建应用。
- 能创建版本。
- 能登记产物。
- 能记录基础更新事件。

## 阶段 2：RBAC 与鉴权

目标：MVP 即具备单组织 RBAC 和 scoped API Token。

任务：

- ✅ 登录 / 会话鉴权（`add-auth-and-rbac`: argon2id + tower-sessions + SeaOrmStore + Principal extractor）
- ✅ 默认 Owner 初始化（`add-auth-and-rbac`: 启动期一次性 setup_token，stdout banner，`POST /api/v1/setup`）
- ✅ 角色与权限初始化（`add-persistence-foundation` seed: 5 role + 21 permission + role_permission 关联）
- ✅ 权限 middleware（`add-auth-and-rbac`: `require_permission!` 宏 + `Scope::None | App(uuid)`）
- ⏳ app-level role 绑定（schema 已就位 `user_role.scope_app_id`；admin UI 绑定流程留给 `add-app-release-artifact`）
- ⏳ scoped API Token（`add-pat-and-api-token`）
- ✅ 关键操作审计日志基础设施（`add-auth-and-rbac`: `services/audit::write` + auth:login_succeeded / auth:login_failed / auth:owner_created）

验收：

- ✅ 一次性 setup_token 流程能引导出 Owner（集成测试覆盖）
- ✅ API endpoint 能按 permission 拦截（`/api/v1/_demo/release-publish` stub + 集成测试覆盖 Viewer 403）
- ⏳ Owner 能在 Admin UI 创建用户并分配角色（依赖 Admin UI Phase，本 stage 只把后端基础设施备好）
- ⏳ CI token 能限制到 app / channel / permission（`add-pat-and-api-token`）
- ✅ storage / token / release 敏感操作有审计日志（基础设施完成；具体 action 等业务 endpoint 落地时调 `audit::write`）

## 阶段 3：S3-compatible 存储后端

目标：所有产物统一保存到 S3-compatible object storage。

任务：

- Storage trait。
- S3-compatible storage 实现。
- RustFS 配置示例。
- 阿里云 OSS 配置示例。
- 上传 API。
- 下载重定向 API。
- test upload / test download。

验收：

- 能上传文件到 S3-compatible backend。
- 能使用 bundled RustFS 完成上传和下载 URL 生成。
- 能使用阿里云 OSS 完成上传和下载 URL 生成。
- 能通过统一下载 URL 获取文件。

## 阶段 4：存储初始化向导

目标：用户启动服务后，可以在后台完成 storage 配置。

任务：

- Admin Setup Wizard。
- Existing S3-compatible 表单。
- Aliyun OSS preset。
- Single-server RustFS 选项。
- 展示 Docker Compose profile / CLI 命令。
- endpoint health check。
- bucket check / create。

验收：

- 未配置 storage 时进入初始化向导。
- 能连接已有 S3-compatible storage。
- 能按指引启动 RustFS 并检测健康状态。
- 能保存 StorageBackend 配置。

## 阶段 5：CLI 本地发布

目标：不用 Web Admin，也不用 CI/CD，开发者本地即可发布产物。

任务：

- `swarmhive login`。
- `swarmhive init`。
- `swarmhive storage init rustfs`。
- `swarmhive verify tauri`。
- `swarmhive verify android`。
- `swarmhive publish tauri`。
- `swarmhive publish android`。
- 上传进度条。
- dry-run。

验收：

- 本地能发布 Tauri 产物。
- 本地能发布 Android APK。
- CLI 能复用 `swarmhive.toml` 默认配置。
- CLI 能输出发布结果和更新 endpoint。
- CLI 能输出 single-server RustFS 部署指引。

## 阶段 6：Tauri 更新链路

目标：SwarmDrop / SwarmNote 可接入。

任务：

- Tauri updater endpoint。
- latest.json 兼容响应。
- target / arch 匹配。
- signature metadata。
- 强制更新扩展字段。
- `update_check` / `update_available` 记录。

验收：

- Tauri 客户端能检查到更新。
- Tauri updater 能下载并安装。
- 后台能看到 Tauri 更新检查量。

## 阶段 7：React Native Android 链路

目标：SwarmNote-RN Android 可接入。

任务：

- APK 更新检查 API。
- versionCode 判断。
- RN SDK 初版。
- 下载进度与安装器跳转。
- 稍后提醒缓存。
- SDK 下载结果上报。

验收：

- RN 客户端能检查到 APK 更新。
- 能下载 APK 并跳转安装。
- 后台能看到 RN 下载开始和下载结果。

## 阶段 8：CI/CD

目标：发布流程自动化。

任务：

- GitHub Action 初版。
- workflow 示例。
- changelog 注入。
- CI 参数映射到 CLI。

验收：

- GitHub Actions 能自动发布 Tauri 版本。
- GitHub Actions 能自动发布 Android APK。

## 阶段 9：Admin、统计与埋点

目标：可视化管理与基础更新链路观测。

任务：

- 应用列表。
- 版本列表。
- 产物列表。
- 存储配置页面。
- 下载量 dashboard。
- 更新检查量 dashboard。
- 更新漏斗页面。

验收：

- 后台能看到应用、版本、产物。
- 后台能看到基础下载量。
- 后台能看到更新检查、发现更新、下载入口请求等事件。

## 阶段 10：OTA Provider 探索

目标：在不破坏主线定位的前提下，接入现有开源 OTA 生态。

任务：

- 定义 provider interface。
- 调研 Expo Updates provider。
- 调研 CodePush-compatible provider。
- 设计 OTA release 与 native release 的关系。
- 后台展示 OTA bundle、runtime version、rollout。

验收：

- 能明确 OTA provider 的边界。
- 能选择一个 provider 做 PoC。
- 不影响 Tauri / Android APK 更新主链路。

## MVP 之后

- Postgres 支持。
- 下载成功/失败上报完善。
- 安装后启动确认。
- 灰度发布。
- promote / rollback 完整后台化。
- 更多 S3-compatible 配置示例。
- SDK UI 完整组件集与主题预设。
- OTA provider 正式实现。
