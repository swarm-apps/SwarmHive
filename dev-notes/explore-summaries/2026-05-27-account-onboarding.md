# Explore Summary — Account Onboarding（2026-05-27）

> 这是一份 explore 模式产出的临时决策档，给接下来 5 个 proposal 的 `/opsx:propose` 引用。归档前不要 commit 到 main 的"正经"文档树（dev-notes/knowledge/）—— 决策落到具体 proposal 后，本文件可删。

## 背景

用户希望像 Coolify 一样"第一个注册者成 root"，并补齐 login UI + 邮箱注册 + OAuth。当前已有：

- `/api/v1/setup` 用 setup_token + stdout 模式 bootstrap Owner（已 archived `add-auth-and-rbac`）
- argon2id 密码 + tower-governor 请求级限流（已就绪）
- `add-mail-infrastructure` / `add-oauth-github` proposal 已写但 0/0 tasks
- admin SPA `/login` 是占位 Card

## 关键决策

### Bootstrap 模式

- **Coolify 式**：删 setup_token；user 表空时 admin SPA 跳 `/setup` 裸表单（email/password）；首人完成即 Owner；user 表非空后 `/setup` 永久 410 Gone
- **可选锁定**：ENV `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` 设置后，`/setup` 表单 email 字段固化为该值（防陌生人抢；docker run 时 `-e` 即拿到中等保护）
- **OAuth 不在 bootstrap**：oauth_provider 配置走 admin 后台（路 2），首个 Owner 必须 email/password；之后 Owner 登录配 OAuth provider，后续才有 GitHub 入口

### 安全 baseline

| 项 | 决策 |
|---|---|
| 密码哈希 | argon2id OWASP 2024 params（**已就绪**） |
| 请求级限流 | tower-governor 已 attach `/auth+/setup`（**已就绪**） |
| 账号级软锁 | **新增** `user_login_attempts` 表（user_id, failed_count, last_failed_at, locked_until）；5 次失败 → 30 min 锁；锁定期间 `/login` 返 410 + "请稍后或重置密码" |
| 密码强度 | **新增** Zod (前端) + garde (后端) 双校验：≥12 字符 / ≥3 类（大写 / 小写 / 数字 / 特殊符）/ 弱口令字典；NIST 800-63B 推荐参数 |
| `/login` 错误信息 | 始终返 "invalid credentials"（防 timing + 防 email 枚举），**已就绪** |
| CAPTCHA | **不做**（需外部服务，违 self-host 主旨） |
| HIBP 已泄露密码 | **不做**（轻微外部依赖，留 NTH） |

### 注册路径矩阵

|  | bootstrap | 自助 | 邀请 | OAuth |
|---|---|---|---|---|
| email/password | ✓ | policy 开 | ✓ | — |
| GitHub OAuth | ✗ | policy 开 | — | ✓ |
| admin 手动创建 | — | — | ✓ | ✓ |

### registration_policy（singleton 表）

```rust
struct RegistrationPolicy {
    id: i32,                              // always 1
    allow_self_register_email: bool,      // 默认 false
    allow_self_register_oauth: bool,      // 默认 false
    require_email_verify: bool,           // 默认 true
    self_register_default_role: String,   // 'viewer' (default)
    self_register_require_approval: bool, // 默认 true → verify 后 status='pending_approval'
    allowed_email_domains: Vec<String>,   // 空 = 无白名单
    updated_at: DateTimeUtc,
    updated_by: Uuid,
}
```

`self_register_default_role` 和 `self_register_require_approval` 正交：role 决定"批准后什么权限"，approval 决定"要不要先批"。

### OAuth × email 冲突

GitHub email 已被 password user 占用 → **409 + 文案 "请先用 [email] 密码登录后到 Profile 点 'Link GitHub'"**。沿用 `add-oauth-github` 现有设计，零额外字段。

### Provider 配置形态

- **DB schema 独立**：`mail_provider` 表 vs `oauth_provider` 表，各自字段、各自 `/test` 行为、各自演化
- **admin SPA 统一菜单**：Settings > Mail / Authentication / Storage / Telemetry

### user 表 schema 变更（分散到 ① / ④ / ⑤ 落地）

```diff
  user {
    id, org_id, email, display_name, avatar_url,
-   status: 'active' | 'disabled' | 'invited',
+   status: 'active' | 'disabled' | 'invited' | 'pending_verify' | 'pending_approval',
+   email_verified: bool,
+   email_verified_at: Option<DateTimeUtc>,
  }

+ user_login_attempts { user_id PK, failed_count, last_failed_at, locked_until }
+ registration_policy { id (singleton) ... 见上 }
+ oauth_provider { id, kind, name, client_id, client_secret_enc, ... }
```

## 推进路线（5 proposal × 4 phase）

```
Phase 1（独立可并行）
├─ ① add-login-and-owner-bootstrap-ui
│   What:
│   - 改 /api/v1/setup：删 setup_token、加 ENV SWARMHIVE_BOOTSTRAP_OWNER_EMAIL、bootstrap window 完成后 410
│   - admin SPA /login 真实表单（取代占位）
│   - admin SPA /setup 引导页（user 表空 → 跳 /setup，否则跳 /login）
│   - router beforeLoad 检测 bootstrap 状态分流
│   - 加 user_login_attempts 表 + 5/30min 软锁（baseline）
│   - Zod 密码强度（仅 /login 不强校验已存，/setup 必须强）
│   Depends on: 已 archived 全部
│
└─ ② add-mail-infrastructure（✅ 已 apply，待归档；规模 67 tasks）
    Done:
    - server: Mailer trait + SmtpMailer (lettre) + ConsoleMailer fallback + hot-swap MailerSlot
    - 4 event × 2 locale 默认模板（minijinja runtime）+ Admin "恢复默认" + iframe sandbox preview
    - crypto::SecretKey（AES-256-GCM；SWARMHIVE_SECRET_KEY env 或 config/local.toml）→ provider 密码加密落盘
    - /api/v1/mail/{providers,templates,logs,status} 12 endpoints + mail:manage permission
    - admin SPA /settings/mail (providers/templates/logs) + __root.tsx fallback banner
    - dev mailpit auto-seed（cfg.mail.seed_mailpit_in_dev + 表空）
    Deferred to follow-up（17 [~]）:
    - Vitest mail page + Playwright e2e（admin app 还没有第一个组件级 vitest 套）
    - DTO 外提到 swarmhive-api-types（等第二消费者，CLI mail 子命令出现再做）
    - mail_provider partial unique（sea-orm 2 rc.38 schema-sync bug；用 TX + READ COMMITTED 替代）
    Depends on: 已 archived 全部（独立于 ①）

Phase 2（依赖 ①）
└─ ③ add-oauth-github-and-provider-config（重命名 add-oauth-github + 扩展）
    What:
    - 沿用现有 oauth-github proposal：oauth2 crate + GitHubProvider + start/callback endpoint + 409 邮箱冲突
    - 新增：oauth_provider 实体（kind/name/client_id/client_secret_enc/scopes/userinfo_url/email_field）
    - 新增：admin SPA Settings > Authentication 页（CRUD provider + /test 校验 client_id/secret）
    - 改：/login 按 DB provider 列表条件渲染按钮
    Depends on: ①（共享 /login 表单容器）

Phase 3（依赖 ① + ②）
└─ ④ add-invite-and-password-reset
    What:
    - server: /api/v1/users/invite + /api/v1/users/accept-invite + /api/v1/auth/forgot-password + /api/v1/auth/reset-password
    - admin SPA: Users 页邀请按钮 + /accept-invite + /forgot-password + /reset-password
    - 新增 user.status='pending_verify'（被邀人在 verify 前的状态）
    - 邀请 token / reset token 用一次性 + 24h 过期 + DB 存 hash
    Depends on: ① + ②（mail 必须先就绪）

Phase 4（依赖 ① + ② + ③）
└─ ⑤ add-registration-policy-and-self-register
    What:
    - registration_policy 实体（singleton）
    - admin SPA Settings > Authentication 加自助注册开关 + 默认角色 + 是否需审批
    - /register UI（仅 allow_self_register_email=true 时可访问）
    - server: 自助注册 endpoint + email verify endpoint
    - OAuth callback 路径增加"new GitHub user"分支（按 allow_self_register_oauth 决定）
    - 加 user.status='pending_approval'（require_approval=true 时使用）
    Depends on: ① + ② + ③
```

## 显式排除的 Non-goals（避免下次 explore 重复）

- **不做**：CAPTCHA、HIBP、MFA / 2FA、SCIM 用户同步、Google / GitLab / OIDC（除 trait 留扩展）
- **不做**：多 org / 多租户（项目锚定单 org + RBAC）
- **不做**：营销邮件、退订中心
- **不做**：bootstrap 阶段允许 OAuth（强约束 Owner 必走 email/password 创建）

## propose 启动顺序

按依赖建议顺序：

```bash
/opsx:propose add-login-and-owner-bootstrap-ui
# 或先 update 已有的：
/opsx:propose add-mail-infrastructure   # 现有 proposal 已写，需扩展
/opsx:propose add-oauth-github          # 重命名为 add-oauth-github-and-provider-config

# 后两个全新：
/opsx:propose add-invite-and-password-reset
/opsx:propose add-registration-policy-and-self-register
```

每次 propose 时直接引用本文件做 What/Acceptance 草稿。
