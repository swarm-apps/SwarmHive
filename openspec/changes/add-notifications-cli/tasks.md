# Tasks — add-notifications-cli

## 1. 命令模块 [code]

- [x] 1.1 `commands/notifications.rs`:三组嵌套 `Subcommand` 枚举(`EndpointsCommand` / `SubscriptionsCommand` / `DeliveriesCommand`)+ `NotificationsCommand` 顶层 + `run()` 分发。
- [x] 1.2 endpoints:`list`(emit + EndpointRow)/ `create`(post_json → CreateWebhookEndpointResp,emit_ack 一次性 whsec_)/ `update`(resolve_endpoint + patch_json,`--disable`/`--enable` 互斥)/ `delete`(--yes + delete_no_content + emit_ack)/ `rotate-secret`(post_empty_json → RotateSecretResp,emit_ack)/ `test`(post_empty_json → WebhookEndpointTestResp,emit_one)。
- [x] 1.3 subscriptions:`list`(emit,表格解析 endpoint name / app slug)/ `create`(parse_enum event+channel,email/webhook 互斥校验,resolve_app slug→id,resolve_endpoint,post_json)/ `delete`(--yes + delete_no_content)。
- [x] 1.4 deliveries:`list`(拼 `?webhook_endpoint_id&status&limit` 查询,resolve_endpoint 过滤,parse_enum status,表格解析 endpoint name)/ `redeliver`(post_empty_json → Delivery,emit_one)。
- [x] 1.5 helper:`resolve_endpoint`(resolve_unique by name|id)、`resolve_app`(by slug)、`wire_str`(serde 序列化枚举 → wire 串,供表格展示)。

## 2. 注册 [code]

- [x] 2.1 `commands/mod.rs` 加 `pub mod notifications;`。
- [x] 2.2 `main.rs`:`Command` 枚举加 `Notifications { #[command(subcommand)] command: commands::notifications::NotificationsCommand }`;`dispatch()` 加 `Command::Notifications { command } => commands::notifications::run(command, output).await?`。

## 3. 验收 gates [test]

- [x] 3.1 `cargo build -p swarmhive-cli` + `cargo clippy -p swarmhive-cli --all-targets -- -D warnings` 绿。
- [x] 3.2 `cargo fmt --all` + `cargo test --workspace` 绿。
- [x] 3.3 `swarmhive notifications --help` 与 `endpoints`/`subscriptions`/`deliveries --help` 输出正常,命令树与 11 endpoint 对应。

## 4. docs / 同步 [docs]

- [x] 4.1 `docs/12-cli.md` 补 notifications 命令;`docs/15-notifications.md` 补「CLI 管理」段;`CLAUDE.md` 的 CLI 子命令清单顺带加 notifications。
- [x] 4.2 `openspec/changes/README.md` 依赖图 changes 表加本 change;`dev-notes/knowledge/project-notifications.md`「后续」勾掉 cli。
