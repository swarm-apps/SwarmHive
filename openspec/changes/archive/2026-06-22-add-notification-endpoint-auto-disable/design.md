## Context

worker `mark_success` / `mark_failure` 目前只更新 delivery 行,不回写 endpoint 健康。endpoint 持续失败只表现为死信堆积,endpoint 仍 enabled。本 change 在 delivery 落终态后回写 endpoint 的 `failing_since`,超阈值自动停用。约束:schema-sync 加 1 nullable 列(dev)/ deployer ALTER(prod);跨 entity / server / api-types / admin / cli。时间基(duration)阈值,volume 无关。

## 数据流

```text
  worker.deliver_one(单条投递)
      │  mark_success / mark_failure(写 delivery)
      ▼
  webhook 投递且落终态 → update_endpoint_health(endpoint_id, healthy)
      │  healthy(sent):  failing_since = NULL(已恢复)
      │  dead:           failing_since ??= now
      │                  now - failing_since >= 3d → disabled = true(保留 failing_since 作标记)
      │  failed(重试中): 不动 endpoint
      ▼
  endpoint.disabled=true → delivery_request 后续投递直接 permanent("disabled")
      ▼
  view.failing_since → Admin「自动停用」/「连续失败中」标签 + Switch 重启(清 failing_since)
```

## Decisions

- **D1 时间基 `failing_since` 单列,而非 consecutive-failures 计数**:duration 阈值 volume 无关(Stripe/Svix 同),且单 nullable 列 schema-sync 友好;计数列在低 volume 下语义模糊(N 次「连续」跨多久不定)。
- **D2 只在 `sent` / `dead` 终态回写**:中间态 `failed`(仍排队重试)不算 endpoint 不健康,避免抖动。
- **D3 自动停用保留 `failing_since`**:作为「因失败停用」的 UI 标记(区分手动停用);手动 re-enable 时由 update handler 清空 → 重置健康窗口。worker 不向 disabled endpoint 投递,故 failing_since 不会被后续 sent 清掉,稳定指示。
- **D4 阈值固定 3 天**(`const AUTO_DISABLE_AFTER_DAYS: i64 = 3`,Stripe 量级):MVP 不可配。检查只在 dead 回写时触发(无定时任务),首次 dead 设 failing_since=now(0 elapsed 不停用),后续 dead 超 3 天才停。
- **D5 mark_failure 返回 `bool`(是否 dead/exhausted)**:deliver_one 据此决定是否调 `update_endpoint_health(healthy=false)`;mark_success 恒 healthy=true。webhook 才回写(email 无 endpoint)。
- **D6 endpoint 健康更新与 delivery 落库同事务**(deliver_due_batch 的 txn 内):崩溃一致。

## 影响面

```text
entity   webhook_endpoint.rs   + failing_since 列 + view From
api-types notification.rs       WebhookEndpoint view + failing_since
server   notify/worker.rs       deliver_one 落终态后 update_endpoint_health;mark_failure 返回 bool;
                                webhook_endpoint_of helper;const AUTO_DISABLE_AFTER_DAYS
         routes/notifications.rs update_webhook_endpoint:disabled=false 时清 failing_since;
                                create ActiveModel failing_since NotSet
admin    notifications/index.tsx Endpoints 健康 Tag(自动停用/连续失败中)
cli      commands/notifications.rs EndpointRow + failing-since 列
tests    app_release_smoke      failing_since 预置过去 + 投递 dead → 自动 disabled;re-enable 清空
```

## Risks / Trade-offs

- 每条 webhook 终态投递多一次 endpoint 读+写 → 低 volume 可忽略;在投递事务内,无额外往返风险。
- 阈值只在 dead 回写时检查 → 完全 idle 的坏 endpoint 直到下次事件投递才停用(无事件无害)。
- 生产需 deployer ALTER(不自动)→ proposal Non-goal + docs。
