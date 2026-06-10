# design

## Context

阶段 9 收口。三处 server hook 已 ship(`routes/updates.rs` 两条 check 路由 + `routes/download.rs`
的 `tracing::info!(target:"telemetry")`,字段名当时就对齐了本 change);check 链路已收
`client_id`(Tauri `X-Client-Id` header / RN query,灰度分桶在用);SDK 8 态状态机
(`idle/checking/up-to-date/available/force-required/downloading/ready/error`)给主动事件
提供了天然发射点。核心决策:① 事件模型与表切分;② 去重指标怎么在 raw 清理后存活;
③ 隐私字段取舍;④ 上报通道;⑤ rollup/清理任务形态;⑥ Admin 呈现。

调研依据(2026-06-10 双路 web 调研,结论详见 explore 记录):

- **指标面**:CodePush(Active%/Total/Pending/Rollbacks/Rollout%)、EAS Update insights
  (uniqueUsers/failedInstalls/crashRate)、Sparkle System Profiling(每周限报一次防 check
  频率污染)、CrabNebula Cloud、Play Console(per-version active devices)。
  自托管竞品(hazel/nucleus/update.electronjs.org)零内置统计。
- **隐私面**:Aptabase/TelemetryDeck/Plausible/Umami 一致不落盘原始 IP/UA;
  日轮换 hash 路线(Plausible/Aptabase)官方承认做不了跨日 adoption/retention;
  TelemetryDeck/Homebrew 的设备持久标识路线支撑去重指标;Matomo 的
  「raw 短留存 + 先归档后删 + 聚合永久」是留存策略范本(CNIL 豁免上限 raw ≤25 个月)。

## Goals / Non-Goals

**Goals:**

- 三处既有 hook 落库(swallow 失败,主流程零影响)+ result 维度(灰度观察)
- 公开 `POST /api/v1/events` 收 SDK 6 种主动事件
- rollup 双日表:可加计数 + 不可加去重分离;raw 90 天清理后 adoption 永久可查
- Admin /telemetry 统计页(adoption / 漏斗 / 分布 / 长尾 / 失败率)

**Non-Goals:** 见 proposal(geo、用户级归因、SDK 实现、rollback 事件、check 捎带通道、实时推送)。

## Decisions

### 1. update_available 并入 update_check 的 result 维度

原 stub 把 `update_check` 与 `update_available` 记两行。合并为一行
`update_check + result ∈ {up_to_date, available, rollout_held}`:

- 行数减半;漏斗第一二级变成同一行的两种过滤(`count(*)` vs `count(result=available)`)
- **rollout_held 是灰度观察的钥匙**:原设计里"没更新"和"灰度未命中"混在
  check 与 available 的差值里不可分;现在桶内外失败率对比、放量命中数都能算
- hook 点改造:check 路由已在函数尾部分支(202/204/灰度 warn)处打 tracing,
  result 在那一刻已知,零额外计算

`download_intent` 同理带 `result ∈ {redirected, failed}`(吸收原 `download_redirected` /
`download_redirect_failed` 两个事件名)。

### 2. 可信/不可信两张事件表,而不是按"更新/下载"切

`update_event`(server 写)与 `client_event`(公开端点收)的信任边界完全不同:前者字段
都来自 server 自己的判定,后者任何字段都可能被伪造(只能靠限流 + 白名单 + 长度校验兜底,
查询时也应意识到它是自报数据)。物理分表让这个边界显式化,也避免一张表里
一半列只对一半行有意义。

### 3. rollup 双日表:可加计数与不可加去重分离(本 change 最重要的 schema 决策)

```text
event_rollup_day  (app_id, day, source, event_name, result?, version?, platform?, channel?) → count
                   纯计数;任意维度可 SUM;漏斗/分布/趋势全由它出
device_rollup_day (app_id, day, version?) → unique_clients
                   per-version 活跃设备(version=NULL 行 = 当日总活跃)
                   adoption 曲线 / Active% / DAU / 版本长尾由它出
```

**Why 拆**:`COUNT(DISTINCT client_id)` 不可加——`SUM(各版本 unique)` ≠ 总 unique,
把 unique 塞进全维度展开的计数表会诱导错误聚合。把去重指标限定在「(app, day, version)」
这一个业务上真正需要的粒度单独物化,语义干净且查询不会踩坑。
**Why 不用 HyperLogLog**:自托管单组织、设备量 1e4~1e6 级,Postgres 精确
`COUNT(DISTINCT)` 毫无压力;HLL 是过度工程。
**Why day 不是 hour**(stub 是 hour):dashboard 全部需求是天级;hour 桶行数 ×24 没有消费者。
**来源限定**:`device_rollup_day` 只从 `update_check` 事件算(每设备 ≥12h 一次,频率天然
归一,呼应 Sparkle 每周限报的动机)——不混入 client_event,防自报事件污染设备数。

### 4. 隐私字段:删 ip/user_agent 列,而非"可配置保存"

stub 是 `ip?`/`user_agent?` + `collect_ip` 开关。修订为**列不存在**:

- 四家隐私先锋(Aptabase/TelemetryDeck/Plausible/Umami)一致不落盘原始 IP;UA 通行做法
  是解析成枚举后丢弃——而我们的 check 链路已有结构化 `platform/target/arch/abi`,
  原始 UA 零增量价值
- 「没有这个列」比「有列 + 开关」省掉全部合规论证;未来真要地理分布,
  做法是内存 GeoIP 解析出 `country` 列、IP 仍即弃
- `client_id`(持久随机 UUID)按 GDPR 主流读法是假名化个人数据:docs 写明
  运营者是 data controller;SDK 契约要求提供 reset 能力;Breyer 案的相对论
  读法下自托管+无账号关联场景风险本就很低

### 5. 上报通道:单一 POST /api/v1/events

EAS 的「下次 check 捎带上次安装结果」模式经评估放弃(MVP):RN 全程 SDK 掌控,实时 POST
自然;Tauri 安装成功靠 `app_started_after_update`(SDK 启动时报,带 `previous_version`
归因);「安装失败」=「download_completed 后版本长期未变」属查询层推断,不值得为它
增加第二条采集通道。端点设计:

- 公开、挂 sensitive 子树(governor 限流);单条不批量(事件频率:每设备每次更新 ≤6 条)
- 校验:event 白名单(serde enum)、app slug 存在(404)、client_id 必填 ≤64 字符、
  error_message 服务端截断(512)、bytes/duration 为非负
- **写库失败 swallow,仍返 200**:遥测绝不让客户端重试风暴;失败率观测靠 server 日志

### 6. 落库 swallow 模式:services/telemetry.rs 复刻 audit::write_swallowing

check 是热路径(但每设备 12h 节流,QPS 低),同步 insert + swallow(`warn` 日志)是
audit 已验证的先例;不引入批量缓冲/channel(过早优化,且进程崩溃丢缓冲)。
tracing::info 预留点保留(双写):日志侧 `target:"telemetry"` 仍可被外部收集器消费。

### 7. rollup/清理任务:重算式幂等,免水位

server 内 tokio 周期任务(`services/telemetry.rs`,bin 启动时 spawn,与 mailer/storage
hot-swap 同级别的基础设施):

- **rollup(每小时)**:`DELETE+INSERT`(TX)重算「今天 + 昨天」两个 day bucket
  (昨天要重算:跨日时刻的迟到事件)。重算而非增量 = 天然幂等、无水位表、
  与 `db_smoke` 类测试好写。聚合 SQL 用 sea-orm 结构化 API(group by + count
  + count distinct);若表达力不够,这是 server 第三处「刻意 raw SQL」的合理候选
  (backend.md 记录)。
- **清理(每天)**:`DELETE WHERE created_at < now() - retention`;`raw_retention_days = 0`
  禁用。顺序安全性:rollup 持续覆盖近两天,90 天前的 bucket 早已固化,删 raw 不丢聚合
  (Matomo「先归档后删」的约束在本设计里由时间差天然满足)。
- 任务退避:失败 warn + 等下个周期,不重试风暴;启动即跑一次(部署后立即有数据)。

### 8. Admin 呈现:顶层「统计」菜单 + 4 个查询端点

- 路由 `_auth/telemetry.tsx`,菜单项「统计」(BarChartOutlined),`telemetry:read` 门控
  (PermissionName 已存在;owner/admin/viewer seed 已含)。settings 里 disabled 的
  「遥测」占位项删除——数据页不是配置页,retention 走 config.toml。
- 查询端点查 **rollup 表**(不查 raw,响应稳定快速):
  - `GET /api/v1/telemetry/summary?app=&days=` → 指标卡(今日活跃、下载完成、最新版 Active%)
  - `GET /api/v1/telemetry/adoption?app=&days=` → device_rollup_day 序列(by version)
  - `GET /api/v1/telemetry/funnel?app=&days=` → 漏斗 5 节点计数
  - `GET /api/v1/telemetry/distribution?app=&days=&dim=platform|arch|version` → 分布
- 图表 `@ant-design/plots`(AntD 生态官方图表库;admin 唯一新依赖)。
- 漏斗口径:**按次计数**(count),不按设备——口径标注在 UI tooltip;按设备去重的
  漏斗留后续(需要跨事件源 distinct,rollup 不支持,属"后续指标")。

### 9. SDK 契约(本 change 只写文档,不写代码)

`packages/sdk` 后续接入 change 的硬约束,先在 docs/10 固化:

- 上报静默失败(fire-and-forget,不重试不阻塞状态机)
- `telemetry: false` 配置项,app 开发者可向最终用户透传 opt-out;check 请求本身是
  功能必需不在 opt-out 范围
- 事件发射点 = 状态机转移:`downloading 进入→download_started`、`ready→download_completed`、
  `error(下载中)→download_failed`、RN 安装器调起→`install_started`、安装异常→`install_failed`、
  启动时 `current_version != last_seen_version`→`app_started_after_update`(带 previous_version)
- client_id 复用 `ensureClientId`;提供 reset(清存储重新生成)

## Risks / Trade-offs

- **[client_event 可伪造]** → 公开端点本质如此(业界相同);缓解:限流 + 白名单 + app 必须
  存在 + 长度校验;设备数等关键指标只从可信的 update_event 算(Decision 3 来源限定)。
- **[同步 insert 加重 check 延迟]** → 单 insert 个位数 ms、QPS 低(12h 节流);swallow 保证
  故障不传染。若未来量级上来,再改批量 channel(决策点留 backend.md)。
- **[重算式 rollup 在大 raw 表上变慢]** → 重算窗口只有 2 天的 raw(90 天表的 ~2%),
  created_at 索引下毫无压力。
- **[漏斗按次 vs 按设备口径混淆]** → UI 明确标注口径;funnel 端点文档写清。
- **[client_id 缺失的老客户端]** → update_event.client_id 可空;device_rollup 只统计非空,
  adoption 曲线在旧 SDK 存量期内偏低,随升级自愈(文档注明)。
- **[`@ant-design/plots` 包体积]** → admin 是后台应用且已有 antd-vendor 大 chunk;
  按需 import + 现有 chunk 策略,可接受。

## Migration Plan

纯 additive:4 张新表(schema-sync)+ 新配置段(有默认值)+ 新端点。无数据迁移、
无既有 API 行为变化(check/download 响应不变)。回滚 = revert 代码,表残留无害。

## Open Questions

- **install_failed 在 RN 的可探测性**:用户在系统安装器点取消 vs 真失败难区分,
  SDK 接入 change 时定语义(MVP server 端先照收)。
- **漏斗的设备去重版**:需要跨事件 distinct(raw 才能算),是否值得在 raw 留存期内
  提供"近 90 天精确漏斗"端点 → 留后续,看运营反馈。
- **多 app 聚合首页**(不选 app 的全局视图)→ MVP 必选 app,后续看需求。
