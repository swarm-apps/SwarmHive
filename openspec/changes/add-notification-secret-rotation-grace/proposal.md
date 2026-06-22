## Why

`add-notifications-page-ui` 落地时,webhook 签名密钥轮换是**硬切换**——旧密钥立即失效,在途 / 未及更新的接收端会**验签全失败**,所以 Admin / CLI 的轮换确认都不得不挂「旧密钥立即失效」的告警。Standard Webhooks 规范本身支持**密钥轮换宽限**:`webhook-signature` 头可空格分隔多个签名(`v1,<新> v1,<旧>`),接收端任一匹配即通过。本 change 实现零停机轮换:轮换时保留旧密钥 24h,期间新旧双签,给接收端从容切换的时间窗。

## What Changes

- **entity `webhook_endpoint`** 新增 2 nullable 列(schema-sync 加列):
  - `previous_secret_encrypted`(Text)—— 上一把密钥的 AES-256-GCM 密文(轮换时由 current 移入)。
  - `previous_secret_expires_at`(timestamptz)—— 旧密钥失效时刻(轮换时 = now + 24h)。
- **rotate handler**(`routes/notifications.rs`):轮换时 `previous_secret_encrypted = current`、`previous_secret_expires_at = now + 24h`、`secret_encrypted = encrypt(new)`;一次性返回新密钥(不变)。
- **worker**(`notify/worker.rs::delivery_request`):构造 webhook target 时,current 总解密;`previous_secret_expires_at > now` 时**额外解密旧密钥**一并下传。
- **channel**(`notify/channel.rs`):`DeliveryTarget::Webhook` 带 `previous_secret: Option<String>`;`deliver_payload` 在宽限期内对同一 body 用新旧各签一次,`webhook-signature` 头 = `v1,<新> v1,<旧>`(空格分隔)。`request_signature` 快照存实际发送的多签名头。test endpoint 仍单签(配置自检无需双签)。
- **api-types `WebhookEndpoint` view** 加 `previous_secret_expires_at: Option<DateTime<Utc>>`(只是时间戳,不泄密钥),供 Admin / CLI 展示「轮换宽限期至 X」。
- **admin / cli**:Endpoints 列表展示宽限状态(Tag「轮换中,旧密钥至 X」);轮换确认文案从「立即失效」改为「旧密钥保留 24h,期间新旧双签」。

## Acceptance

- `cargo build --workspace` / `clippy --workspace --all-targets -- -D warnings` / `fmt --check` / `cargo test --workspace` 绿。
- notification smoke 新增:轮换后宽限期内,worker 投递的 `webhook-signature` 头同时含新旧两个 `v1,` 签名,**新旧密钥都能验签通过**;`previous_secret_expires_at` 过期后只剩新签名。
- `openapi_surface` 通过;admin `typecheck`/`lint`/`build` 绿,`schema.gen.ts` 含新字段;CLI `--help` 正常。

## Non-goals

- ❌ 宽限时长可配置:MVP 固定 24h(`const`);可配留后续。
- ❌ 多于一把旧密钥:只追踪最近一把 previous(宽限期内再次轮换会覆盖更早的 previous)。
- ❌ ed25519 / `v1a` 非对称签名(仍 `v1` 对称,沿 add-notifications Non-goal)。
- ❌ 生产自动 ALTER:dev schema-sync 加列;生产 deployer `ALTER TABLE webhook_endpoint ADD COLUMN ...`。

## Depends on

- `add-notifications`(✅ webhook_endpoint + signer + channel)、`add-notifications-page-ui`(✅ 轮换 UI)、`add-notification-delivery-payload-log`(✅ request_signature 快照,验证多签名头)。

## Maps to docs

- `docs/15-notifications.md`(Standard Webhooks 段补 dual-signing 轮换宽限,删「硬切换」描述)。
- 更新 `openspec/changes/README.md` + `dev-notes/knowledge/project-notifications.md`。
