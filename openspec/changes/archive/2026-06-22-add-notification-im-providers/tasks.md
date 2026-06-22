# Tasks — add-notification-im-providers

## 1. api-types [code]

- [x] 1.1 `notification.rs`:`ProviderKind` 枚举(generic/feishu/slack/dingtalk/discord,lowercase);`WebhookEndpoint` view + `provider_kind`;`CreateWebhookEndpointReq` + `provider_kind`(default generic) + `secret`(可选,IM 加签密钥);lib 导出。

## 2. entity [code]

- [x] 2.1 `webhook_endpoint.rs`:`ProviderKind` DeriveActiveEnum(default generic)+ Model `provider_kind` 列;`From<&Model>` 映射 provider_kind。

## 3. providers 模块(纯函数) [code]

- [x] 3.1 `notify/providers.rs`:`build_feishu_body` / `build_slack_body` / `build_dingtalk_body` / `build_discord_body`(event → `serde_json::Value`,语义色 + KV + notes 截断 + 回链);`sign_feishu(ts,secret)` / `sign_dingtalk(ts_ms,secret)`;`is_im_success(kind, status, body) -> bool`;notes 截断 helper。
- [x] 3.2 单测:feishu/dingtalk sign 算法(对官方/自算向量)、is_im_success 各平台、消息体含关键字段。

## 4. channel 分叉 [code]

- [x] 4.1 `notify/channel.rs`:`DeliveryTarget::Webhook` + `provider_kind`;`deliver` match——generic 走现有 deliver_payload,IM 走 `deliver_im`(render → 注入 sign(feishu body / dingtalk query) → POST → read capped body → is_im_success → Ok(outcome 带快照)/Err(failure))。

## 5. worker [code]

- [x] 5.1 `notify/worker.rs::delivery_request`:读 `endpoint.provider_kind`;secret 解密改可空(IM 无 secret 时 None);放进 target。previous_secret 仅 generic 有意义(IM 跳过双签)。

## 6. routes [code]

- [x] 6.1 `create_webhook_endpoint`:按 provider_kind——generic 生成 whsec_ 一次性返回(现有);IM 存 `req.secret`(可空)加密,resp.secret 空。ActiveModel 写 provider_kind。
- [x] 6.2 `rotate_webhook_secret`:非 generic 返回 422 Validation。

## 7. admin / cli [code]

- [x] 7.1 admin `index.tsx`:DrawerForm provider `ProFormSelect`(默认 generic)+ `ProFormDependency` 按 kind 显示可选「加签密钥」(feishu/dingtalk)/ 隐藏(slack/discord/generic);create 成功仅 generic 弹 reveal;列表 provider 列/标签。i18n。
- [x] 7.2 cli `commands/notifications.rs`:`EndpointsCommand::Create` + `--provider`(parse_enum)+ `--secret`/`--secret-stdin`(resolve_secret);EndpointRow + provider 列。

## 8. 测试 [test]

- [x] 8.1 notification smoke:feishu endpoint(带 sign secret)+ wiremock(matcher 校验 body `msg_type==interactive` + `sign` 用同算法重算匹配,回 `{"code":0}`)→ 投递 sent;slack endpoint + wiremock(校验 body 有 `blocks`、无 webhook-signature 头,回 200 "ok")→ sent。

## 9. 验收 gates [test]

- [x] 9.1 `cargo build --workspace` / `clippy -D warnings` / `fmt --check` / `cargo test --workspace`(notification smoke + providers 单测 + openapi_surface)绿;admin `typecheck`/`lint`/`build`/`vitest`;`schema.gen.ts` 含 `provider_kind`;CLI `--help`。

## 10. docs / 同步 [docs]

- [x] 10.1 `docs/15-notifications.md`(IM provider 段:4 家配置 + 加签 + 关键词注意);`openspec/changes/README.md`;`dev-notes/knowledge/project-notifications.md`。
