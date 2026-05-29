# 后台与统计

## Admin 目标

SwarmHive Admin 用于替代第三方更新平台控制台，让开发者能直观看到版本、策略、产物、下载量、更新漏斗和存储状态。

后台第一阶段不需要复杂，但必须覆盖发布与排障核心路径。尤其是首次启动后的存储初始化向导，这是单服务器用户能顺利使用 SwarmHive 的关键。

## 页面设计

### Setup Wizard

首次启动时，如果未配置 storage，进入初始化向导。

选项：

- Existing S3-compatible storage。
- Aliyun OSS preset。
- Single-server RustFS。

能力：

- 展示 RustFS 官方 Docker Compose profile 或 CLI 命令。
- 检测 RustFS / S3 endpoint 健康状态。
- 测试 AK/SK。
- 检查或创建 bucket。
- 测试上传和下载。
- 保存 StorageBackend 配置。

### Dashboard

展示：

- 总应用数。
- 当前发布版本数。
- 今日下载量。
- 今日更新检查量。
- 近 7 天下载趋势。
- 更新漏斗概览。
- 下载失败率。
- 存储后端状态。

### Apps

展示应用列表：

- 应用名称。
- slug。
- 支持平台。
- 默认 channel。
- 最新 stable 版本。
- 总下载量。
- 更新检查量。

### Releases

展示某个应用的版本列表：

- 版本号。
- channel。
- 状态。
- 发布时间。
- 更新策略。
- 下载量。
- 更新检查量。
- 产物完整度。

### Artifacts

展示产物：

- 平台。
- target / arch / ABI。
- 文件名。
- 文件大小。
- 存储后端。
- 签名状态。
- 下载地址。

### Policies

配置：

- 可选更新。
- 强制更新。
- 最低可用版本。
- 灰度比例。
- channel 指向。

### Storage

配置：

- S3-compatible endpoint。
- bucket。
- region。
- force path style。
- public base URL。
- signed URL TTL。
- 当前模式：existing S3 / Aliyun OSS / bundled RustFS。
- 连通性测试。
- test upload / test download。

> Provider / 模板 / 日志除 Web Admin 外也可经 CLI 管理(`swarmhive mail providers|templates|logs|status`,见 [12-cli.md](12-cli.md)「配置命令」);SMTP 密码走 `SWARMHIVE_MAIL_PASSWORD` env / `--secret-stdin`,适合 AI / 脚本代为配置。

### Mail Provider

SMTP 配置不写死在配置文件，存在 DB 中由后台编辑（与 Storage 对称）：

- 启用开关：单一 active provider 互斥，`POST /providers/:id/activate` 在 TX 内把其他 row `active=false`，依赖 Postgres READ COMMITTED + 行锁保证并发 activate 串行化。
- SMTP host / port / 用户名 / 密码：密码在客户端以 plaintext 提交、服务端用 `SWARMHIVE_SECRET_KEY`（AES-256-GCM）加密落盘；GET API 仅返回 `password_set: bool`，密文从不出网。
- 加密方式（STARTTLS / TLS / 无）。
- 发件人 From（display name + email）+ Reply-To。
- 连通性测试：`POST /providers/:id/test` 用一份临时 SmtpMailer 给当前登录账号发一封 self-test 邮件，不污染当前 active 槽位。
- Fallback 通道：当任何 provider 未激活、或激活 provider 构建失败（密钥错 / SMTP host 无效），server 回落到 `ConsoleMailer` —— stdout 打印 + 同样写 `mail_log status=Sent provider_id=NULL`，server 不 crash，Admin SPA 顶部 banner 提示"邮件未配置"。
- Hot-swap：`AppState.mailer = Arc<RwLock<MailerHandle>>`，activate / delete handler 调 `refresh_mailer()` 即时切换，无须重启。
- dev 环境（`mail.seed_mailpit_in_dev=true` + provider 表空）启动期自动 seed mailpit provider（host=localhost:1025、无密码），prod 由部署者填自己的 SMTP。

### Mail Templates

邮件模板存 DB，可在线编辑、按 locale 维护多语言：

- 4 event × 2 locale = 8 行默认模板：`user_invite`、`password_reset`、`email_verify`、`security_alert`，每行均 en + zh-CN。复合唯一约束 `(event_name, locale)` 用 sea-orm `#[sea_orm(unique_key)]` 表达。
- 每行字段：subject、html_body、text_body、updated_at。
- 模板用 minijinja 渲染；context 变量按 event 类型有明确 schema（如 password_reset 提供 `{{ user_name }}`、`{{ reset_url }}`、`{{ expires_at }}`）。
- 首次启动 `seed_default_templates(db)` idempotent INSERT-if-not-exists，运维可改、可"恢复默认"（`POST /templates/seed-defaults` 对全部 8 行 UPSERT）。
- TemplateEngine cache：key = `(event, locale, template_id, updated_at)`，admin 编辑保存后 updated_at 推进即立刻失效，下一次发送拉新版本。
- 预览功能：`POST /templates/:id/preview` 接 `{ sample: JSON }` 渲染 subject / html_body / text_body 三段，UI 用 `<iframe sandbox srcDoc>` 隔离展示 HTML；422 错误带 typed `mail-template-invalid` problem，extra 字段 `field=subject|html_body|text_body` 让前端高亮。
- 模板编辑使用 `@monaco-editor/react`（manualChunks `monaco-vendor`）作为 HTML / Text 的代码编辑器。

### Mail Log

每封发送（成功 / 失败 / ConsoleMailer fallback）都写一行 `mail_log`：

- 字段：id / to / template_id (nullable) / provider_id (nullable, ConsoleMailer 时为 NULL) / status (sent|failed) / error / sent_at。
- 仅持久化 metadata，不写 body，供 support / debug 用。
- `GET /api/v1/mail/logs?limit=` 默认 50、上限 500；UI 展开行显示完整 error。后续可按需补 query 参数（时间 / status / 收件人模糊搜目前为客户端过滤）。

### Telemetry

展示更新链路事件：

- 检查更新次数。
- 发现更新次数。
- 下载入口请求次数。
- 下载重定向次数。
- SDK 上报的下载成功 / 失败。
- 新版本启动回传次数。

### Users & Roles

管理（`/users`，需 `user:manage`）：

- **用户列表**：ProTable 列 email / display_name / 角色 Tag / 状态 / 创建时间。状态 Tag：已激活（active）/ 待接受（invited）/ 已禁用（disabled）。数据来自 `GET /api/v1/users`（含每用户 roles）。
- **邀请用户**：抽屉表单 email + 确认 email（双输入防手误）+ 角色下拉（`GET /api/v1/roles`，排除 Owner）+ 可选显示名。提交 `POST /api/v1/users/invite`，被邀人收邮件点链接设密码激活。`email-already-taken` / `cannot-invite-owner` 有专属错误提示。
- **重发邀请**：仅 invited 行可见，Popconfirm → `POST /api/v1/users/invite/{id}/resend`，轮换 token（旧链接立即失效）。
- 分配角色 / app-level role 绑定 / 禁用用户（后续 proposal）。

邮箱验证 banner：未验证用户（`email_verified_at=NULL`）在 AuthLayout 顶部见常驻黄色 banner，可一键重发验证邮件；mailer 处于 console fallback 时 banner 改提示「先配置 SMTP」。账户资料 + 验证状态也在 设置 → 账户（`/settings/account`，人人可见）。详见 [13-rbac.md](13-rbac.md) 邀请 / 密码重置 / 邮箱验证段。

### API Tokens

管理：

- CI/CD Token。
- 只读客户端 key。
- token 权限范围。
- app scope。
- channel scope。
- 过期时间。
- 撤销 token。

## 统计指标

MVP 指标：

- 总下载量。
- 更新检查量。
- 有更新响应量。
- 按应用下载量。
- 按版本下载量。
- 按平台下载量。
- 按天下载趋势。

后续指标：

- 下载失败率。
- 当前活跃版本分布。
- 更新转化率。
- 镜像命中率。
- 地区分布。
- 安装后启动确认率。

## 数据保留

MVP 可直接保存原始事件。

后续可增加：

- 按小时聚合表。
- 按天聚合表。
- 原始事件定期清理。

## 权限策略

MVP 做单组织 + 完整 RBAC，不做真正多租户。

基础角色：

- Owner：管理用户、角色、存储、token 和所有应用。
- Admin：管理应用、版本和策略。
- Release Manager：发布、promote、rollback、yank。
- Developer：上传 draft / beta 产物。
- Viewer：只读查看版本、下载量和埋点。

权限应以 permission 为准，角色只是 permission 集合。

重点权限：

- `storage:manage`。
- `token:manage`。
- `release:publish`。
- `release:promote`。
- `release:rollback`。
- `release:yank`。
- `artifact:upload`。
- `analytics:read`。
- `telemetry:read`。

关键操作需要写入审计日志。

