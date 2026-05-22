# 后台与统计

## Admin 目标

SwarmHive Admin 用于替代第三方更新平台控制台，让开发者能直观看到版本、策略、产物、下载量、更新漏斗和存储状态。

后台第一阶段不需要复杂，但必须覆盖发布与排障核心路径。尤其是首次启动后的存储初始化向导，这是单服务器用户能顺利使用 SwarmHive 的关键。

## 页面设计

### Setup Wizard

首次启动时，如果未配置 storage，进入初始化向导。

选项：

- Existing S3-compatible storage。
- Aliyun OSS preset。
- Single-server RustFS。

能力：

- 展示 RustFS 官方 Docker Compose profile 或 CLI 命令。
- 检测 RustFS / S3 endpoint 健康状态。
- 测试 AK/SK。
- 检查或创建 bucket。
- 测试上传和下载。
- 保存 StorageBackend 配置。

### Dashboard

展示：

- 总应用数。
- 当前发布版本数。
- 今日下载量。
- 今日更新检查量。
- 近 7 天下载趋势。
- 更新漏斗概览。
- 下载失败率。
- 存储后端状态。

### Apps

展示应用列表：

- 应用名称。
- slug。
- 支持平台。
- 默认 channel。
- 最新 stable 版本。
- 总下载量。
- 更新检查量。

### Releases

展示某个应用的版本列表：

- 版本号。
- channel。
- 状态。
- 发布时间。
- 更新策略。
- 下载量。
- 更新检查量。
- 产物完整度。

### Artifacts

展示产物：

- 平台。
- target / arch / ABI。
- 文件名。
- 文件大小。
- 存储后端。
- 签名状态。
- 下载地址。

### Policies

配置：

- 可选更新。
- 强制更新。
- 最低可用版本。
- 灰度比例。
- channel 指向。

### Storage

配置：

- S3-compatible endpoint。
- bucket。
- region。
- force path style。
- public base URL。
- signed URL TTL。
- 当前模式：existing S3 / Aliyun OSS / bundled RustFS。
- 连通性测试。
- test upload / test download。

### Telemetry

展示更新链路事件：

- 检查更新次数。
- 发现更新次数。
- 下载入口请求次数。
- 下载重定向次数。
- SDK 上报的下载成功 / 失败。
- 新版本启动回传次数。

### Users & Roles

管理：

- 用户列表。
- 邀请用户。
- 分配角色。
- app-level role 绑定。
- 禁用用户。

### API Tokens

管理：

- CI/CD Token。
- 只读客户端 key。
- token 权限范围。
- app scope。
- channel scope。
- 过期时间。
- 撤销 token。

## 统计指标

MVP 指标：

- 总下载量。
- 更新检查量。
- 有更新响应量。
- 按应用下载量。
- 按版本下载量。
- 按平台下载量。
- 按天下载趋势。

后续指标：

- 下载失败率。
- 当前活跃版本分布。
- 更新转化率。
- 镜像命中率。
- 地区分布。
- 安装后启动确认率。

## 数据保留

MVP 可直接保存原始事件。

后续可增加：

- 按小时聚合表。
- 按天聚合表。
- 原始事件定期清理。

## 权限策略

MVP 做单组织 + 完整 RBAC，不做真正多租户。

基础角色：

- Owner：管理用户、角色、存储、token 和所有应用。
- Admin：管理应用、版本和策略。
- Release Manager：发布、promote、rollback、yank。
- Developer：上传 draft / beta 产物。
- Viewer：只读查看版本、下载量和埋点。

权限应以 permission 为准，角色只是 permission 集合。

重点权限：

- `storage:manage`。
- `token:manage`。
- `release:publish`。
- `release:promote`。
- `release:rollback`。
- `release:yank`。
- `artifact:upload`。
- `analytics:read`。
- `telemetry:read`。

关键操作需要写入审计日志。

