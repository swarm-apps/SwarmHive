# add-oauth-github

## Why

docs/13 要求 MVP 即支持 GitHub OAuth 登录（与 email-password 并存）。OAuth flow 模型独立于密码登录基础，单独成块可以保证 `add-auth-and-rbac` 的密码路径不被 OAuth 复杂度污染。

## What

- 引入 `oauth2` crate。
- `swarmhive-server::auth` 中定义 `IdentityProvider` trait：

  ```rust
  #[async_trait]
  pub trait IdentityProvider {
      fn name(&self) -> &'static str;            // "github"
      fn authorize_url(&self, state: &str) -> Url;
      async fn exchange(&self, code: &str) -> Result<ExternalIdentity>;
  }
  pub struct ExternalIdentity {
      pub subject: String,        // GitHub user id
      pub email: Option<String>,
      pub display_name: Option<String>,
      pub avatar_url: Option<String>,
      pub raw: serde_json::Value, // 存进 IdentityLink.metadata
  }
  ```

- `GitHubProvider` 实现：调 `https://github.com/login/oauth/authorize` + token 交换 + `/user` + `/user/emails`。
- Server endpoints：
  - `GET /api/v1/auth/oauth/:provider/start` → 生成 state + PKCE → 重定向到 provider。
  - `GET /api/v1/auth/oauth/:provider/callback` → 校验 state → exchange → 查 / 建 IdentityLink → 写 session cookie。
- 邮箱冲突处理：若 GitHub email 已被 password 用户占用，**强制要求先用密码登录后再绑定**，避免账号合并漏洞。
- Admin SPA 加 "Sign in with GitHub" 按钮（由 `add-openapi-and-admin-client` 暴露的 endpoint 列表渲染）。

## Acceptance

- 用户从 Admin SPA 点 GitHub 登录 → 跳转 → 回跳 → 已登录。
- 已用密码登录的用户，可在 "Profile → Linked accounts" 绑定 GitHub（同 callback，绑定模式）。
- GitHub 邮箱冲突 → 返回 409 + 引导用户先用密码登录。
- 集成测试：用 oauth2 crate 的 mock server 跑完整 flow。

## Non-goals

- 不实现 Google / GitLab / 内部 OIDC（trait 留好，本 proposal 只挂 GitHub）。
- 不实现 GitHub Enterprise（host 可在 config 改，但不测试矩阵）。
- 不做 SCIM 用户同步。

## Depends on

- `add-auth-and-rbac`

## Maps to docs

- [docs/13-rbac.md](../../../docs/13-rbac.md) Identity Providers。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 2。
