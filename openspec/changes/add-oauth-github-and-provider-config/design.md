# design

## Context

`add-auth-and-rbac` 把 `identity_link` 实体（user_id + provider + subject + metadata）落齐了，但 OAuth flow 本身一直没实现。同时探索（2026-05-27）拍板：OAuth provider 必须支持运行时配置（不是 config.toml 重启）+ admin Settings 菜单统一 UX + bootstrap 阶段禁用 OAuth（首个 Owner 必走 email/password）。

约束：

- **单 org**：一种 OAuth kind 在 MVP 只配一个 provider（GitHub 一个就够）
- **secret 加密 at-rest**：跟 mail provider 同形态，复用 `SecretKey` 模块
- **bootstrap 排除**：首人不能 GitHub，防止被陌生 GitHub 用户抢成 Owner
- **依赖 ①**：/login UI 容器 + bootstrap window 模型
- **可选依赖 ②**：SecretKey 复用；二选一谁先落地

## Goals / Non-Goals

**Goals:**

- Owner / admin 能在 web Settings 配 GitHub OAuth provider（client_id / secret / scopes）
- 已配 + enabled 的 provider 自动出现在 /login 按钮区
- 已登录 user 能从 Profile 绑定 / 解绑 GitHub
- GitHub email 跟现存 password user 冲突 → 安全文案引导（不自动合并）
- bootstrap 期间所有 OAuth start endpoint 410（不能成 Owner）

**Non-Goals:**

- 不实现 OAuth 自助注册（callback "无现存 user 无冲突" 分支留 ⑤）
- 不实现 Google / GitLab / 内部 OIDC（trait 留扩展）
- 不实现 GitHub Enterprise 矩阵测试
- 不实现 SCIM
- 不重写 archived auth proposal 的 spec

## Decisions

### 1. IdentityProvider trait + GithubProvider

```rust
#[async_trait]
pub trait IdentityProvider: Send + Sync {
    fn name(&self) -> &str;                       // "github"
    fn authorize_url(&self, state: &str, pkce: &str, redirect_uri: &str) -> Url;
    async fn exchange(&self, code: &str, pkce: &str) -> Result<ExternalIdentity, OAuthError>;
}

pub struct ExternalIdentity {
    pub subject: String,           // GitHub user id (stable)
    pub email: Option<String>,     // verified primary email
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub raw: serde_json::Value,    // 原始 user payload，存 identity_link.metadata
}

pub struct GithubProvider {
    client_id: String,
    client_secret: String,         // 已 decrypted
    scopes: Vec<String>,
    authorize_url: Url,
    token_url: Url,
    userinfo_url: Url,
}
```

**为什么 trait 化**：未来加 Google / GitLab / OIDC 只挂新 impl，不动 callback handler。

**为什么 ExternalIdentity 不直接是 User**：解耦"外部身份" 和 "内部账号"。一个内部 user 可能有多个 identity_link（GitHub + Google）；ExternalIdentity 只描述"刚从 provider 拿到什么"。

### 2. /start 与 /callback 数据流

```text
GET /api/v1/auth/oauth/github/start?next=/apps
   │
   ▼
gen state = random(32B), pkce = random(64B)
session 存 { state, pkce, next, kind: 'github' }
   │
   ▼
查 oauth_provider WHERE kind=github AND enabled=true
   ├─ 找到 + bootstrap_state.needs_bootstrap=false → redirect provider.authorize_url(state, pkce, callback_url)
   └─ 找不到 OR needs_bootstrap → 410 oauth_not_available_during_bootstrap / 404 provider_not_configured

GET /api/v1/auth/oauth/github/callback?code=...&state=...
   │
   ▼
从 session 取 { stored_state, stored_pkce, next }；不匹配 → 400 oauth_state_mismatch
   │
   ▼
provider.exchange(code, pkce) → ExternalIdentity { subject, email, ... }
   │
   ▼
查 identity_link WHERE kind=github AND subject=<id>
   │
   ├─ 已存在 → 找 user → 写 session → 302 redirect next
   │
   └─ 不存在 → 查 user WHERE email = ext.email
        │
        ├─ 找到 password user → 不自动合并 → 302 redirect /login?oauth_conflict=github,<email>
        │     /login UI 读 search param 展示 Alert "GitHub 邮箱 X 已注册，请先用密码登录后到 Profile 绑定"
        │
        └─ 找不到 → 本 proposal 默认 401 oauth_registration_disabled
              （⑤ 落地后改为读 registration_policy.allow_self_register_oauth 决定）
```

**为什么用 session 存 state/pkce**：tower-sessions 已就绪；比 signed cookie / Redis 简单；nonce 单次性自然由 session 生命周期保证。

**为什么 conflict 走 redirect 不直接 405 problem+json**：用户在 GitHub 回跳是浏览器导航，不是 fetch；直接 problem+json 用户看不到友好提示。redirect 让 /login 把冲突信息渲染成 Alert。

### 3. Bind/Unlink 流程（已登录 user 操作）

```text
POST /api/v1/auth/oauth/providers/link/github/start (require auth)
   │
   ▼
session 存 { state, pkce, mode: 'link', user_id: current_user.id, kind: 'github' }
   │
   ▼
redirect provider.authorize_url(...)
   │
   ▼
回到 same callback handler
   │
   ▼
检测 session.mode == 'link':
   ├─ subject 已被其他 user link → 409 identity_already_linked
   ├─ subject 已被当前 user link → 200 no-op（idempotent）
   └─ 否则 → INSERT identity_link (current_user.id, github, subject, metadata) → 302 /profile

DELETE /api/v1/auth/oauth/links/github (require auth)
   │
   ▼
查 user.user_credentials WHERE user_id = current_user.id
   ├─ 存在 password → DELETE identity_link → 200
   └─ 无 password → 409 cannot_unlink_only_auth_method（OAuth-only user 解绑会锁死）
```

**为什么 link 流程复用 callback handler**：避免两套 state 校验代码；mode 字段做分支。

**为什么不允许 OAuth-only user 解绑**：解了登录都进不来；让 user 先 set-password（NTH，留 profile-ui proposal），再 unlink。

### 4. oauth_provider 实体 + GitHub 默认 URL

```rust
pub enum ProviderKind {
    Github,
    // future: Google, GitLab, OidcGeneric, ...
}

pub struct Model {
    id: Uuid,
    kind: ProviderKind,
    name: String,                 // 显示 "GitHub" / "Internal SSO"
    enabled: bool,
    client_id: String,
    client_secret_encrypted: String,
    scopes: Vec<String>,          // ["read:user", "user:email"] for GitHub
    authorize_url: String,
    token_url: String,
    userinfo_url: String,
    email_field: String,          // default "email"，OIDC 自定义用
    created_at: DateTimeUtc,
    updated_at: DateTimeUtc,
}
```

`POST /api/v1/auth/providers` 若 kind=Github 且 URL 字段为空 → 自动填：
- authorize_url = `https://github.com/login/oauth/authorize`
- token_url = `https://github.com/login/oauth/access_token`
- userinfo_url = `https://api.github.com/user`
- scopes 默认 `["read:user", "user:email"]`

**为什么字段存 URL 而非硬编码 GitHub URL**：留 GitHub Enterprise / fork 自托管的扩展空间；admin 改也是合法。

**为什么 kind 是 enum 而非 free text**：MVP 限制可信集；future 加新 kind 走代码变更而非 admin 配置（factory 函数需要新 impl）。

### 5. SecretKey 模块共享

如果 ② mail-infrastructure 先落地：
- 共享 `crates/swarmhive-server/src/crypto.rs`（重命名自 `mail/crypto.rs`）+ `SecretKey` 类型 + ENV `SWARMHIVE_SECRET_KEY`
- 本 proposal apply 时把 mail 模块的 import 改成 `crate::crypto::SecretKey`

如果 ③ 先落地：
- 本 proposal 落 `crates/swarmhive-server/src/crypto.rs` + `SecretKey`
- ② apply 时复用

apply 时序 + 是否需要 alias key（兼容 `SWARMHIVE_MAIL_PASSWORD_KEY`）由 tasks 中明确决定。

### 6. Admin SPA 三块 UI

```
/login                        ① 落地，本 proposal 注入 OAuth 按钮区
/settings/authentication      本 proposal 新建（layout 由 ② 已落）
/profile                      本 proposal 新建（最小版，仅 Linked accounts）
```

**为什么 Profile 也在本 proposal**：linked accounts 是 OAuth flow 的另一端（用户视角的 "我已绑哪些"），跟 Settings>Authentication（admin 视角的 "可绑哪些 provider"）天然成对。

## Risks / Trade-offs

- **[GitHub email 未 verified]** → `https://api.github.com/user/emails` 返回所有 email + verified 标记；本 proposal 仅信任 `verified=true` 的 email；找不到 verified email → 422 oauth_no_verified_email。
- **[oauth_email_conflict 信息泄露]** → 攻击者通过 /login?oauth_conflict=...&email=... 可以确认某 email 是否在系统中。Mitigation：admin SPA 仅显示 "您的 GitHub 邮箱已在系统中注册，请用该邮箱登录"，不暴露具体 email（让用户从 GitHub 那边自己知道），URL 参数仅 `oauth_conflict=github`（不带 email）。
- **[bootstrap 期间 GitHub 探针]** → 配过 oauth_provider 后清空 user 表的边角案例。Mitigation：bootstrap window 检查只查 user 表 count；oauth_provider 存在不影响判断；start endpoint 410 即可。
- **[/test 实现简单可能漏掉 typo]** → 仅校验 client_id 非空 + URL 可达；client_secret 错误要 user 真实点 GitHub 登录才发现。Mitigation：本 proposal NTH；后续可加"silent token exchange smoke test"。
- **[oauth2 crate 升级 API 不稳]** → pin workspace 版本；oauth2 5.x 已稳定，升级走 Renovate。
- **[PKCE 在 confidential client 不必要]** → GitHub OAuth 支持 confidential client 不强制 PKCE，但加 PKCE 没坏处；保留以备未来 SPA-only flow。
- **[Linked accounts 数据上限]** → 一个 user 一个 identity_link per kind（DB 唯一索引 `(user_id, kind)`）；不支持同时绑两个 GitHub 账号。
- **[Settings>Authentication 跟 ⑤ 的 Registration Policy 同位]** → ⑤ 会在该页加 "Registration Policy" 卡片；本 proposal 留好结构（页面顶部 oauth_provider list + 底部 placeholder "Registration Policy 即将上线" Alert）。

## Migration Plan

无破坏性。部署路径：

1. 本 proposal 后 schema 自动加 oauth_provider 表
2. 已部署且无 OAuth provider 配置 → /login 不显示 GitHub 按钮（query 返空列表）
3. Owner 主动配 → 启用 → /login 出现按钮

回滚：revert + 重启；oauth_provider 表残留无害。

## Open Questions

- **是否在 admin SPA 顶部强提示 "建议启用 OAuth 减少密码风险"** → 不做（self-host 用户判断力强；过度引导反 UX）。
- **是否记录 OAuth login audit log** → 是，但跟现有 audit log 同表（event_name=`oauth_login_succeeded` / `oauth_login_failed`）；audit_log 实体 ✓ 已存在，本 proposal 仅消费。
- **identity_link.metadata 存什么** → 存 ExternalIdentity.raw 原始 JSON（GitHub /user 返回的整 object）；future 排查/迁移有用，体积 ~2KB/row 可接受。
- **`/api/v1/auth/oauth/providers` 是否需要分页** → 不需要，MVP 一个 kind 一个 provider 几行而已。
