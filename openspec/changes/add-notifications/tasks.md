## 1. Spike + 依赖

- [ ] 1.1 [code] outbox worker 选型 spike:`LISTEN/NOTIFY` + `SELECT ... FOR UPDATE SKIP LOCKED` 自建 vs `apalis`/`sqlxmq`;记结论到 design.md「Open Questions」(倾向自建,避免重依赖)
- [ ] 1.2 [code] workspace `Cargo.toml` 加 `hmac` + `sha2`(Standard Webhooks HMAC-SHA256);确认 `reqwest` 已在(webhook POST)

## 2. api-types DTO（无 sea-orm）

- [ ] 2.1 [code] `api-types/src/notification.rs`:`NotificationEventType` 枚举(`release.published`/`channel.promoted`/`channel.rolled_back`,serde wire 串 + utoipa)、`NotificationEvent` 信封(id/source/type/time/data,CloudEvents 风格字段)
- [ ] 2.2 [code] `Subscription` / `CreateSubscriptionReq`(channel_id + event_types + 可选 app_slug)、`Channel`/`ChannelKind`(email/webhook)DTO
- [ ] 2.3 [code] `WebhookEndpoint`(永不含 secret)、`CreateWebhookEndpointReq`、`CreateWebhookEndpointResp`(一次性 `whsec_` 明文,flatten WebhookEndpoint)、`Delivery`(status/response_code/attempt/timestamps)DTO
- [ ] 2.4 [test] api-types crate `cargo tree -p swarmhive-api-types | grep sea-orm` 为空(边界回归)

## 3. entity + migration（migration 用 raw SQL,不依赖 entity）

- [ ] 3.1 [code] migration:`notification_subscription`(channel_id FK、event_type、app_id 可空、created_at)
- [ ] 3.2 [code] migration:`webhook_endpoint`(url、secret_ciphertext〔AES-256-GCM〕、prefix、disabled、created_at)+ `notification_delivery`(endpoint_id、event_id、webhook_id、status、response_code、attempt、next_retry_at、created/updated)+ `notification_outbox`(event 信封、status=pending/dispatched、created_at)
- [ ] 3.3 [code] entity:四表 Model/ActiveModel + `From<&Model>` → api-types(转换写 entity crate)
- [ ] 3.4 [test] migration up 在 testcontainers Postgres 通过,表/索引齐(outbox 按 status+created、delivery 按 next_retry_at 建索引)

## 4. Standard Webhooks 签名

- [ ] 4.1 [code] `server/src/notify/signer.rs`:`sign(webhook_id, ts, raw_body, secret) -> "v1,<base64>"`,HMAC-SHA256 over `{id}.{ts}.{body}`;secret 取 `whsec_` 后段 base64 解码作 key
- [ ] 4.2 [test] 单测对 standardwebhooks.com / Svix 公开测试向量,签名逐字节匹配(锁死实现正确)

## 5. channel provider 抽象 + 两实现

- [ ] 5.1 [code] `NotificationChannel` trait（`deliver(event, target) -> Result<DeliveryOutcome>`,类比 `Mailer`);outcome 带 http code / 错误分类(可重试 vs 终态)
- [ ] 5.2 [code] email provider:复用 `Mailer::send`,组 `MailEnvelope`(release 详情);webhook provider:生成 `webhook-id`(uuid)+ ts,signer 签名,reqwest POST 带三头 + 硬超时
- [ ] 5.3 [code] webhook URL 校验:scheme `https`(dev profile 容忍 http)+ 拒私网/loopback IP(SSRF 基础加固)

## 6. outbox + worker（可靠投递核心）

- [ ] 6.1 [code] `notify::emit(txn, event)`:在调用方事务内 INSERT `notification_outbox`(随业务一起提交/回滚)
- [ ] 6.2 [code] worker(tokio 任务,server 启动时 spawn):`LISTEN swarmhive_outbox` 唤醒 + `FOR UPDATE SKIP LOCKED` 批取;对每 event 查 subscription 命中 → 展开 `notification_delivery` 行
- [ ] 6.3 [code] 投递循环:调 channel provider;成功置 sent;可重试失败按指数退避写 `next_retry_at`;超 max_attempts 置 `dead`;有界并发 + 基础节流
- [ ] 6.4 [test] 集成:fake webhook 接收端,验 订阅→签名头正确→投递 sent;5xx→退避重试;超限→dead(testcontainers)

## 7. 事件 emit 接入发布列车

- [ ] 7.1 [code] release publish handler 事务内 `notify::emit(release.published)`;channel promote→`channel.promoted`;rollback→`channel.rolled_back`(payload 含 app/version/channel)
- [ ] 7.2 [test] 集成:publish 成功 → outbox 有行;publish 事务回滚 → outbox 无行(事务性保证)

## 8. routes + RBAC + utoipa

- [ ] 8.1 [code] `routes/notifications`:subscription `{list,create,delete}`、webhook endpoint `{list,create,delete,rotate-secret}`(create/rotate 返回一次性 `whsec_`)、delivery `{list, redeliver}`(redeliver 保持原 webhook-id)
- [ ] 8.2 [code] `PermissionName::NotificationManage` + seed;所有 notification 端点挂 `notification:manage` 门控;错误 RFC 9457;路由挂 `src/lib.rs`
- [ ] 8.3 [code] 全端点 utoipa `#[utoipa::path]` 注解 + schema,tag = `notifications`
- [ ] 8.4 [test] `openapi_surface`:`EXPECTED_TAGS` 加 `notifications`、新端点在 doc;`bearer`/permission smoke 验 viewer 无 `notification:manage` 时 403

## 9. docs / memory / README 同步

- [ ] 9.1 [docs] 新增 `docs/15-notifications.md`(四层模型 + Standard Webhooks 契约 + 订阅/重投 + SSRF/https 说明);`docs/README.md` 列入
- [ ] 9.2 [docs] `openspec/changes/README.md` 依赖图 + 阶段映射加 `add-notifications`(depends on mail-infrastructure / app-release-artifact / auth-and-rbac)
- [ ] 9.3 [docs] memory 新增 `project-notifications.md`(术语决策、Standard Webhooks 选型、outbox、Non-goals);blog §8 release_watcher 标注「已演进为本 change」
- [ ] 9.4 [docs] 记 follow-up:`add-notification-im-providers`(飞书/钉钉/QQ/Discord 专用签名+格式)、`add-notifications-page-ui`(admin 管理页)、ed25519 `v1a`

## 10. gates

- [ ] 10.1 [test] `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` 全绿
- [ ] 10.2 [test] `pnpm --filter @swarm-hive/admin typecheck`(schema.gen.ts 随新 endpoint regen,确认无意外 drift)+ openapi drift gate 绿
- [ ] 10.3 [test] 边界回归:`cargo tree -p swarmhive-cli | grep sea-orm` 空、api-types 无 sea-orm/axum
