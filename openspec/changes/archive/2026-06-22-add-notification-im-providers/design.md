## Context

`WebhookChannel` 当前只走 Standard Webhooks(whsec_ HMAC + 3 头 + 原始事件 JSON,success 看 HTTP 2xx)。4 个 IM 平台各自的加签、消息体、success 判定全不同(子调研已完成)。本 change 加 `provider_kind` 列,channel 按 kind 分叉;IM 路径产出平台原生消息体并按平台规则判 success。约束:schema-sync 加列(dev)/ deployer ALTER(prod);跨 entity / server / api-types / admin / cli。消息体用 `serde_json::json!` 紧凑构建(避免为每家 card/blocks/embed schema 定义大量 typed struct)。

## 各平台契约(子调研结论)

```text
              加签                                  放哪      success 判定           消息体
generic   whsec_ HMAC(双签/快照,现有)           3 个 header  HTTP 2xx              Standard Webhooks JSON
feishu    HMAC key="{ts}\n{secret}" 签空串→b64    body 顶层    body code==0(HTTP恒200) interactive 卡片
slack     无(URL 即 secret)                      —           HTTP 200 && body=="ok"  Block Kit blocks
dingtalk  HMAC key=secret 签"{ts_ms}\n{secret}"   URL query   body errcode==0(恒200)  markdown
          →b64→urlencode
discord   无(token 在 URL)                       —           HTTP 2xx(204)          embeds
```

## 数据流

```text
  worker.delivery_request → DeliveryTarget::Webhook { url, secret(可空), previous_secret, provider_kind }
      ▼
  WebhookChannel::deliver  match provider_kind:
      ├─ Generic → deliver_payload(Standard Webhooks,现有,不变)
      └─ Feishu/Slack/Dingtalk/Discord:
            providers::render(kind, &event, secret) → ImRequest { url, body }
              (feishu/dingtalk 把 ts/sign 注入 body / query;消息体 json! 构建,notes 截断)
            POST url + body(application/json) → read(status, capped body)
            providers::is_success(kind, status, &body) → Ok(outcome) / Err(failure)
            outcome/failure 带快照(request_body=发送的 JSON、response_body=平台响应)
```

## Decisions

- **D1 `provider_kind` enum 列,默认 generic**:DeriveActiveEnum string_value;现有行 schema-sync 默认 generic(NOT NULL DEFAULT 'generic')。
- **D2 secret 语义按 kind**:generic = SwarmHive 生成 whsec_(一次性 reveal,现有);feishu/dingtalk = 用户提供的加签密钥(创建时存,可空 = 不加签);slack/discord = 不用(URL 即凭据)。`CreateWebhookEndpointResp.secret` 对 IM 为空字符串。
- **D3 channel 分叉而非全替换**:generic 完整保留 deliver_payload(双签 + 快照 + 现有 success);IM 走独立 `deliver_im`。`DeliveryTarget::Webhook` 加 `provider_kind`。
- **D4 消息体 json! 构建**:每家一个 `build_<kind>_body(event) -> Value`,语义色按 event_type(published=绿/promoted=蓝/rolled_back=红);notes 截断(飞书 30KB / slack section 3000 / dingtalk / discord field 1024);时间 rfc3339 / 本地化串。
- **D5 success 判定按平台**:feishu/dingtalk 解析 body code/errcode==0;slack HTTP 200 && body trim=="ok";discord/generic HTTP 2xx。失败一律 retryable(MVP 不细分 config 错,见 Non-goal)。
- **D6 IM 加签现算**:feishu/dingtalk timestamp 现取(时间窗 1h),每次发送重算 sign,不缓存;复用 `hmac`+`sha2`。feishu 签空串(HMAC key=`{ts}\n{secret}`),dingtalk 签 `{ts_ms}\n{secret}`(HMAC key=secret)→ base64 → dingtalk 再 urlencode 入 query。
- **D7 rotate 仅 generic**:IM endpoint rotate 返回 422 Validation;IM secret 改用 delete+recreate(MVP)。
- **D8 快照统一**:IM 投递也填 DeliveryOutcome/Failure 的 request_body(发送的平台 JSON)/ request_timestamp / request_signature(feishu/dingtalk 的 sign,slack/discord None)/ response_body。

## 影响面

```text
entity   webhook_endpoint.rs   + provider_kind enum 列 + view From
api-types notification.rs       ProviderKind 枚举 + WebhookEndpoint view + CreateReq{provider_kind,secret} + lib 导出
server   notify/providers.rs    新模块:ProviderKind→消息体 json! 构建 + 加签 + success 判定(纯函数,可单测)
         notify/channel.rs      DeliveryTarget::Webhook + provider_kind;deliver 分叉 generic vs deliver_im
         notify/worker.rs       delivery_request 读 provider_kind + 可空 secret
         routes/notifications.rs create 按 kind(generic 生成 whsec_ / IM 存用户 secret);rotate 仅 generic
admin    notifications/index.tsx provider 下拉 + 条件 secret 输入 + reveal 仅 generic + provider 标签
cli      commands/notifications.rs endpoints create --provider/--secret;list provider 列
tests    app_release_smoke      feishu(sign 重算比对)+ slack(blocks + 无签名)投递 smoke;providers 纯函数单测
```

## Risks / Trade-offs

- IM 失败一律 retryable → 配置错(关键词/IP/sign)会重试到 dead 才停;可接受(dead + UI 可见),细分留后续。
- notes 截断按各家上限 → 长 notes 丢尾,加「…」标记。
- 飞书/钉钉群设了关键词安全且只发卡片/markdown 可能命不中关键词 → docs 提示用加签或在 notes 含关键词。
- 生产需 deployer ALTER(默认 generic)→ proposal Non-goal + docs。
