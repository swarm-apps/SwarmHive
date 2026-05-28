# design

## Context

无邮件能力是单组织协作流程的硬性短板：邀请新成员、密码重置、邮箱验证、未来的发布通知 / 安全告警都没法投递。同时 docs/08 已经把 Mail Provider 与 Mail Template 列为 Admin 可管理资源（与 Storage 同形态），决定了"运行时配 SMTP + 模板可编辑 + 自检 + 日志"是 MVP 必须的能力面。

约束：

- **self-host**：不接外部 HTTP API provider（SES/SendGrid 等），只 SMTP
- **单组织**：一个 active provider 就够，不做多 org 子域
- **加密 at-rest**：SMTP password 不能明文存 DB，必须加密
- **不让 template 错误炸 server**：minijinja 错误必须可恢复
- **admin SPA 是配置入口**：Settings > Mail 菜单层级，跟 Storage / Telemetry 同级
- **依赖关系下游有 ④⑤**：要明确"业务流接入"不在本 proposal 范围

## Goals / Non-Goals

**Goals:**

- Owner / admin 能在 web 后台配 SMTP provider + 编辑模板 + 发自检 + 查日志
- 邮件投递走 trait 化的 Mailer，dev 环境无 SMTP 时 fallback 到 ConsoleMailer（不阻塞开发）
- Template 编辑 / 渲染失败可观测且可恢复（不让 server crash）
- 为 ④⑤ 留好"调用 Mailer + 用 template event_name" 的 API 接口

**Non-Goals:**

- 不实现邀请 / 重置 / 验证业务 endpoint（留 ④⑤）
- 不支持 HTTP API provider（SES / SendGrid / Resend / Postmark 等）
- 不做退订 / 群发 / 营销
- 不做模板版本管理
- 不做 i18n fallback chain（找不到 locale 走 en）

## Decisions

### 1. Mailer trait + SmtpMailer + ConsoleMailer

```rust
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, envelope: MailEnvelope) -> Result<MailLogEntry, MailError>;
}

pub struct MailEnvelope {
    pub to: String,
    pub event_name: String,        // 用于查 template
    pub locale: Locale,            // user.locale，没有则 'en'
    pub context: serde_json::Value, // 模板渲染上下文
}

pub struct SmtpMailer {
    transport: lettre::AsyncSmtpTransport<...>,
    template_engine: Arc<TemplateEngine>,
    db: DatabaseConnection,
}

pub struct ConsoleMailer {
    template_engine: Arc<TemplateEngine>,
    db: DatabaseConnection,
}
```

```text
启动期：
   读 mail_provider WHERE active=true
   ├─ 找到 → 解密 password (AES-GCM with SWARMHIVE_MAIL_PASSWORD_KEY) → 构造 SmtpMailer
   └─ 没有 → 构造 ConsoleMailer
   把 Arc<dyn Mailer> 装进 AppState
```

**为什么 trait 化**：方便测试（mock Mailer）+ dev fallback（ConsoleMailer）+ 后续接 SES 等 HTTP provider 不动 caller 代码。

**为什么 active provider 仅一个**：单组织语义；多 active 反而引入"按 event 路由"复杂度（业务 NTH）。DB partial unique index（`UNIQUE WHERE active=true`）强制。

**为什么 ConsoleMailer 不仅打 stdout**：同时落 `mail_log`，让 admin SPA 的 logs 页在 dev 也能看到内容，调试模板更顺。

### 2. 密码加密：AES-256-GCM + ENV key

```rust
SWARMHIVE_MAIL_PASSWORD_KEY = base64(随机 32 字节)
```

- 启动期读 ENV，缺失 → server 启动失败 fast-fail（不能在生产部署时忘配）
- dev 环境 `config/dev.toml` 注释提示用 `openssl rand -base64 32` 生成
- AES-256-GCM (nonce 12B + ciphertext + tag 16B) 用 `aes-gcm` crate
- `mail_provider.password_encrypted` 存 `base64(nonce || ciphertext || tag)`
- API 永不返明文：`POST/PUT` 接受明文 password 字段；`GET` 返回 `password_set: bool`（永不返加密 blob）

**为什么不用对称 key 派生 from session secret**：session secret 是 rotation 候选；mail provider 加密 key 一旦轮换全部 password 失效。隔离更安全。

**为什么不引 vault / KMS**：self-host 主旨违背 + 部署复杂度爆炸。后续 NTH。

### 3. TemplateEngine + minijinja

```rust
pub struct TemplateEngine { /* 内部 LRU cache: (event_name, locale, updated_at) → CompiledTemplate */ }

impl TemplateEngine {
    pub async fn render(&self, event: &str, locale: &Locale, ctx: &Value)
        -> Result<RenderedMail, TemplateError>;
}

pub struct RenderedMail { pub subject: String, pub html_body: String, pub text_body: String }
```

```text
admin 改模板 → PUT /api/v1/mail/templates/:id → updated_at 更新
TemplateEngine 下次 render 检查 cache key 含 updated_at → miss → DB load + 编译 → cache
模板语法错误 → 编译失败 → TemplateError → handler 返 422 problem+json type=template_invalid
```

**为什么 minijinja 不用 tera / askama**：minijinja runtime 模板，admin 可编辑；tera 也行但 minijinja 体积更小且语法 Jinja2 兼容（设计师可用）。askama 是编译期模板，不能 admin 改。

**为什么 cache 加 updated_at 维度**：避免每次 render 查 DB；同时改模板立即生效。

### 4. Admin SPA Settings > Mail 三页

```
routes/_auth.settings.mail.tsx               provider 列表 + ProDrawerForm 编辑
routes/_auth.settings.mail.templates.tsx     template 列表 + Monaco 编辑 + preview
routes/_auth.settings.mail.logs.tsx          mail_log 分页 + 错误展开
```

```text
Settings 菜单（admin SPA __root.tsx 注入）:
├─ Mail
│   ├─ Providers
│   ├─ Templates
│   └─ Logs
├─ Authentication （留给 ③）
├─ Storage （留给后续 storage proposal）
└─ Telemetry （留给后续 telemetry proposal）
```

`routes/_auth.settings.tsx` 是 layout route，渲染左侧二级菜单 + 右侧 `<Outlet />`。本 proposal 落 layout + mail/* 三页；其他菜单条目 disabled 直到对应 proposal 落地。

### 5. Permission gate

新增 permission `mail:manage`，默认绑定 `owner` + `admin` role。`add-auth-and-rbac` 的 permission seed 在启动期补这一个；user_role 表无 schema 变化。

mail 相关 endpoint 用 `RequirePermission<MailManage>` extractor（沿用 archived auth proposal 的 permission gate 模式）。

### 6. Mailpit dev 集成

```yaml
# docker-compose.dev.yml 新增 service
mailpit:
  image: axllent/mailpit:latest
  ports: ['1025:1025', '8025:8025']
```

```toml
# config/dev.toml 新增
[mail]
# dev fallback：seed 一个 provider 指向 mailpit；prod 留空，admin 自己配
seed_mailpit_in_dev = true
```

启动期若 `seed_mailpit_in_dev=true` 且 mail_provider 表空 → INSERT 一个 active provider `{ host: 'localhost', port: 1025, encryption: 'none', from_email: 'noreply@swarmhive.local' }`。

## Risks / Trade-offs

- **[SWARMHIVE_MAIL_PASSWORD_KEY 丢失 → 所有 provider password 失效]** → 文档强调"备份 + 不要轮换"；admin SPA 在 provider 列表显示 "password_set: false" 让 owner 重新填。
- **[minijinja 编译错误 cache 污染]** → cache 只存 `Ok(CompiledTemplate)`，编译失败不入 cache，下次 render 重新尝试（不堵塞修复）。
- **[ConsoleMailer fallback 让 prod 部署误以为邮件已发]** → admin SPA Provider 列表显式标 "Console (dev only)"；prod 启动若检测到 ConsoleMailer + non-dev profile → warn 日志 + admin SPA 顶部 banner "尚未配置邮件，部分功能不可用"。
- **[lettre 升级风险]** → lettre 0.11 稳定；pin 在 workspace `[workspace.dependencies]`，升级走 Renovate。
- **[mail_log 表无限增长]** → 不加 retention（本 proposal NTH）；admin SPA logs 页加分页 + 时间过滤；后续可加 cron 定期清理（独立 proposal）。
- **[Monaco editor bundle 大]** → ~1MB gzip，独立 chunk + 仅 templates 页 lazy load；admin-frontend-foundation 已有 manualChunks 拆 vendor，可加 monaco-vendor。
- **[模板预览 XSS 风险]** → preview 接口返回纯字符串，admin SPA 用 `<iframe srcDoc>` 隔离渲染 HTML body（不直接 inject 当前文档 DOM）。
- **[partial unique index 的 sea-orm 表达]** → sea-orm 0.13+ 用 `Index::create().table(...).col(Column::Active).cond_where(...)` 表达 partial；schema-sync 模式下需要 raw SQL fallback，本 proposal apply 时验证。

## Migration Plan

无破坏性 DB 变更（全新表）。部署路径：

1. PR 落 main → CI 全绿
2. 部署 → schema sync 自动创建 3 表 + seed 默认 8 行 template
3. dev 环境 mailpit auto-seed active provider；prod 环境留 console fallback + banner 提示
4. Owner 登录 → Settings > Mail → 配 SMTP → 激活 → 发自检

回滚：revert + 重启；mail_provider 表残留无害。

## Open Questions

- **mail_log 是否记录 body** → 本 proposal 不记录（隐私 + 体积）；只记 to / template / status / error。若调试需要，让 admin 用 `/preview` 看渲染结果。
- **是否支持 SMTP retry policy** → 不支持（NTH）；失败即标 `failed` + 错误信息进 log，由调用方决定是否重试。
- **是否支持 from_email 校验 SPF/DKIM** → 不校验（admin 自行配 DNS 是基础假设）。
- **是否允许 admin 手动重发 failed mail** → 不允许（NTH）；admin 看 log 后可在原业务流重新触发。
- **Monaco 替换为 CodeMirror 6 时机** → 本 proposal 用 Monaco（生态成熟）；若后续 bundle 优化需要可换 CodeMirror（API 兼容性差异需评估）。
