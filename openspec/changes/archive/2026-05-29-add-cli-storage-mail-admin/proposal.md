# add-cli-storage-mail-admin

## Why

继 `add-cli-management-commands`(发布线)之后,把 CLI 的管理面延伸到**配置线**:storage(对象存储后端)与 mail(SMTP provider / 模板 / 日志)。动机同前——用户会让 **AI 帮忙配置**(如"帮我接上 OSS"、"配好邀请邮件模板"),AI 走 CLI / 脚本最自然。storage 的 DTO 已在 api-types,纯接线;mail 的 DTO 目前内联在 `routes/mail.rs` 且字段用 entity 枚举,CLI(不依赖 entity/sea-orm)用不了——需先把 mail DTO **提升到 api-types**(项目「单一共享 DTO 层」约定),再接 CLI。

## What Changes

- **storage CLI**(扩现有 `storage` 命令组,已有 `init rustfs`):`get` / `create` / `update` / `test` / `activate` / `cors`。
- **mail DTO 提升到 api-types**(`add-mail-infrastructure` 的内联 DTO 重构):
  - 新建 `api-types/src/mail.rs`:`MailProviderView` / `CreateProviderReq` / `UpdateProviderReq` / `MailTemplateView` / `UpdateTemplateReq` / `PreviewReq` / `PreviewResp` / `MailLogView` / `MailStatusResp` / `TouchedResp` + 枚举 `ProviderKind` / `SmtpEncryption` / `MailLogStatus`(纯 serde,`rename_all="lowercase"`,与现有 wire 完全一致)。
  - `routes/mail.rs` 改为 `use swarmhive_api_types::...`,删除内联 struct/enum。
  - entity 加 `From<&Model>` / `From<entity enum>` → api-types 的转换(转换归属 entity crate)。
- **mail CLI**(新 `mail` 命令组):`providers {list, create, update, activate, delete, test}`、`templates {list, get, set, preview, restore-defaults}`、`logs {list}`、`status`。
- **密钥处理**(storage / mail 共用):secret(S3 `access_key_secret` / SMTP `password`)三路输入 —— `--access-key-secret` / `--password` 明文 flag **或** env(`SWARMHIVE_STORAGE_SECRET` / `SWARMHIVE_MAIL_PASSWORD`)**或** `--secret-stdin`(管道);docs 标注「AI / skill 走 env/stdin,明文 flag 会进 history/ps/日志」。update 省略 = 保留(沿用 web「留空不改」)。
- **多行模板**:`templates set --subject … --html-file invite.html --text-file invite.txt`(正文走文件)。
- 复用 `add-cli-management-commands` 的 `--output json` / problem+json / 非零 exit / 全非交互契约。

## Capabilities

### New Capabilities
- `storage-cli-admin`: CLI 管理 S3 storage backend(get/create/update/test/activate/cors)+ 密钥三路输入。
- `mail-cli-admin`: CLI 管理 mail provider / template / log(含 mail DTO 提升到 api-types 后才可能)。

### Modified Capabilities
（无 —— mail DTO 从 server 内联迁到 api-types 是**纯实现重构**,wire / 路由 / 行为不变,不改 `mail-infrastructure` 的任何 spec 需求;捕捉在 design + tasks。）

## Impact

- **api-types**:新增 `mail` 模块(DTO + 3 枚举)。storage 模块无改动(已够用)。
- **entity**:`mail_provider` / `mail_template` / `mail_log` 加 `From<&Model> for api::*View` + enum 转换(原在 `routes/mail.rs` 的 `From` 迁来)。
- **server**:`routes/mail.rs` 改为 import api-types DTO、删内联定义;行为 / 路由 / wire **不变**。
- **swarmhive-cli**:`commands/storage.rs` 扩管理子命令;新 `commands/mail.rs`;`main.rs` 命令树扩 `StorageCommand` + 新 `MailCommand`;`commands/client.rs` 加共享 `resolve_secret`(flag/env/stdin)helper(若 management proposal 未覆盖 patch/delete 则一并加)。
- **docs**:`docs/07-storage-and-delivery.md`(CLI 配 storage)、`docs/08-admin-and-analytics.md`(CLI 配 mail)、`docs/12-cli.md`(命令清单 + 密钥输入约定);`dev-notes/knowledge/backend.md`(mail DTO 提升)+ `admin-spa.md`(若 schema.gen.ts 因枚举收紧而变,记一笔)。
- **测试**:api-types mail enum round-trip 单测;mail DTO 迁移后 `openapi_surface` 仍绿(wire 不变);CLI storage/mail 链路 e2e(testcontainers postgres + MinIO + mailpit,或扩既有 smoke)。
- **schema.gen.ts**:mail enum 从 `value_type=String` 变成真枚举可能**收紧** admin 端类型(string → 字面量联合);需 regen 并确认 admin typecheck 仍过(预期更严更好)。

## Non-goals

- **不依赖 `add-cli-management-commands` 的实现**做前提,但**复用其约定**(json/错误/非交互);若两者并行,共享 helper 谁先落谁建,另一个引用。
- **不做 users / tokens CLI**(仍只在 web)。
- **不做 MCP**(AI 操作 = AI 友好 CLI + 后续配套 skill)。
- **不重写 mailer / 模板渲染 / storage trait**:只迁 DTO + 接 CLI,不动 `mail::{Mailer,TemplateEngine}` 或 `storage::{mod,s3}` 业务。
- **不加 mail provider 的"清空密码"**:沿用现有「省略=保留」语义(清空走未来专用端点)。

## Depends on

- `add-mail-infrastructure`(已归档)—— mail 端点 + 内联 DTO(本提案迁移其 DTO)。
- `add-storage-and-presign-upload`(已归档)—— storage 端点 + api-types storage DTO + `cors` 端点(`add-web-artifact-upload`)。
- `add-cli-management-commands`—— 复用 json/错误/非交互契约 + client helper(并行或先后均可)。

## Maps to docs

- `docs/07-storage-and-delivery.md` —— storage 后端配置 + CORS。
- `docs/08-admin-and-analytics.md` —— Mail Provider / Templates / Log。
- `docs/12-cli.md` —— CLI 命令设计 + 密钥输入约定。
