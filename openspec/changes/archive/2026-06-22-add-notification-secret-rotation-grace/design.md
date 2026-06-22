## Context

webhook 签名密钥目前单把存 `webhook_endpoint.secret_encrypted`(AES-256-GCM)。轮换硬切换:旧密钥即刻失效。Standard Webhooks 规范允许 `webhook-signature` 头携带多个空格分隔签名,接收端任一匹配即通过 —— 这是零停机轮换的机制。本 change 加「上一把密钥 + 过期时刻」两列,在宽限窗口内双签。约束:schema-sync 加 nullable 列(dev)/ deployer ALTER(prod);跨 entity / server / api-types / admin / cli。

## 数据流

```text
  rotate-secret(routes/notifications.rs)
      │  previous_secret_encrypted = 当前 secret_encrypted
      │  previous_secret_expires_at = now + 24h
      │  secret_encrypted = encrypt(generate_secret())   → 一次性返回新明文
      ▼
  worker.delivery_request(构造 webhook target)
      │  current = decrypt(secret_encrypted)
      │  previous = (previous_secret_expires_at > now) ? Some(decrypt(previous_secret_encrypted)) : None
      ▼
  channel.deliver_payload(url, current, previous, msg_id, body)
      │  sig_new = sign(current, id, ts, body)
      │  header  = previous 有 ? format!("{sig_new} {}", sign(previous, id, ts, body)) : sig_new
      │  POST webhook-signature: v1,<新> v1,<旧>
      ▼
  接收端:任一 v1 签名匹配即通过(宽限期内新旧 secret 都能验)
```

## Decisions

- **D1 多签名头而非双发**:同一请求一个 body、`webhook-signature` 头放多个空格分隔 `v1,...`(Standard Webhooks 规范),不发两次请求。接收端用现成 verifier 自动逐个尝试。
- **D2 宽限窗口靠 `previous_secret_expires_at` 时间判定**,不靠定时清理:worker 每次投递现算 `expires_at > now`;过期后旧密钥自然不再参与签名(列可留作历史,不必删)。
- **D3 固定 24h**(`const ROTATION_GRACE = Duration::hours(24)`):MVP 不做可配。
- **D4 只追踪一把 previous**:再次轮换覆盖。多代密钥并存价值低、复杂度高,Non-goal。
- **D5 view 暴露 `previous_secret_expires_at`(非密钥)**:Admin / CLI 据此显示「轮换宽限至 X」;旧密钥明文永不出 wire(同 current)。
- **D6 test endpoint 单签**:配置自检只验当前密钥可达 + 当前签名,无需双签;`deliver_payload` 的 previous 传 None。
- **D7 request_signature 快照存实际多签名头**:与 add-notification-delivery-payload-log 一致,详情里能看到当次发的是单签还是双签。

## 影响面

```text
entity   webhook_endpoint.rs   +2 列(previous_secret_encrypted/expires_at) + view From 加 expires_at
api-types notification.rs       WebhookEndpoint view + previous_secret_expires_at
server   notify/channel.rs      DeliveryTarget::Webhook + previous_secret;deliver_payload 双签
         notify/worker.rs       delivery_request 宽限期内解密 previous
         routes/notifications.rs rotate_webhook_secret 写 previous + expires;test 传 None
admin    routes/.../notifications/index.tsx  Endpoints 列显宽限 Tag;轮换确认文案改双签
cli      commands/notifications.rs  endpoint row 显宽限到期(可选)
tests    app_release_smoke      轮换→宽限期内新旧都验签;过期只新签名
```

## Risks / Trade-offs

- 宽限期内每投递多算一次 HMAC + header 变长 → 可忽略。
- 再次轮换覆盖 previous → 若 24h 内连轮两次,最早那把提前失效;运维罕见,接受。
- 生产需 deployer ALTER(不自动)→ proposal Non-goal + docs 写明。
