# Design — add-notification-worker-hardening

## 1. 投递事务边界重构(#1,核心)

### 问题(现状)

```text
deliver_due_batch:
  ┌─ BEGIN ───────────────────────────────────────────────┐
  │ SELECT ... FOR UPDATE SKIP LOCKED  (锁住整批 50 行)      │
  │ for row in rows:                                        │
  │     deliver_one(&txn, row)                              │
  │         ├─ HTTP POST  ←── 外部 IO 在事务内,最坏 10s/条   │
  │         └─ UPDATE delivery + INSERT attempt (&txn)      │
  │ COMMIT                                                  │
  └────────────────────────────────────────────────────────┘
  任一条 deliver_one 返回 DbErr → ? 上抛 → 整批 rollback,
  但 HTTP 已发出 → 下一 tick 重发(重复投递)+ 全程占行锁。
```

### 方案(目标)—— 无 schema 变更

```text
deliver_due_batch:
  ┌─ BEGIN(短) ─┐
  │ SELECT due   │   认领一批(FOR UPDATE SKIP LOCKED)
  │ COMMIT       │   立即释放行锁,行数据已读入内存
  └──────────────┘
  for row in rows:                  ← 事务外、逐条独立
      delivery_request(&db, &row)   ← 只读 outbox/sub/endpoint(连接池)
      ┌── 外部投递(无任何事务)──┐
      │ webhook.deliver / email   │   慢 webhook 不占 DB 连接/行锁
      └───────────────────────────┘
      ┌─ BEGIN(短) ─────────────────────────────┐
      │ mark_success|failure + record_attempt    │   各自独立提交,
      │ (became_dead 时) update_endpoint_health   │   一条失败不牵连其它
      └─ COMMIT ──────────────────────────────────┘
```

**关键性质**:
- 行锁只在「认领」短事务内持有,不跨外部 IO。
- 每条投递的结果独立提交 → 一条落库失败不回滚、不重发其它条。
- 残留 at-least-once 重发窗口(发完即崩,结果未提交)→ 下 tick 重发,接收端按 `webhook-id`(= 稳定 event id)去重(既有 spec scenario「Redelivery reuses the webhook-id」)。
- **前置错误**(endpoint 已禁用 / 密钥解密失败)语义不变:仍只标 failed、**不**动 endpoint 健康。
- **单 worker 约束**:`run_once` 在 interval loop 内串行、tick 间不重叠(`tick().await` 后 `run_once().await` 完成才进下一轮),MVP 单 server → 认领后释放锁不会被本进程并发双拣。多 worker(需租约列)是 Non-goal。

`expand_outbox_batch`(纯 DB fanout,无外部 IO)保持单事务不变。

## 2. 索引(#2)

migration crate 新文件,raw SQL,`to_regclass` 守卫容忍表未建:

| 索引 | 表 | 列 | 服务的查询 |
|---|---|---|---|
| `idx_notification_outbox_status_created` | notification_outbox | (status, created_at) | outbox 待派发扫描 |
| `idx_notification_delivery_due` | notification_delivery | (status, next_retry_at, created_at) | worker 到期投递扫描 |
| `idx_notification_delivery_endpoint_updated` | notification_delivery | (webhook_endpoint_id, updated_at) | Admin 按 endpoint 列投递 |
| `idx_notification_subscription_event_app` | notification_subscription | (event_type, app_id) | outbox fanout 找订阅 |
| `idx_notification_delivery_attempt_delivery` | notification_delivery_attempt | (delivery_id, attempt_no) | 详情按 attempt 升序取 |

`CREATE INDEX IF NOT EXISTS` 幂等;dev(schema-sync 先建表 → migration 补索引)与 prod(deployer 建表 → 每次启动 `Migrator::up` 补)都生效。**约定例外**:migration crate doc 原写「不管 schema」,此处明确放宽到「schema-sync 表达不了的二级索引」,不含建表/改列。

## 3. 轮换护栏(#3)

`rotate_webhook_secret` 在 generic 校验后、生成新密钥前插一道:`previous_secret_expires_at` 存在且 `> now()` → **409 Conflict**(资源状态冲突,与 releases.rs 对状态约束的处理一致;判据 `> now` 与 worker 双签定义一致,边界自洽),要求等宽限期结束再轮换。只用单 previous slot 的最简护栏,不引多密钥(Non-goal)。

## 4. Admin 按钮(#4)

抽 `canRotateSecret(providerKind?) = (providerKind ?? "generic") === "generic"`(`lib/api/notifications.ts`,可单测);列操作区用它条件渲染「轮换密钥」按钮。

## 5. 受影响 crate

```text
swarmhive-migration ──(新 migration)──┐
swarmhive-server (notify/worker, routes/notifications) ──┤── PR #5 / apply-notifications
apps/admin (settings/notifications) ──────────────────────┘
```

无 entity / api-types 改动(零 DTO、零 schema 列变更)。
