## Context

`notification_delivery` 是单行模型,快照字段(request/response)latest-attempt 覆盖。重试时旧尝试快照丢失。本 change 加 append-only 的 `notification_delivery_attempt` 表记录每次尝试,detail 端点连带返回时间线。约束:schema-sync 建新表(dev)/ deployer 建表(prod);跨 entity / api-types / server / admin / cli。新表纯增量,不依赖 outbox(防实体漂移)。

## 数据流

```text
  worker.deliver_one → mark_success / mark_failure(更新 delivery 行 latest 快照,现有)
        │  额外:record_attempt(db, delivery_id, attempt_no, status, snapshot, last_error)
        ▼
  notification_delivery_attempt(append-only,每次尝试一行)
        ▼
  GET /deliveries/{id} → DeliveryDetail { delivery, <latest 快照>, attempts: Vec<DeliveryAttempt> }
        │  attempts 按 attempt_no 升序
        ▼
  admin 行展开:latest 快照 + 尝试时间线;cli deliveries get JSON 自带 attempts
```

## Decisions

- **D1 独立 append-only 表**,不在 delivery 行上扩字段:per-attempt 是一对多;delivery 行保留 latest 快照(列表/兼容/性能)。redeliver append 新 attempt,不清历史。
- **D2 worker 同事务插 attempt**:`mark_success`/`mark_failure` 在 deliver_due_batch 的 txn 内更新 delivery + 插 attempt,崩溃一致。`attempt_no` = 该次的 attempt 序号(与 delivery.attempt 同步)。
- **D3 attempt status 复用 entity `DeliveryStatus`**(sent/failed/dead;无 pending —— attempt 是已发生的尝试)。
- **D4 detail 连带加载**:`get_delivery` 查 attempts 填 `DeliveryDetail.attempts`(列表端点不加载,保持轻);UI 行展开本就懒加载 detail。
- **D5 response_body 截断**:与 delivery 快照同 64 KiB 上限(channel 的 read_capped_body 已保证传入的 body 有界)。
- **D6 schema-sync 建新表**:`add-notifications` 范式(4 表由 schema-sync 从 entity 建),本表同路径;不写 migration crate(无数据迁移)。

## 影响面

```text
entity   notification_delivery_attempt.rs  新 Model + From<&Model> for api::DeliveryAttempt;lib 注册
api-types notification.rs                   DeliveryAttempt DTO + DeliveryDetail.attempts;lib 导出
server   notify/worker.rs                   mark_success/mark_failure 插 attempt(record_attempt helper)
         routes/notifications.rs            get_delivery 加载 attempts
admin    notifications/deliveries.tsx       行展开详情面板加尝试时间线
cli      commands/notifications.rs          deliveries get table 摘要加 attempts:N
tests    app_release_smoke                  5xx 重试到 dead → attempts 长度 == 尝试次数 + 各次 status/code
```

## Risks / Trade-offs

- 每次尝试多插一行 + detail 多查一次 → 单机小量可接受;无清理(永久保留),量大后续加 TTL。
- 快照在 delivery 行与 attempt 行各存一份(latest 冗余)→ 接受(列表免 join + 时间线完整)。
- 生产需 deployer 建表(不自动)→ proposal Non-goal + docs。
