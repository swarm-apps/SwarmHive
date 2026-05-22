# 埋点与观测

## 设计目标

SwarmHive 需要埋点，但埋点只服务更新发布链路观测，不做通用用户行为分析。

它要回答的问题是：

- 有多少客户端检查过更新？
- 有多少客户端发现了新版本？
- 有多少用户开始下载？
- 下载是否成功？
- 安装后是否真的启动了新版本？
- 哪些旧版本仍有大量用户滞留？
- 哪个存储后端或镜像失败率更高？

## 隐私原则

- 数据只进入部署者自己的 SwarmHive 服务，不默认上传给 SwarmHive 官方。
- 不强制采集真实设备 ID。
- MVP 使用匿名 installation id 或随机 client id。
- IP、User-Agent 等敏感字段可配置是否保存。
- 默认只采集更新链路事件。

## 事件分层

### 服务端天然事件

这些事件不依赖 SDK 主动上报，MVP 必做：

- `update_check`：客户端检查更新。
- `update_available`：服务端判断存在可用更新。
- `download_intent`：客户端请求统一下载入口。
- `download_redirected`：服务端返回 S3-compatible 下载地址。
- `download_redirect_failed`：服务端生成下载地址失败。

### SDK 主动事件

这些事件更接近升级漏斗，MVP 可先预留接口，RN SDK 优先实现：

- `download_started`。
- `download_completed`。
- `download_failed`。
- `install_started`。
- `install_failed`。
- `app_started_after_update`。

## 平台差异

### Tauri

Tauri updater 负责下载、验证和安装。SwarmHive 不一定能直接确认安装成功。

推荐方式：

- 检查更新时记录 `update_check`。
- 统一下载入口记录 `download_intent`。
- 新版本启动后由 SDK 上报 `app_started_after_update`，用来推断安装成功。

### React Native Android

RN SDK 控制 APK 下载，因此可以更准确记录：

- 下载开始。
- 下载完成。
- 下载失败。
- 跳转系统安装器。
- 新版本启动。

Android 仍无法强制确认用户是否最终点击安装成功，但新版本启动事件可以作为结果确认。

## 事件字段

建议基础字段：

- event_name。
- app_slug。
- release_version。
- current_version。
- platform。
- target / arch / abi。
- channel。
- artifact_id。
- storage_backend。
- anonymous_client_id。
- created_at。

可选字段：

- country / region。
- user_agent。
- error_code。
- error_message。
- duration_ms。
- bytes_total。
- bytes_downloaded。

## 指标

MVP 指标：

- update checks。
- update available count。
- download intents。
- download redirects。
- downloads by version。
- downloads by platform。

后续指标：

- download success rate。
- install confirmation rate。
- update funnel conversion。
- old version retention。
- storage backend failure rate。
- mirror hit ratio。

## 数据保留策略

MVP：

- 保存原始事件。
- 后台直接按时间范围查询。

后续：

- 原始事件保留 30 到 90 天。
- 长期保存按小时 / 天聚合数据。
- 支持关闭部分可选字段采集。

## API 草案

```text
POST /api/v1/events
```

客户端上报：

```json
{
  "event": "download_completed",
  "app": "swarmnote-rn",
  "current_version": "0.2.0",
  "target_version": "0.2.1",
  "platform": "android",
  "channel": "stable",
  "anonymous_client_id": "...",
  "bytes_total": 52428800,
  "duration_ms": 120000
}
```

服务端应允许事件上报失败不影响更新主流程。

