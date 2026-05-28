# add-mail-infrastructure

## Why

docs/08 把 Mail Provider 与 Mail Template 列为 Admin 可管理资源（与 Storage 同形态）。后续所有"把人拉进来 / 把账号找回来"的能力 —— 邀请用户（④）、密码重置（④）、邮箱验证（⑤）、未来的发布通知 / 安全告警 —— 都需要邮件作为底层投递通道。无邮件能力会让 ④⑤ 全部卡住。

本 proposal 只交付"邮件基础设施 + admin 后台可配 + 自检"，**不接入**任何业务流（邀请 / 重置 / 验证由 ④⑤ 各自接入）。

## What Changes

### 1. 实体

- `mail_provider`：id、name、kind (`smtp`)、active、host、port、username、`password_encrypted`、`encryption` (`starttls` | `tls` | `none`)、`from_email`、`from_name`、`reply_to`、created_at、updated_at
  - 任一时刻仅一个 `active=true`（DB partial unique index 强制）
- `mail_template`：id、event_name、locale、subject、html_body、text_body、updated_at
  - 唯一约束 `(event_name, locale)`
- `mail_log`：id、to、template_id、provider_id、status (`sent` | `failed`)、error、sent_at（用于 Admin 排障）

### 2. Server

- 引入 `lettre`（SMTP transport）+ `minijinja`（runtime template）
- `swarmhive-server/src/mail/` 模块：
  - `Mailer` trait（`send(MailEnvelope) -> Result<()>`）
  - `SmtpMailer` 实现，连接 active provider
  - `ConsoleMailer`（dev fallback：把邮件 dump 到 stdout + mail_log，无 active provider 时使用）
  - `TemplateEngine`：从 DB 加载 template，按 event_name + locale 渲染；语法错误转 problem+json 不让 server crash
- Server endpoints（全部 require `mail:manage` permission）：
  - `GET /api/v1/mail/providers`、`POST`、`PUT /:id`、`DELETE /:id`
  - `POST /api/v1/mail/providers/:id/test`：发自检邮件到当前 Owner / Principal 邮箱
  - `POST /api/v1/mail/providers/:id/activate`：把某 provider 设为 active（其它自动 deactivate）
  - `GET /api/v1/mail/templates`、`PUT /:id`、`POST /:id/preview`：preview 用 sample data 渲染，不发送
  - `GET /api/v1/mail/logs`：分页查询历史
- 密码加密：复用 `add-auth-and-rbac` 的 argon2 不合适（argon2 不可逆）；用 server master key（启动期 `SWARMHIVE_MAIL_PASSWORD_KEY` env 派生）+ AES-256-GCM 对 `password_encrypted` 加解密；解密发生在 `SmtpMailer::new()` 处

### 3. 首批 template seed

- `password_reset`、`user_invite`、`email_verify`、`security_alert`
- 默认 en + zh-CN
- 首启自动 seed；已存在则跳过；恢复默认通过 `POST /api/v1/mail/templates/seed-defaults`（require `mail:manage`）

### 4. Admin SPA: Settings > Mail 页

- 路由 `routes/_auth.settings.mail.tsx`（受 `_auth` guard + permission gate `mail:manage`）
- ProTable 列表 mail_provider（含 active 状态高亮 + 启用切换按钮）
- ProDrawerForm 新建 / 编辑 provider（password 字段写入时加密，读取时永不返明文，UI 显示 "已设置" / "未设置"）
- ModalForm "发送自检邮件" → POST `/test` → notification 显示成功 / 失败（含 provider 返回的 SMTP error）
- 模板管理子页 `routes/_auth.settings.mail.templates.tsx`：左侧列表 event_name + locale，右侧 Monaco editor（subject + html_body + text_body）+ preview tab（POST `/preview` 渲染）
- mail_log 查看子页 `routes/_auth.settings.mail.logs.tsx`：ProTable 时间 / 收件人 / 模板 / 状态 / 错误
- 全文案 `<Trans>` 包裹

### 5. Dev 环境

- `docker-compose.dev.yml` 加 `mailpit` service（SMTP :1025 + Web UI :8025）
- `config/dev.toml` 默认 provider 指向 mailpit；首启自动 seed 一个 active provider 指向 mailpit
- prod 部署：admin 在 Settings > Mail 配 SMTP，激活；否则邮件落 mail_log 不发

### 6. 权限补充

- `add-auth-and-rbac` 的 permission 集补 `mail:manage`，默认 `owner` + `admin` role 持有；`publisher` / `viewer` 不持有

## Capabilities

### New Capabilities

- `mail-infrastructure`：mail provider CRUD + 模板系统 + 自检 + 日志 + admin SPA Settings 页的可观测行为契约

### Modified Capabilities

- 隐式扩展 `add-auth-and-rbac` permission 集（加 `mail:manage`），但不修改 archived spec

## Impact

- **Code**：server `src/mail/` 新模块（5-6 文件）+ 6 个新 entity / handler；admin SPA 新增 3 个 page（providers / templates / logs）+ Settings 菜单注入
- **DB**：新增 `mail_provider` / `mail_template` / `mail_log` 表；user_role 表无变化，permission 集逻辑层加 `mail:manage`
- **API**：新增 `/api/v1/mail/*` 全套
- **OpenAPI**：drift gate 触发，admin regen `schema.gen.ts`
- **Deps**：server +`lettre` +`minijinja` +`aes-gcm` +`base64`；admin +Monaco editor（或 CodeMirror，本 proposal 选 Monaco；可后续替换）
- **Dev infra**：`docker-compose.dev.yml` 加 mailpit
- **不影响**：CLI / PAT / Storage / RBAC entity 结构 / RBAC role 表 schema

## Non-goals

- 不做 SES / SendGrid / Resend / Postmark 等 HTTP API provider（仅 SMTP；trait 留扩展点，新 kind 后续 proposal）
- 不做退订 link / unsubscribe center
- 不做营销邮件 / 群发
- 不做 i18n 自动切换（按用户 locale 字段选 template，无 fallback chain，找不到就走 en）
- **不接入业务流** —— 邀请用户、密码重置、邮箱验证 endpoint 与 UI 留给 ④⑤ proposal
- 不做模板版本管理（每个 template 直接覆盖；历史在 git）

## Depends on

- `add-auth-and-rbac`（archived）—— provide `mail:manage` permission slot + audit log
- `add-admin-frontend-foundation`（archived）—— provide ProTable / ProForm / Settings 菜单注入位 / $api client / 错误链

不依赖 ①（login UI）：本 proposal 独立可推。

## Maps to docs

- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) Mail Provider / Mail Templates
- [docs/13-rbac.md](../../../docs/13-rbac.md) `mail:manage` permission
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) 补 mailer trait + 加密策略
- [dev-notes/knowledge/admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) 补 Settings 菜单约定
- [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md) ② 段

## Acceptance

- 启动空库 → seed 默认 4 个 template × 2 locale = 8 行
- Admin 配置一个 SMTP provider → 点 "Send test" → 收到自检邮件（dev 用 mailpit 查 :8025）
- Provider 密码字段以加密形式存 DB，列表 / 详情 API 永不返明文
- minijinja 语法错误的 template `/preview` 返 422 problem+json，不让 server crash
- `mail_log` 在 Admin 能看到失败原因（点行展开 stack trace）
- `pnpm lint` / `cargo clippy` / `cargo test --workspace` / `pnpm --filter @swarmhive/admin test` 全绿
- OpenAPI drift gate 通过
