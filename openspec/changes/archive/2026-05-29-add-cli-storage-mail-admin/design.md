# design — add-cli-storage-mail-admin

跨 crate(api-types + entity + server + cli),按约定画数据流。聚焦 mail DTO 提升、密钥三路输入、命令↔端点映射、schema 稳定性。

## 命令 ↔ 端点映射(端点全部已存在)

```text
  swarmhive-cli                                  swarmhive-server (HTTP)
  ┌────────────────────────────────┐
  │ storage get      --backend <id|name>──GET   /api/v1/storage/backends → 本地按 id/name 过滤
  │         create  … [secret 三路] │──POST   /api/v1/storage/backends
  │         update  --backend … [secret?]──PATCH /api/v1/storage/backends/{id}   (secret 省略=保留)
  │         test    --backend       │──POST   /api/v1/storage/backends/{id}/test
  │         activate --backend      │──POST   /api/v1/storage/backends/{id}/activate
  │         cors    --backend [--origin …]──POST /api/v1/storage/backends/{id}/cors
  │ mail providers list             │──GET    /api/v1/mail/providers
  │      providers create … [pw 三路]──POST   /api/v1/mail/providers
  │      providers update --id … [pw?]──PUT   /api/v1/mail/providers/{id}
  │      providers activate --id    │──POST   /api/v1/mail/providers/{id}/activate
  │      providers delete --id --yes│──DELETE /api/v1/mail/providers/{id}
  │      providers test --id        │──POST   /api/v1/mail/providers/{id}/test
  │ mail templates list             │──GET    /api/v1/mail/templates
  │      templates set --id --subject --html-file --text-file──PUT /api/v1/mail/templates/{id}
  │      templates preview --id --sample-file──POST /…/templates/{id}/preview
  │      templates restore-defaults │──POST   /api/v1/mail/templates/seed-defaults
  │ mail logs list [--limit]        │──GET    /api/v1/mail/logs
  │ mail status                     │──GET    /api/v1/mail/status
  └────────────────────────────────┘
   storage backend / mail provider / template 都按 id 寻址;CLI 允许 --backend/--provider 传 name → list 后本地解析成 id。
```

## Goals / Non-Goals

**Goals:** CLI 配 storage + mail 与 web 对齐;mail DTO 提升到 api-types(单一源);密钥不进命令串的安全输入;wire / openapi 尽量稳定。
**Non-Goals:** users/tokens、MCP、重写 mailer/storage 业务、清空密码。

## Decisions

### D1. mail DTO 提升到 api-types(本提案的后端核心)

```text
现状                                   目标
routes/mail.rs:                        api-types/src/mail.rs:  (纯 serde + ToSchema,无 sea-orm)
  struct MailProviderView{ kind: entity枚举 }  ←迁→  struct MailProviderView{ kind: api::ProviderKind }
  enum ProviderKind/SmtpEncryption(在 entity)  ←镜像→ enum ProviderKind/SmtpEncryption(api,rename_all=lowercase)
  impl From<&Model> for View(在 routes)        ←迁→  entity: impl From<&Model> for api::View + From<entity枚举> for api枚举
```

- api-types 枚举用 `#[serde(rename_all = "lowercase")]`,与 entity 的 `string_value`(`smtp`/`starttls`/`tls`/`none`/`sent`/`failed`)逐一对齐 —— wire 字节不变。
- 转换归属 **entity crate**(`From<&entity::Model> for api::*`,api-types 不反向依赖 entity)。
- `routes/mail.rs` 删内联定义,`use swarmhive_api_types as api`;handler 签名 / 路由 / 鉴权 / 行为全不动。
- **Why over CLI 本地 dupe**:项目「DTO ≥2 消费者 → 提 api-types,不在 CLI 重复」是硬约定;CLI + server + 未来 SDK 共享单一源,杜绝漂移。

### D2. schema 稳定性:枚举从 `value_type=String` 变真枚举

现状 DTO 字段用 `#[schema(value_type = String, example="smtp")]` 把枚举在 OpenAPI 里表现为裸 string。提到 api-types 后枚举自带 `ToSchema` →

- **取舍 A(推荐)**:让枚举进 OpenAPI 成字面量联合(`"smtp"` / `"starttls"`…)。schema.gen.ts 对应字段从 `string` **收紧**为联合类型 —— admin 端类型更准(预期 typecheck 仍过,甚至消除若干 `as`)。需 regen schema.gen.ts + 确认 admin 编译。
- **取舍 B**:在 api-types DTO 字段保留 `#[schema(value_type = String)]`,wire 与 openapi **完全不变**(schema.gen.ts 零 diff),代价是 OpenAPI 仍不精确。
- 倾向 A(更准);若 admin 出现非平凡 break 再回落 B。apply 时以 `openapi_surface` + admin typecheck 为准绳。

### D3. 密钥三路输入(storage secret / mail password 共用)

```text
优先级:--secret-stdin(管道)  >  env(SWARMHIVE_STORAGE_SECRET / SWARMHIVE_MAIL_PASSWORD)  >  明文 flag(--access-key-secret / --password)
有 TTY 且都没给 且 create:可交互 prompt(rpassword,沿用 login 范式);非 TTY 不 prompt
update:三路都没给 = 省略该字段 = server 保留已存 secret
```

- CLI 加共享 `resolve_secret(flag, env_key, stdin: bool, interactive_ok: bool) -> Option<String>`。
- docs 红字:`--password <V>` / `--access-key-secret <V>` 会进 shell history / `ps` / CI 日志;**AI / skill 必须走 env 或 `--secret-stdin`**。
- **Why 三路都给**:用户要明文 flag 的顺手(human 快速试);env/stdin 给 AI / CI 安全路径。

### D4. 多行模板走文件

`templates set --id <id> [--subject "…"] [--html-file path] [--text-file path]`;每项省略 = 不改(PUT `UpdateTemplateReq` 字段 Option)。`preview --id --sample-file ctx.json`(minijinja 上下文走文件,返回渲染后的 subject/html/text)。**Why**:HTML/正文多行,塞 flag 不可读;文件输入对 AI / skill 也更稳(skill 生成模板文件再 set)。

### D5. id/name 寻址

storage backend / mail provider 后端按 id(Uuid)寻址。CLI 允许 `--backend <name>` / `--provider <name>` → 先 GET 列表本地解析成 id(便于人 / AI 用可读名)。歧义(重名)→ 报错列出候选。

### D6. 破坏性 `--yes`

`mail providers delete --id --yes`(沿用 management proposal 的 `--yes` 约定)。storage 无 delete(单 active + hot-swap 模型,web 也没有 DELETE),故无破坏性 storage 命令。

## Risks / Trade-offs

- [mail DTO 迁移波及 server+entity+api-types] → wire 不变是硬约束;`openapi_surface` + mail round-trip 单测 + `serde_json` 对 enum 锁死(backend.md 已记 DeriveActiveEnum+serde 分叉坑)。
- [schema.gen.ts 收紧致 admin break] → D2 取舍 A 失败则回落 B(保留 value_type=String),零 admin 改动。
- [密钥明文 flag 误用] → docs 红字 + 推 env/stdin;给 AI 的 token 即便有 storage:manage/mail:manage,secret 也不该进命令串。
- [CLI 边界] → `cargo tree -p swarmhive-cli | grep sea-orm` 必须仍空(mail DTO 在 api-types,CLI 只引 api-types)。

## Migration Plan

mail DTO 迁移是**纯重构**(wire / 行为不变):先在 api-types 建 mail 模块 → entity 加 From → server 改 import 删内联 → regen openapi/schema.gen.ts(确认 diff 仅枚举收紧或为空)。CLI 命令纯增量。回滚 = 还原 import(DTO 留 api-types 无害)。

## Open Questions

- `templates set` 用 PUT(全量 UpdateTemplateReq,字段 Option)还是要求三项齐全?倾向 Option(只改给的项),与 web 模板编辑一致。
- storage `update` 改 endpoint/region 等连接字段后是否自动 `test`?倾向不自动(显式 `storage test`),保持命令单一职责。
