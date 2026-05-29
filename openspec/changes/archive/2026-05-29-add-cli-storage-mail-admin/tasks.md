# tasks — add-cli-storage-mail-admin

storage CLI 零后端改动;mail 先把 DTO 提到 api-types(server+entity 重构),再接 CLI。`[code]`/`[test]`/`[docs]`。

## 1. api-types:mail 模块

- [x] 1.1 [code] 新建 `api-types/src/mail.rs`:枚举 `ProviderKind` / `SmtpEncryption` / `MailLogStatus`(纯 serde,`#[serde(rename_all="lowercase")]` + `ToSchema`,wire 对齐 `smtp`/`starttls`/`tls`/`none`/`sent`/`failed`)。
- [x] 1.2 [code] `mail.rs` 加 DTO:`MailProviderView` / `CreateProviderReq` / `UpdateProviderReq` / `MailTemplateView` / `UpdateTemplateReq` / `PreviewReq` / `PreviewResp` / `MailLogView` / `MailStatusResp` / `TouchedResp`;`lib.rs` `pub mod mail` + re-export。
- [x] 1.3 [test] enum `serde_json` round-trip 单测(锁 lowercase wire,防 DeriveActiveEnum+serde 分叉坑)。

## 2. entity:From 转换迁移

- [x] 2.1 [code] `mail_provider`:`From<entity::ProviderKind> for api::ProviderKind`、`From<entity::SmtpEncryption> for api::SmtpEncryption`、`From<&Model> for api::MailProviderView`。
- [x] 2.2 [code] `mail_template`:`From<&Model> for api::MailTemplateView`;`mail_log`:`From<entity::MailLogStatus> for api::MailLogStatus` + `From<&Model> for api::MailLogView`。

## 3. server:routes/mail.rs 消费 api-types

- [x] 3.1 [code] 删 `routes/mail.rs` 内联 struct/enum + 其 `From<&Model>`;改 `use swarmhive_api_types as api`,handler 签名引用 `api::*`;路由 / 鉴权 / 行为不变。
- [x] 3.2 [code] regen openapi:`pnpm --filter @swarm-hive/admin openapi`;确认 `openapi_surface` 仍绿;schema.gen.ts diff 仅枚举收紧(取舍 A)或为空(回落 B)。
- [x] 3.3 [code] 若枚举收紧致 admin typecheck break → 评估回落 `#[schema(value_type=String)]`(design D2);确认 `pnpm --filter @swarm-hive/admin typecheck` 过。

## 4. CLI:共享密钥输入

- [x] 4.1 [code] `commands/client.rs` 加 `resolve_secret(flag, env_key, stdin_flag, interactive_ok)`:优先级 `--secret-stdin` > env > 明文 flag > (create 且 TTY)交互 prompt;非 TTY 不 prompt。

## 5. CLI:storage 管理命令

- [x] 5.1 [code] `commands/storage.rs` 扩 `get` / `create` / `update`(secret 走 resolve_secret;update 省略=不带)/ `test` / `activate` / `cors`(`--origin` 可选,默认无则报错提示);`--backend <id|name>` name 走 list 解析。
- [x] 5.2 [code] `main.rs` `StorageCommand` 扩这些子命令 + 接线。

## 6. CLI:mail 管理命令

- [x] 6.1 [code] 新 `commands/mail.rs`:`providers {list,create,update,activate,delete --yes,test}`(password 走 resolve_secret)、`templates {list,get,set(--subject/--html-file/--text-file),preview(--sample-file),restore-defaults}`、`logs {list --limit}`、`status`。
- [x] 6.2 [code] `main.rs` 新 `MailCommand` + `Command::Mail` 接线。

## 7. 校验 + docs

- [x] 7.1 [test] gates:`cargo fmt` / `cargo clippy --workspace --all-targets -D warnings` / `cargo test --workspace`(含 openapi_surface + mail round-trip)/ `cargo build -p swarmhive-cli`;`cargo tree -p swarmhive-cli | grep sea-orm` 必须空;`pnpm --filter @swarm-hive/admin typecheck` + schema.gen.ts 已 regen 入提交。
- [x] 7.2 [test] e2e:api-types mail enum round-trip 单测(锁 lowercase wire)+ `openapi_surface`(wire/surface 不破)+ 命令树 `--help` 验证。**CLI-binary 链路 e2e deferred**(同 add-cli-management-commands:bin crate 不可 import + 需真实 server+MinIO+mailpit 进程,无现成 harness);endpoint 行为由既有 mail/storage in-process 测试覆盖。
- [x] 7.3 [docs] `docs/07`(CLI 配 storage)、`docs/08`(CLI 配 mail)、`docs/12`(命令清单 + 密钥三路输入 + 明文 flag 泄露红字);`dev-notes/knowledge/backend.md`(mail DTO 提升到 api-types)+ `admin-spa.md`(若枚举收紧致 schema.gen.ts 变);`memory/project-cli-surface.md` 命令清单更新。
- [x] 7.4 [docs] `openspec/changes/README.md` 进度表更新 `add-cli-storage-mail-admin`。
