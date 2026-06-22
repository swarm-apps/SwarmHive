# Tasks — add-notification-secret-rotation-grace

## 1. entity [code]

- [x] 1.1 `webhook_endpoint.rs`:Model +2 nullable 列 `previous_secret_encrypted: Option<String>` / `previous_secret_expires_at: Option<DateTimeUtc>`;`From<&Model> for api::WebhookEndpoint` 加 `previous_secret_expires_at`。

## 2. api-types [code]

- [x] 2.1 `notification.rs`:`WebhookEndpoint` view + `previous_secret_expires_at: Option<DateTime<Utc>>`。

## 3. channel 双签 [code]

- [x] 3.1 `notify/channel.rs`:`DeliveryTarget::Webhook` + `previous_secret: Option<String>`;`deliver_payload` 签名加 previous 形参,宽限期有旧密钥时 `webhook-signature = format!("{new} {old}")`(空格分隔多 `v1,`);snapshot.signature 存实际多签名头。webhook `deliver` 从 target 取 previous 传入。
- [x] 3.2 test endpoint 路径 previous 传 None(单签)。

## 4. worker [code]

- [x] 4.1 `notify/worker.rs::delivery_request`:webhook 分支在 `previous_secret_expires_at > now` 时解密 `previous_secret_encrypted` 一并放进 `DeliveryTarget::Webhook.previous_secret`。

## 5. rotate handler [code]

- [x] 5.1 `routes/notifications.rs::rotate_webhook_secret`:`previous_secret_encrypted = Set(Some(existing.secret_encrypted))`、`previous_secret_expires_at = Set(Some(now + 24h))`、`secret_encrypted = Set(encrypt(new))`;返回新明文不变。`const ROTATION_GRACE`。

## 6. admin / cli [code]

- [x] 6.1 admin `notifications/index.tsx`:Endpoints 列加宽限 Tag(`previous_secret_expires_at` 非空且未过期 → 「轮换中,旧密钥至 X」);轮换 `modal.confirm` content 改「旧密钥保留 24h,期间新旧双签,接收端有时间切换」。i18n。
- [x] 6.2 cli `commands/notifications.rs`:EndpointRow 可选加 grace 列 / 或在 get 显示(轻量)。

## 7. 测试 [test]

- [x] 7.1 notification smoke:create endpoint + sub + publish + 投递(单签验证已有);**rotate** → 再 publish/redeliver → 断言投递的 `webhook-signature` 头含两个 `v1,` 且新旧 secret 各能验签通过;把 `previous_secret_expires_at` 置过去(或直接断言列)验过期后只单签。

## 8. 验收 gates [test]

- [x] 8.1 `cargo build --workspace` / `clippy -D warnings` / `fmt --check` / `cargo test --workspace`(notification smoke + openapi_surface)绿;admin `typecheck`/`lint`/`build`/`vitest`;`schema.gen.ts` 含新字段;CLI `--help`。

## 9. docs / 同步 [docs]

- [x] 9.1 `docs/15-notifications.md` Standard Webhooks 段补 dual-signing 宽限(删硬切换);`openspec/changes/README.md`;`dev-notes/knowledge/project-notifications.md`。
