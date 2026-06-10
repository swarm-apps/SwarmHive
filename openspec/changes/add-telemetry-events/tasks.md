# tasks

> 落点已按真实代码核对:hook 在 `routes/updates.rs`(两处 check)与 `routes/download.rs`;
> swallow 模式照 `services/audit.rs`;周期任务在 bin 启动 spawn;权限 `TelemetryRead` 已存在。
> 排序:支柱 A(采集 + 聚合,server)可独立验收;支柱 B(查询端点 + 统计页)其后。

## 支柱 A:采集与聚合

## 1. Entity(4 张新表)

- [x] 1.1 [code] `crates/swarmhive-entity/src/update_event.rs`:`id uuid v7`、`event_name`
      (ActiveEnum: `update_check`|`download_intent`,`#[serde(rename_all="snake_case")]` 对齐
      string_value,防 mail_provider 同款 wire 分叉坑)、`result`(ActiveEnum:
      `up_to_date`|`available`|`rollout_held`|`redirected`|`failed`)、`app_id Uuid`、
      `release_id Option<Uuid>`、`channel Option<String>`、`current_version Option<String>`、
      `platform String`、`target/arch/abi Option<String>`、`artifact_id Option<Uuid>`、
      `client_id Option<String>`、`created_at`。**无 ip/user_agent 列**
- [x] 1.2 [code] `crates/swarmhive-entity/src/client_event.rs`:`event_name`(ActiveEnum 6 种)、
      `app_id`、`channel?`、`target_version?`、`previous_version?`、`platform`、
      `client_id String`(必填)、`bytes_total Option<i64>`、`duration_ms Option<i64>`、
      `error_code?`、`error_message?`、`created_at`
- [x] 1.3 [code] `crates/swarmhive-entity/src/{event_rollup_day,device_rollup_day}.rs`:
      rollup 双表(design Decision 3 维度);唯一键用 `#[sea_orm(unique_key=...)]` 复合标签
      (**不**用 raw partial index,rc.38 坑);`day` 用 `Date`
- [x] 1.4 [code] entity `lib.rs` 注册 4 表;api-types 新增 `telemetry.rs` DTO
      (查询响应:Summary/AdoptionSeries/Funnel/Distribution + ReportEventReq)

## 2. services/telemetry.rs(swallow 写入 + 周期任务)

- [x] 2.1 [code] `record_update_event(db, NewUpdateEvent)`:swallow 失败 + `tracing::warn`
      (复刻 `audit::write_swallowing` 模式);NewUpdateEvent 字段与 hook 现场一一对应
- [x] 2.2 [code] `run_rollup(db)`:TX 内 delete+insert 重算「今天+昨天」两 bucket;
      `event_rollup_day` 按全维度 group-by count(update_event + client_event 两源,
      `source` 列区分);`device_rollup_day` 仅从 `update_check` 算 `COUNT(DISTINCT client_id)`
      (含 version=NULL 总行);聚合优先 sea-orm 结构化 API,表达力不够则 raw SQL
      (backend.md 记"第三处刻意 raw SQL")
- [x] 2.3 [code] `run_cleanup(db, retention_days)`:删两张 raw 表 `created_at < cutoff`;
      `0` 直接返回
- [x] 2.4 [code] config 新增 `[telemetry] raw_retention_days = 90`(`config/default.toml` +
      `config::TelemetrySection`,注意与现有 `telemetry.log_level` 段名冲突——现有
      `TelemetryConfig` 是日志配置!**先核对再定段名**,必要时用 `[events]` 或并入现有段)
- [x] 2.5 [code] bin `server.rs` 启动 spawn 周期任务:rollup 每小时(启动即跑一次)、
      cleanup 每天;失败 warn 不退避重试
- [x] 2.6 [test] `telemetry_rollup_smoke.rs`(testcontainers):造 raw(3+2 distinct clients,
      一客户端 check 两次)→ rollup → 断言 per-version unique 与 NULL 总行;二跑幂等;
      伪造 91 天前 created_at → cleanup 删 raw 留 rollup

## 3. Hook 落库改造(routes/updates.rs + routes/download.rs)

- [x] 3.1 [code] `routes/updates.rs` Tauri check 路由:三个出口(up_to_date 204 / available 200 /
      rollout_held)各调 `record_update_event`(result 对应);保留原 tracing
- [x] 3.2 [code] 同文件 RN android check 路由:同样三出口改造
- [x] 3.3 [code] `routes/download.rs`:redirect 成功 → `result=redirected`;生成下载地址失败
      分支 → `result=failed`;带 artifact_id/release_id
- [x] 3.4 [test] 扩 `update_check_tauri_smoke.rs` / `update_check_rn_android_smoke.rs`:
      check 后断言 update_event 行(含 rollout_held 场景——已有灰度测试基建);
      模拟表缺失(drop table)→ check 仍 200/204

## 4. POST /api/v1/events(routes/events.rs,扁平)

- [x] 4.1 [code] 公开 handler:GardeJson 校验(event 白名单 serde enum、client_id ≤64、
      app slug 必填);app 查不到 → 404;error_message 截 512;写 `client_event` swallow
      仍 200;挂 sensitive 子树(governor);handler 名避撞(`report_client_event`)
- [x] 4.2 [test] `client_events_smoke.rs`:正常上报落库 / 未知 app 404 / 非法 event 422 /
      超长 error_message 截断 / 表缺失仍 200

> ✅ **支柱 A 验收点**:check/download 即产生可信事件,rollup 出 adoption 数据,
> SDK 契约端点可用——统计页未建前可先用 SQL/psql 验证数据正确性。

## 支柱 B:查询端点 + Admin 统计页

## 5. routes/telemetry.rs(查询,telemetry:read)

- [x] 5.1 [code] `GET /api/v1/telemetry/summary?app=&days=`:今日活跃(device_rollup NULL 行)、
      期内 download_completed 计数、最新版本 Active%(最新 published release 的 unique /
      当日总 unique)
- [x] 5.2 [code] `GET .../adoption?app=&days=`:device_rollup_day 按 version 序列
- [x] 5.3 [code] `GET .../funnel?app=&days=`:event_rollup_day 四级计数 + 转化率
      (check available → intent redirected → download_completed → app_started_after_update)
- [x] 5.4 [code] `GET .../distribution?app=&days=&dim=platform|arch|version`:
      event_rollup_day 按 dim group;全部 `require_permission!(TelemetryRead)`;
      mount(api_routes)+ utoipa;`pnpm openapi` 重生成
- [x] 5.5 [test] 并入 `telemetry_smoke.rs::telemetry_queries_are_gated_and_consistent`(owner 200 / 匿名 401 / 非法 dim 422 / adoption 与 rollup 一致)

## 6. Admin /telemetry 统计页

- [x] 6.1 [code] `pnpm --filter @swarm-hive/admin add @ant-design/plots`;
      `lib/api/telemetry.ts` queryOptions × 4
- [x] 6.2 [code] `routes/_auth/telemetry.tsx`:app Select(复用 apps 列表 query)+
      天数 Segmented(7/30/90);指标卡 ×3;adoption Line(by version);Funnel(口径
      tooltip 标注"按次计数");platform/arch 分布;版本长尾表(含最旧活跃版本)
- [x] 6.3 [code] `_auth/route.tsx`:顶层菜单「统计」(BarChartOutlined,`has("telemetry:read")`
      门控);**删除** settings 子菜单里 disabled 的「遥测」占位项
- [x] 6.4 [code] 空态:无数据时引导文案("接入 SDK 上报或等待客户端 check");
      typecheck + biome + `pnpm admin:build`(routeTree 重生成)

## 7. 测试与文档

- [x] 7.1 [test] `cargo test --workspace` 全绿(新增 4 个 smoke);clippy 零警告
- [x] 7.2 [docs] `docs/10-telemetry.md` 按落地实情重写:事件清单(result 维度)、
      隐私姿态(无 IP/UA、client_id 假名化声明、retention)、SDK 上报契约
      (静默失败、telemetry:false、reset client_id、状态机发射点映射)
- [x] 7.3 [docs] `docs/08-admin-and-analytics.md` 统计页段;`dev-notes/knowledge/backend.md`
      加 telemetry 段(双表 rollup 的可加/不可加决策、swallow 模式、周期任务范式、
      [telemetry] 配置段名结论);`admin-spa.md` 加 @ant-design/plots 用法注意
- [x] 7.4 [docs] memory `project-telemetry-events.md` **已在 propose 阶段修订**(Tauri 限制
      过时、IP/UA 不落盘、update_available 并入 result、rollup 双表)——apply 收尾时复核
      与最终实现无出入即可
- [x] 7.5 [docs] `openspec/changes/README.md` 状态行更新

## 8. 端到端验证

> 8.1/8.2 的 server 侧断言已由集成测试等价覆盖(`telemetry_smoke` 走真实 router 的
> check/events/查询;三种 result 落库见 `update_check_tauri_smoke`);以下勾选留给
> 真机浏览器过一遍(/telemetry 页真渲染 + 429 实测)。

- [ ] 8.1 [code] 本地起 server + 真实 check(curl 模拟 Tauri/RN)→ psql 查 update_event
      三种 result 各一行
- [ ] 8.2 [code] curl POST /events 六种事件 → client_event 落库;429 验证(连发超阈值)
- [ ] 8.3 [code] 手动触发 rollup → /telemetry 页 adoption/漏斗渲染;viewer 账号可见、
      无权限账号菜单消失
