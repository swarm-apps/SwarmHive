## Context

`add-notifications` 暴露 3 对象 / 11 endpoint(全部 `notification:manage`),api-types 根导出全部 notification DTO。CLI 只依赖 api-types(不依赖 entity / sea-orm),复用 `commands::client` 的 HTTP + 输出 helper。本 change 纯 CLI,不跨 crate 边界、零新依赖。

## 命令树(与 11 endpoint 一一对应)

```text
swarmhive notifications
├── endpoints
│   ├── list                                   GET    /webhook-endpoints
│   ├── create   --name --url                  POST   /webhook-endpoints        → whsec_ 一次性
│   ├── update   --endpoint <id|name>          PATCH  /webhook-endpoints/{id}
│   │             [--name --url --disable|--enable]
│   ├── delete   --endpoint <id|name> --yes    DELETE /webhook-endpoints/{id}
│   ├── rotate-secret --endpoint <id|name>     POST   /webhook-endpoints/{id}/rotate-secret → whsec_ 一次性
│   └── test     --endpoint <id|name>          POST   /webhook-endpoints/{id}/test (不入库)
├── subscriptions
│   ├── list                                   GET    /subscriptions
│   ├── create   --event --channel             POST   /subscriptions
│   │             [--to | --endpoint] [--app]
│   └── delete   --id --yes                     DELETE /subscriptions/{id}
└── deliveries
    ├── list     [--endpoint --status --limit] GET    /deliveries?webhook_endpoint_id&status&limit
    └── redeliver --id                          POST   /deliveries/{id}/attempts (保持原 webhook-id)
```

## 数据流

```text
  swarmhive notifications ...
      │  require_creds()(SWARMHIVE_TOKEN / credentials.toml)+ Bearer
      │  get_json / post_json / patch_json / post_empty_json / delete_no_content
      ▼
  /api/v1/notifications/*  (既有 server endpoint, notification:manage)
      ▼
  emit(list 表格/JSON) · emit_one(单对象) · emit_ack(一次性 secret / 删除 ack)
```

## Decisions

- **D1 镜像 mail/tokens 的命令模块范式**:嵌套 `Subcommand` 枚举 + `run()` 分发 + 每类一个 `#[derive(Tabled)]` row;复用 `commands::client` 全部 helper,不新写 HTTP 代码。
- **D2 endpoint 用 `--endpoint <id|name>` 寻址**(`resolve_unique`,镜像 mail `--provider`):name 精确或 id 字符串匹配,0/多个 → 错误。subscription/delivery 无 name,用 `--id <uuid>`。
- **D3 枚举参数走 `parse_enum`(serde 反序列化 wire 串)**:`--event release.published`、`--channel email`、`--status dead`,不给 api-types 加 clap `ValueEnum` 依赖(保边界);`--help` 文档注明取值集。
- **D4 一次性 secret 用 `emit_ack`**:`create` / `rotate-secret` 把 `whsec_` 明文打印一次(JSON 给整个响应体,table 给人话 + "shown only once"),镜像 `tokens create`。
- **D5 不引 uuid 直接依赖**:`CreateSubscriptionReq.app_id` / `webhook_endpoint_id` 是 `Uuid`,但其值从解析到的 `WebhookEndpoint` / `App` 的 `.id`(已是 `Uuid`)取,无需在 CLI 模块构造 Uuid → 不加 dep。
- **D6 list 表格解析友好名**:subscriptions/deliveries 的 endpoint id → name、app id → slug(各多拉一次 list 建查找);JSON 输出仍是原始 DTO(含 id,可脚本化)。**仅 list 解析友好名**——`create` / `redeliver` 的单对象回显显示裸 id(与 mail.rs `create` 不解析友好名的范式一致,且避免为单对象多发 GET);解析不到 name(endpoint 已删)时回退裸 id 而非空串,不丢信息。
- **D7 危险/破坏操作沿用 `--yes`**:`delete`(endpoint/subscription)需 `--yes`(镜像 apps/tokens delete);`rotate-secret` 无需 `--yes`(打印一次即生效,与 Web 一致,不二次确认)。

## Risks / Trade-offs

- `subscriptions list` / `deliveries list` 为解析友好名各多发 1-2 个 GET(endpoints/apps);量小,可接受;JSON 路径不受影响(原始 DTO)。
- `--event` 取 dotted wire 串(`release.published`)略不直观,但与 API / Web / docs 一致,且 `--help` 列出取值集。
