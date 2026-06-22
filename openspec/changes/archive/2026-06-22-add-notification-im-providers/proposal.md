## Why

通知子系统当前把飞书/Slack/钉钉/Discord 当「通用 webhook URL」——它们收到的是 SwarmHive 自有的 Standard Webhooks JSON({id,type,source,time,data}),在聊天里显示成一坨不可读的原始 JSON。`add-notifications` proposal 明确把「各家专用 bot 加签 + 消息格式适配」单列为本 change(需子调研,已完成)。本 change 给 webhook endpoint 加 `provider_kind`,为 4 个 IM 平台产出**平台原生的可读消息** + 各自的**加签/鉴权**。

## What Changes

- **entity `webhook_endpoint`** + `provider_kind`(enum:`generic`/`feishu`/`slack`/`dingtalk`/`discord`,默认 `generic`,schema-sync 加列默认值 / 生产 ALTER)。
- **secret 语义按 kind**:`generic` 仍由 SwarmHive 生成 `whsec_` 并一次性返回(不变);IM 平台的 `secret` 是**用户提供的可选加签密钥**(飞书/钉钉),创建时存入 `secret_encrypted`(Slack/Discord 无签名,可空)。`CreateWebhookEndpointReq` + `provider_kind` + 可选 `secret`;`CreateWebhookEndpointResp.secret` 对 IM 为空(无 SwarmHive 密钥可揭示)。
- **channel**(`notify/`):`WebhookChannel::deliver` 按 `provider_kind` 分叉:
  - `generic` → 现有 Standard Webhooks(whsec_ 双签 + 3 头 + 原始事件 JSON + 快照),不变。
  - `feishu` → interactive 卡片(语义色 header + KV fields + notes + 跳转按钮);可选加签(HMAC key=`{ts}\n{secret}` 签空串→base64,`timestamp`/`sign` 入 body);success 看 body `code==0`。
  - `slack` → Block Kit(header + fields section + notes + context);无签名;success 看 HTTP 200 && body=="ok"(纯文本)。
  - `dingtalk` → markdown(标题 + KV + notes 引用 + 回链);可选加签(HMAC key=secret 签 `{ts_ms}\n{secret}`→base64→urlencode,`timestamp`/`sign` 入 URL query);success 看 body `errcode==0`。
  - `discord` → embed(语义色 + fields + footer + timestamp);无签名;success 看 HTTP 2xx(204)。
  - IM 路径同样捕获 delivery 快照(request_body/timestamp/signature/response_body)。
- **worker**:`delivery_request` 读 `provider_kind` + 解密 secret(可空),放进 `DeliveryTarget::Webhook`。
- **rotate-secret**:仅 `generic`(轮换 whsec_);IM endpoint 调用返回 422「rotation applies to generic webhooks only」。
- **api-types `WebhookEndpoint` view** + `provider_kind`。
- **admin**:创建表单 provider 下拉 + 条件 secret 输入(飞书/钉钉显示可选「加签密钥」,Slack/Discord/generic 隐藏);一次性 reveal 仅 generic;列表 provider 标签。
- **cli**:`endpoints create --provider <kind> [--secret/--secret-stdin]`;list 显示 provider 列。

## Acceptance

- `cargo build --workspace` / `clippy --workspace --all-targets -- -D warnings` / `fmt --check` / `cargo test --workspace` 绿。
- notification smoke 新增:feishu endpoint(带 sign secret)投递 → wiremock 断言 body 含 `msg_type:"interactive"` + 正确的 `sign`(用同算法重算比对) + success(mock 回 `{"code":0}`);slack endpoint 投递 → 断言 body 含 `blocks` + 无签名头 + success(mock 回 200 "ok")。
- `openapi_surface` 通过;admin `typecheck`/`lint`/`build`/`vitest`,`schema.gen.ts` 含 `provider_kind`;CLI `--help`。

## Non-goals

- ❌ 每个 IM 错误码细分 retryable/permanent:MVP 一律按 success 判定失败即可重试(配置错由重试预算 + dead 兜底)。
- ❌ @人 / 关键词注入 / 富交互按钮回调:MVP 用 url 跳转 + 不 @;飞书/钉钉若群里设了关键词安全,用户需改用加签或 IP 白名单(docs 说明)。
- ❌ 限流精调(Discord 429 retry_after / 钉钉 20/min):靠现有指数退避兜底,不读 Retry-After 精调。
- ❌ QQ / 企业微信 / Teams:本 change 只 4 家;其余后续。
- ❌ IM secret 的 rotate / 一次性 reveal:IM secret 是用户自有,创建时设,改用 delete+recreate(MVP);update-secret 后续。
- ❌ 生产自动 ALTER:dev schema-sync;生产 deployer `ALTER TABLE webhook_endpoint ADD COLUMN provider_kind ...`。

## Depends on

- `add-notifications`(✅ webhook endpoint + channel + signer)、`add-notification-delivery-payload-log`(✅ 快照)、`add-notification-secret-rotation-grace` / `add-notification-endpoint-auto-disable`(✅ webhook_endpoint 加列范式)。

## Maps to docs

- `docs/15-notifications.md`(新增「IM provider」段:4 家配置 + 加签 + 关键词注意)。
- 更新 `openspec/changes/README.md` + `dev-notes/knowledge/project-notifications.md`。
