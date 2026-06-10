# 埋点与观测

> 2026-06-10 随 `add-telemetry-events` 落地重写,内容即实现实情(此前为设计稿)。

## 设计目标

埋点只服务**更新发布链路观测**,不做通用用户行为分析。它回答:

- 新版本发布后,多快覆盖设备群?(adoption 曲线 / 最新版 Active%)
- 这次更新有没有把用户搞挂?(下载/安装失败率)
- 下载链路是否健康?(check → 重定向 → 完成 → 启动 的漏斗)
- 哪些旧版本仍有大量设备滞留?(版本长尾,停支持决策)
- 灰度放量命中了多少设备?(`rollout_held` 维度)
- 还要不要给某个平台/架构打包?(platform / arch 分布)

## 隐私原则(实现即默认值)

- 数据只进入部署者自己的 SwarmHive 服务,不上传给任何第三方。
- **原始 IP 与 User-Agent 任何表都不存在对应列**(不是"可配置关闭",是列本身不存在);
  维度全部来自结构化请求参数(platform / target / arch / abi)。不做 IP→地理位置解析。
- 设备标识 `client_id` 是 SDK 在本地生成并持久化的**随机 UUID**(可重置、与账号无关、
  非硬件 ID)。GDPR 视角它属于假名化标识:部署者是 data controller,SwarmHive 提供
  上述合规默认值。
- 原始事件默认保留 90 天(`[telemetry] raw_retention_days`,`0` 关闭清理),
  聚合表永久——清理后 adoption 等核心指标不受影响。
- SDK 主动事件须提供 `telemetry: false` 配置,让 app 开发者向最终用户透传 opt-out;
  更新检查请求本身是功能必需,不属遥测范畴。

## 事件分层(两张表,信任边界分离)

### 服务端天然事件 → `update_event`(可信,server 自己写)

| event_name | result | 触发点 |
| --- | --- | --- |
| `update_check` | `up_to_date` / `available` / `rollout_held` | Tauri / RN Android 两条 check 路由的全部出口 |
| `download_intent` | `redirected` / `failed` | `/download/...` 公开下载入口 |

`update_available` 不是独立事件——并入 `update_check` 的 `result=available`;
`rollout_held`(灰度未命中)让"没更新"与"被灰度拦住"可区分,灰度观察由此成立。
写入是 **swallow 模式**:遥测故障只打 warn,绝不影响 check/download 响应。

### SDK 主动事件 → `client_event`(不可信,公开端点上报)

```
POST /api/v1/events        公开;限流;单条;写库失败仍返 200(fire-and-forget)
{ event, app, platform, client_id, channel?, target_version?, previous_version?,
  bytes_total?, duration_ms?, error_code?, error_message? }
```

事件白名单:`download_started` / `download_completed` / `download_failed` /
`install_started` / `install_failed`(RN 专属)/ `app_started_after_update`
(带 `previous_version` 升级归因,用于推断安装成功)。

SDK 接入契约(实现位于 `packages/sdk` 的后续 change):

- 事件发射点 = 状态机转移:`downloading→download_started`、`ready→download_completed`、
  下载中 `error→download_failed`、RN 调起安装器→`install_started`、
  启动时版本变化→`app_started_after_update`。Tauri 经 plugin-updater 同样拿得到
  下载进度,**不再**只限 `app_started_after_update`。
- 上报静默失败,不重试、不阻塞状态机。
- `client_id` 复用 `ensureClientId`(与灰度分桶同一标识),提供重置能力。

## 聚合与保留(rollup 双日表)

- `event_rollup_day`:`(app, day, source, event_name, result, version, platform, arch,
  channel) → count`。**可加**计数,漏斗/分布/趋势由它出。
- `device_rollup_day`:`(app, day, version) → unique_clients`(只从可信 `update_check`
  的 distinct client_id 统计;`version=NULL` 行是当日总活跃)。**不可加**的去重指标
  单独物化——不要对它的行做 SUM。
- server 内置周期任务:rollup 每小时重算「今天+昨天」UTC bucket(幂等);
  清理每天删过期 raw。先聚合后删序由"重算窗口只覆盖近两天"天然保证。

## Admin 统计页(`/telemetry`,需 `telemetry:read`)

指标卡(今日活跃 / 期内下载完成 / 最新版 Active%)+ 版本采用曲线 + 更新漏斗
(**按次计数**,口径在页面标注;设备去重版漏斗留后续)+ 平台/arch 分布 + 版本长尾。
查询端点:`GET /api/v1/telemetry/{summary,adoption,funnel,distribution}`,
全部只读 rollup 表。

## 已知边界

- 老 SDK 不传 `client_id` 时不计入设备数(adoption 在存量期内偏低,随升级自愈)。
- `client_event` 为自报数据,可被伪造——设备数等关键指标只从可信表统计,
  自报事件仅用于漏斗/失败率参考。
- "安装失败"无直接事件(进程死透报不出),由查询层推断:`download_completed`
  后该设备版本长期未变。
