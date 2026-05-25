# design

## 鉴权数据流

```
Browser                  axum Router                    swarmhive-server
   │                          │                              │
   │  POST /auth/login        │                              │
   │  {email, password}       │                              │
   ├─────────────────────────▶│                              │
   │                          │  AuthService::login()        │
   │                          ├─────────────────────────────▶│
   │                          │                              │  ① 查 user by email
   │                          │                              │  ② 查 user_credentials
   │                          │                              │  ③ argon2 verify
   │                          │                              │  ④ 写 audit_log(login_succeeded)
   │                          │                              │  ⑤ 创建 session row
   │                          │  Ok(SessionInfo)             │
   │                          │◀─────────────────────────────┤
   │                          │  设置 Set-Cookie             │
   │  200 + Set-Cookie        │                              │
   │◀─────────────────────────┤                              │
   │                          │                              │
   │  GET /api/v1/apps        │                              │
   │  Cookie: swarmhive_…     │                              │
   ├─────────────────────────▶│                              │
   │                          │  Principal extractor         │
   │                          ├─────────────────────────────▶│  解析 session_id → 查 session/user/permissions
   │                          │◀──── Principal ──────────────┤
   │                          │                              │
   │                          │  RequirePermission(app:read) │
   │                          │  → 检查 Principal.permissions│
   │                          │                              │
   │                          │  handler(...)                │
   │  200                     │                              │
   │◀─────────────────────────┤                              │
```

## Principal extractor 实现要点

```rust
#[async_trait]
impl<S> FromRequestParts<S> for Principal
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // 1. Bearer token（PAT / API Token，由后续 proposal 实现）
        if let Some(bearer) = parse_bearer(parts) {
            return resolve_bearer(&app_state, bearer).await;   // stub: 本 proposal 返回 Unauthorized
        }

        // 2. Cookie session
        if let Some(session_id) = parse_session_cookie(parts) {
            return resolve_session(&app_state, session_id).await;
        }

        Err(ApiError::unauthorized())
    }
}
```

`resolve_session` 内部：

1. `Session::find_by_id(session_id)`，校验 `expires_at > now()`。
2. 关联加载 `User → UserRole → Role → Permission`（用 SeaORM Entity Loader 一次拉齐）。
3. 更新 `session.last_seen_at`（异步 batch，不阻塞请求）。

## Permission middleware

两种风格备选：

### 风格 A：宏

```rust
async fn publish_release(
    p: Principal,
    State(app): State<AppState>,
    Path(app_slug): Path<String>,
    Json(req): Json<PublishReq>,
) -> Result<Json<Release>, ApiError> {
    require_permission!(p, "release:publish", Scope::App(app_id))?;
    ...
}
```

### 风格 B：extractor

```rust
async fn publish_release(
    _: RequirePermission<"release:publish">,
    ...
) -> Result<Json<Release>, ApiError> { ... }
```

**倾向风格 A**：scope 在 handler 里动态解析（依赖 path param），用 extractor 表达不自然。

## Bootstrap Owner 流程

```
   首次启动 (user 表为空)
        │
        ▼
   生成一次性 setup_token (32 字节随机 + base64url)
   存 DB: setup_tokens(token_hash, expires_at, used_at)
        │
        ▼
   stdout 打印：
   ════════════════════════════════════════════════════════
   SwarmHive first-run setup
   Open: http://<your-host>/setup?token=<setup_token>
   This token is one-shot and expires in 1 hour.
   ════════════════════════════════════════════════════════
        │
        ▼
   /setup 页面（Admin SPA）让用户填 email + display name + 密码
        │
        ▼
   POST /api/v1/setup → 校验 token → 创建 Owner User + IdentityLink(provider=password)
                     → user_credentials → 标记 setup_token used
                     → 自动登录（写 session cookie）
```

## AuthMethod 枚举

```rust
pub enum AuthMethod {
    Session { session_id: Uuid },
    Pat { token_id: Uuid },                  // 待 add-pat-and-api-token
    ApiToken { token_id: Uuid, scope: Scope }, // 待同上
}
```

AuditLog 里 `actor_type` 记录这个，方便区分浏览器操作 vs CI/CD 操作。

## 错误响应（RFC 9457）

```json
{
  "type": "https://swarmhive.dev/errors/forbidden",
  "title": "Forbidden",
  "status": 403,
  "detail": "Missing permission: release:publish",
  "instance": "/api/v1/apps/swarmdrop/releases",
  "required_permission": "release:publish",
  "scope": "app:swarmdrop"
}
```

`Content-Type: application/problem+json`。

## Risks

- argon2 默认参数在 server 启动期可能慢（每次登录 100ms+）。Mitigation: 取 OWASP 2024 推荐参数（m=19456 KiB, t=2, p=1），benchmark 确保 <150ms。
- bootstrap setup token 落 stdout 在 docker 场景需要看 `docker logs`。Mitigation: 在文档明示；提供 `swarmhive setup-token print` CLI 命令兜底（拆到 `add-pat-and-api-token` 或单独小 proposal）。
- session sliding 与并发请求竞争更新 `last_seen_at`。Mitigation: 异步 batch 更新；写入用 `UPDATE … WHERE last_seen_at < now() - 30s` 节流。

## Open questions

- "禁用用户" 与 "失效所有 session" 的关系：是 user.status = Disabled 时自动批 session？还是后台手动踢？倾向自动批。
- AuditLog 的 metadata 字段要不要 schemaful？倾向 JSONB + ad-hoc 字段；后续做 admin filter 时再加 GIN 索引。
