# add-mail-infrastructure

## Why

docs/08 把 Mail Provider 与 Mail Template 列为 Admin 可管理资源（与 Storage 同形态）。邀请用户、密码重置、邮箱验证、发布通知都需要邮件。无邮件能力会让多人协作流程卡在"Owner 怎么把新成员拉进来"。

## What

### 1. 实体

- `mail_provider`：id、name、kind (`smtp`)、active、host、port、username、`password_encrypted`、`encryption` (`starttls` | `tls` | `none`)、`from_email`、`from_name`、`reply_to`、created_at。
- `mail_template`：id、event_name、locale、subject、html_body、text_body、updated_at。
- `mail_log`：id、to、template_id、provider_id、status、error、sent_at（用于 Admin 排障）。

### 2. Server

- 引入 `lettre`（SMTP transport）+ `minijinja`（runtime template）。
- `swarmhive-server/src/mail/` 模块：
  - `Mailer` trait（`send(MailEnvelope) -> Result<()>`）。
  - `SmtpMailer` 实现，连接 active provider。
  - `ConsoleMailer`（dev fallback：把邮件 dump 到 stdout / mail_log）。
  - `TemplateEngine`：从 DB 加载 template，按 event_name + locale 渲染。
- Server endpoints：
  - `GET /api/v1/mail/providers`、`POST`、`PUT/:id`、`DELETE/:id`、`POST/:id/test`（发一封自检邮件到 Owner）。
  - `GET /api/v1/mail/templates`、`PUT/:id`、`POST/:id/preview`。

### 3. 首批 template seed

- `password_reset`、`user_invite`、`email_verify`、`security_alert`。
- 默认 en + zh-CN。
- 首启自动 seed；存在则跳过；恢复默认通过单独 endpoint。

### 4. 邀请用户流程接入

- `add-auth-and-rbac` 已落地的"Bootstrap Owner"之外，新增 `POST /api/v1/users/invite`（require `user:manage`）：
  - 建 user(status=invited) + 一次性 token → 发 `user_invite` 邮件。
  - 被邀请人点链接 → /accept-invite → 填密码 → 自动登录。

### 5. Dev 环境

- `docker-compose.dev.yml` 加 `mailpit` service（SMTP :1025 + Web UI :8025）。
- `config/dev.toml` 默认 provider 指向 mailpit。

## Acceptance

- 启动空库 → seed 默认 4 个 template。
- Admin 配置一个 SMTP provider → 点 "Send test" → 收到自检邮件。
- 邀请新用户 → 收到邀请邮件 → 点链接接受 → 用户被激活。
- minijinja 语法错误的 template 不会让 server crash，返回 problem+json。
- `mail_log` 能在 Admin 看到失败原因。

## Non-goals

- 不做 SES / SendGrid / Resend / Postmark 等 HTTP API provider（只 SMTP；trait 留扩展点）。
- 不做退订 link / unsubscribe center。
- 不做营销邮件 / 群发。
- 不做 i18n 自动切换（按用户 locale 字段选 template，无 fallback chain，找不到就走 en）。

## Depends on

- `add-auth-and-rbac`

## Maps to docs

- [docs/08-admin-and-analytics.md](../../../docs/08-admin-and-analytics.md) Mail Provider / Mail Templates。
- [docs/13-rbac.md](../../../docs/13-rbac.md) "邀请用户" 隐式提及。
