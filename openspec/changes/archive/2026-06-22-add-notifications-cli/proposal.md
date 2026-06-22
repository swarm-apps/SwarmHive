## Why

`add-notifications` 落地了通知后端 + 11 endpoint,`add-notifications-page-ui` 补了 Web 管理页。但 `add-notifications` proposal 当时写明「CLI 不碰(MVP)」。SwarmHive 的 **CLI 是一等发布入口**(CI/CD 复用同一二进制),通知配置作为「provision-as-code / CI bootstrap」也应能在 CLI 完成,镜像已有的 `mail` / `tokens` / `storage` CLI admin。本 change 补 `swarmhive notifications` 子命令——纯 CLI,只依赖 api-types,**零后端、零前端**。

## What Changes

- 新增顶层 `swarmhive notifications` 命令,三组嵌套子命令(命令树与 11 endpoint 一一对应):
  - **`endpoints {list,create,update,delete,rotate-secret,test}`** —— webhook endpoint 管理。`create` / `rotate-secret` 一次性打印 `whsec_` 签名密钥(`emit_ack`,镜像 `tokens create`);`update` 走 PATCH(`--name`/`--url`/`--disable`/`--enable`);`delete` 需 `--yes`;`test` 发 `webhook.test`(不入库)。
  - **`subscriptions {list,create,delete}`** —— `--event`(release.published 等)/`--channel`(email|webhook)走 `parse_enum`;`--to <addr>`(email)/`--endpoint <id|name>`(webhook)互斥必填其一;`--app <slug>` 解析成 app_id(留空=所有 app);`delete --id --yes`。
  - **`deliveries {list,redeliver}`** —— 投递日志(`--endpoint <id|name>` / `--status` / `--limit` 过滤,对齐后端 `DeliveriesQuery`)+ `redeliver --id`(保持原 webhook-id)。
- endpoint 用 `--endpoint <id|name>` 寻址(`resolve_unique`,镜像 mail 的 `--provider`)。
- 复用既有 client helper(`get_json`/`post_json`/`patch_json`/`post_empty_json`/`delete_no_content` + `emit`/`emit_one`/`emit_ack`);`--output json|table` 全局生效。`list` 表格把 endpoint/app id 解析回 name/slug。

## Acceptance

- `cargo build -p swarmhive-cli` / `cargo clippy -p swarmhive-cli --all-targets -- -D warnings` 绿。
- `cargo fmt --all --check` 绿;`cargo test --workspace` 绿。
- `swarmhive notifications --help` 与三组子命令 `--help` 正常输出,命令树与 11 endpoint 一一对应。

## Non-goals

- ❌ 后端 / 前端任何改动(纯消费既有 endpoint)。
- ❌ 新增 CLI 依赖:只用 api-types + 现有 `commands::client` helper;**不引 uuid 直接 dep**——endpoint/app id 从解析到的对象(`.id` 已是 `Uuid`)取。
- ❌ 交互式 wizard:`create` 全 flag 驱动(CI 友好);webhook 签名密钥由 server 生成、CLI 只负责打印,不读 secret 输入。
- ❌ 一次性密钥的可反复查看:同 Web,仅 `create` / `rotate-secret` 打印一次。

## Depends on

- `add-notifications`(✅ server API + api-types notification DTO)。
- `add-cli-storage-mail-admin`(✅ 复用 `client` helper + `emit_ack` + `resolve_unique` 范式)、`add-cli-publish-polish`(✅ `--output json` 契约)。

## Maps to docs

- `docs/12-cli.md`(CLI 命令面补 notifications)+ `docs/15-notifications.md`(补「CLI 管理」段)。
- 更新 `openspec/changes/README.md` 依赖图 + `dev-notes/knowledge/project-notifications.md`「后续」勾掉 cli。
