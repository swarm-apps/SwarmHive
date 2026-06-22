## Context

SwarmHive 已有发布列车(release publish / channel promote / rollback)与 `Mailer`(`add-mail-infrastructure`,SMTP + 模板 + AES-256-GCM 密钥),但发布后零外发通知。业界调研(GitHub webhooks、Sentry trigger-filter-action、Grafana contact-point+integration、PagerDuty Event Orchestration、Standard Webhooks、CloudEvents、transactional outbox)给出清晰范式。约束:单机 self-hosted、Postgres 唯一存储、5 crate 边界、不引外部 MQ。

## 数据流

```text
   发布列车 handler(publish / promote / rollback)
        │  ① 同一 DB 事务内 emit 事件
        ▼
   notify::emit(event_type, app_id, payload)
        │  写 notification_outbox 行(status=pending) —— 与业务变更同事务,要么都成功要么都回滚
        ▼
   ┌─────────────────────────────────────────────┐
   │ notification_outbox (Postgres)              │  ← 事务性 outbox:崩溃不丢
   └─────────────────────────────────────────────┘
        │  ② worker(tokio 任务):LISTEN swarmhive_outbox 唤醒
        │     + SELECT ... FOR UPDATE SKIP LOCKED 取一批
        ▼
   notify worker:对每个 event 查 subscription 命中 → 展开成多条 delivery
        │
        ├─ email channel ──► Mailer::send(MailEnvelope)            (复用现有)
        └─ webhook channel ─► StandardWebhooksSigner.sign(body)    (HMAC-SHA256 v1)
                              │ headers: webhook-id / -timestamp / -signature
                              ▼  HTTP POST 订阅方 endpoint(reqwest,带超时)
        │
        ▼
   ┌─────────────────────────────────────────────┐
   │ notification_delivery (status/code/attempts/ │  ← 每次投递记录;失败 → 指数退避重排
   │  next_retry_at)  超过 max_attempts → dead     │     重投接口手动 re-enqueue
   └─────────────────────────────────────────────┘
```

## Goals / Non-Goals

**Goals:** 事件 ↔ 通道解耦的可扩展通知层;至少一次 + 幂等的可靠投递;webhook 走业界标准(订阅方能用现成库验签);全程 Postgres 内置、无外部依赖。

**Non-Goals:** 见 proposal —— 飞书/钉钉/QQ 专用 bot 适配、WebSub、完整 CloudEvents、外部 MQ/Svix、ed25519、扇出/限流精调,均不在本 change。

## Decisions

- **D1 四层模型(event / subscription / channel / delivery)而非邮件直发**。理由:Sentry/Grafana 一致用"事件条件 ↔ 投递通道"正交模型;加聊天机器人/新事件只是加 provider/event_type,不改核心。备选:沿用 `Mailer` 直接在 handler 发邮件——被拒,无法扩 webhook/订阅、与业务逻辑耦合。
- **D2 事务性 outbox + 进程内 worker**。理由:emit 与业务变更同事务 → 崩溃不丢、不脏发(rollback 的事件不会发出);worker 用 `LISTEN/NOTIFY` 唤醒 + `FOR UPDATE SKIP LOCKED` 取批,单机零额外组件。备选:① handler 内 fire-and-forget(被拒——崩溃丢、事务回滚仍发);② 外部 MQ/Svix(被拒——违背单机 self-hosted)。实现库倾向自建轻量(对照 `apalis`/`sqlxmq`,见 Open Questions)。
- **D3 webhook 通道实现 Standard Webhooks(`v1` 对称)**。SwarmHive 是**发送方**:生成 `webhook-id`(uuid,幂等键)+ `webhook-timestamp`,对 `id.timestamp.rawbody` 算 HMAC-SHA256,头 `webhook-signature: v1,<base64>`。理由:跨厂商事实标准,订阅方有现成 verifier;自带防重放(timestamp)+ 幂等(id)。备选:GitHub `X-Hub-Signature-256`(可作兼容别名,但主用 Standard Webhooks)。新依赖 `hmac`+`sha2`。
- **D4 webhook secret 复用 `crypto::SecretKey` AES-256-GCM 落库**,创建时明文一次性返回(镜像 API token / mail provider 密码的既有范式)。
- **D5 channel 抽象为 trait**(类比 `Mailer`):`email` 复用 Mailer;`webhook` 为 Standard Webhooks 实现;**聊天机器人 MVP = 指向其 incoming-webhook URL 的通用 JSON webhook**,专用签名/格式留后续 change。
- **D6 event_type 借鉴 CloudEvents 字段**(`id`/`source`/`type`/`time`,type 形如 `com.swarmhive.release.published`),但不实现完整 CloudEvents 规范。

## Risks / Trade-offs

- 订阅方慢/挂阻塞 worker → 每投递硬超时 + 有界并发 worker 池 + 指数退避重试 + 超 max_attempts 置 dead(DLQ 语义)。
- 扇出放大(一个 release → 上千订阅) → MVP 仅有界并发 + 基础节流,**明确不调优**(Non-goal),delivery 表可观测后再迭代。
- **SSRF**:webhook URL 指向内网/元数据端点 → MVP 至少校验 scheme=https(dogfood 允 http)+ 拒私网/loopback IP;真正加固列 task。
- axum 验签需**反序列化前拿原始 body bytes**(一个空格都会让签名失效)→ 用 `Bytes` extractor + 手动 parse,不要先 `Json<T>`。
- 至少一次会重复投递 → 由 `webhook-id` 幂等键托底,订阅方去重(文档说明)。

## Migration Plan

纯增量:4 张新表(`webhook_endpoint` / `notification_subscription` / `notification_delivery` / `notification_outbox`)由 **sea-orm `schema-sync` 从 entity 定义自动建表**(dev/test)/ deployer(prod)—— **不写 migration crate 文件**(该 crate 只做存量数据改写,本 change 无数据迁移)。无数据迁移。发布列车 handler 增量 emit(不改既有行为)。回滚:停 worker + 新表无外部引用,可弃用。索引暂不声明(rc.38 schema-sync 对索引是已知雷区;通知量小;真需要交 prod SQL / 后续)。

## Open Questions

- ~~outbox worker 自建 vs apalis/sqlxmq~~ **已定**(apply spike):自建轻量 tokio worker + `FOR UPDATE SKIP LOCKED` interval 轮询(镜像 `services/telemetry.rs::spawn_tasks`);**LISTEN/NOTIFY 延迟优化后置**(MVP 几秒轮询足够)。不引重依赖。
- webhook SSRF 加固边界(MVP 仅 https + 拒私网,还是要可配 allowlist)。
- 飞书(HMAC-SHA256 加签 + timestamp)/钉钉/QQ/Discord 的专用签名与消息格式 → 拆到 `add-notification-im-providers` + 单独子调研(本 change Non-goal)。
