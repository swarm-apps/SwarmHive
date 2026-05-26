# design

## Context

`add-auth-and-rbac` 落地了 8 个 handler + 一个完整的 `ApiError` problem+json 体系，但 0 个 handler 有 utoipa 注解。当前 server 没有任何机器可读 API 文档面：

- Admin SPA 未来要消费 API，没 schema 就得手写每个 request/response 类型，跟 server 漂移；
- CLI 写命令时也得手写 client，验证签名靠 grep；
- 任何外部贡献者 onboarding 没有 endpoint 索引可看。

约束：

- workspace 当前 pin `utoipa = "5"`。本次顺势把 utoipa 系列统一升到 crates.io 当前最新 stable major（如果有 utoipa 6 / 7 已发布），新增的 `utoipa-axum` / `utoipa-redoc` 也 pin 同代最新 stable —— 一次性把这个生态拉齐，避免后面单独升级 axum/redoc 时 minor 不兼容。Apply 时先 `cargo search utoipa utoipa-axum utoipa-redoc` 确认当前最新，再在 `[workspace.dependencies]` 一并 pin。
- handler 现状是 `axum::Router` 通过 `.merge()` 装配，session/governor layer 都已 wire；改 router 类型要兼容现有层。
- `ApiError` 是 8 个 handler 唯一的 `Rejection`，错误响应注解要一处定义全处生效。
- 项目惯例（dev-notes/knowledge/backend.md）：所有 4xx/5xx 走 RFC 9457 `application/problem+json`。

## Goals / Non-Goals

**Goals:**

- 给现有 8 个 handler 加 OpenAPI 注解，全部进入 `/api/openapi.json`。
- 错误响应（401/403/404/409/410/422/500）由 `ApiError` 一次性 doc，handler 注解零重复。
- 暴露 Redoc UI（`/api/docs`）便于本地调试 / 新人 onboarding。
- 接下来的 `add-pat-and-api-token` 一开始就在轨道上（OpenApiRouter + utoipa::path 是新 handler 的默认形态）。

**Non-Goals:**

- 不接入 admin SPA 的 typed client（openapi-typescript + openapi-fetch + TanStack Query 包装层）。
- 不做 CI drift gate。
- 不生成 CLI 的 Rust client（progenitor / openapi-generator）。
- 不切 API 自动版本管理；`/api/v1` 是手写前缀。
- 不把 OpenAPI 作为公共 API 契约对外承诺。

## Decisions

### 1. utoipa-axum `OpenApiRouter` 替代 `axum::Router`

`utoipa-axum::router::OpenApiRouter<S>` 是 utoipa 5 内置的 axum 集成层。在 routes 模块用：

```rust
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login, logout, me))
}
```

`routes!` 宏会读取 handler 上的 `#[utoipa::path]` 注解，自动把 path 收集到 OpenApi doc。`build_router` 在顶层调 `.split_for_parts()` 得到 `(axum::Router, utoipa::openapi::OpenApi)`，前者照常 serve，后者用 `utoipa_redoc::Redoc` + `Json(openapi)` 暴露给 `/api/docs` 与 `/api/openapi.json`。

**Why over 手列 paths**：手列要求每加一个 handler 都改 `#[derive(OpenApi)] struct Doc { paths(routes::auth::login, ...) }` 列表，新人忘改就漏 doc。`routes!` 宏让 handler 注册的"出现"与"被收录"绑定在同一处代码。

**Alternatives**：

- 纯 `#[derive(OpenApi)]` + 手列 paths：现存 8 个 handler 可行，未来 30+ 个 handler 会持续摩擦。
- `utoipa-actix-web` 等其他框架适配器：不适用（我们用 axum）。

### 2. `impl utoipa::IntoResponses for ApiError`

`ApiError` 是所有 handler 的 `Rejection` 类型。一次性 impl `IntoResponses` 让 handler 注解写：

```rust
#[utoipa::path(
    post, path = "/api/v1/auth/login",
    request_body = LoginReq,
    responses(
        (status = 200, body = User, description = "Authenticated"),
        ApiError,
    ),
    tag = "auth",
)]
```

`ApiError` 标识符自动展开成 7 个变体（401/403/404/409/410/422/500）+ 各自的 `Problem` schema 引用。

**Why**：8 个 handler × 4-6 个错误码 = ~30+ 重复行如果每个 handler 手列。集中后未来加新错误变体只改 error.rs 一处。这同时是 [feedback-abstraction-timing](../../../memory/feedback_abstraction_timing.md) 的正例 —— 8 个真实 consumer，抽象成本完全摊薄。

**Alternatives**：

- 每 handler 手列响应：boilerplate 重灾区。
- 用 `responses = (default = ApiError)` 一刀切：丢失各 status 的语义区分，Redoc 渲染只看到一个 "default"。

### 3. `Problem` schema 公开

`error.rs` 现存私有 `Problem { type_uri, title, status, detail, required_permission }` struct。改 `pub` + `derive(ToSchema)`，让所有错误响应都 ref 到同一个 schema。

注意 `type` 字段在 serde 里 rename 自 `type_uri`；utoipa 要识别这个 rename，需要 `#[schema(rename_all = "...")]` 或字段级 `#[schema(rename = "type")]` 显式标。

### 4. Tag 划分（5 个，drives Redoc 左侧导航）

| Tag | endpoint |
|---|---|
| `health` | `/healthz` |
| `version` | `/api/v1/version` |
| `auth` | `/api/v1/auth/{login,logout,me}` |
| `setup` | `/api/v1/setup{,info}` |
| `internal` | `/api/v1/_demo/release-publish`（description 标注 "Removed by add-app-release-artifact"） |

未来 `add-app-release-artifact` 加 `apps` / `releases` / `artifacts` tag，`add-pat-and-api-token` 加 `tokens` tag。

### 5. `/api/openapi.json` 与 `/api/docs` 公开访问

不要求 auth。SwarmHive 是 self-hosted 内部部署，OpenAPI doc 不泄露敏感数据（path / schema / 错误码不算敏感）。类比 GitHub 公开自己的 swagger。

挂在 root router（session layer 之外），跟 `/healthz` / `/api/v1/version` 一起。governor 层不挂这两个 endpoint（doc 拉取频繁是正常的，调试时反复刷 Redoc 不该被限流）。

**Why over auth-required**：要 auth 意味着 admin SPA 生成 client 时必须先 login 拿 cookie。dev/CI 流程都更绕。proposal 的 Non-goals "不作为公共 API 契约" 已经从语义上限制了它的承诺面，访问控制再加一层是过度防御。

### 6. `info.version` 跟随 cargo

`info.version = env!("CARGO_PKG_VERSION")`，跟 `/api/v1/version` endpoint 返回的版本一致。`info.title = "SwarmHive API"`，`info.description` 一句话："Self-hosted update distribution hub for Tauri desktop + React Native Android apps."

不在 OpenApi 里硬编码 `servers` —— 部署形态多变（dev `:3030`、prod 域名各异、单 binary 内嵌 SPA 时同源），让客户端从相对路径推断。

### 7. utoipa-axum 与 governor / session layer 的顺序

`build_router` 当前结构：

```text
root Router
├── /healthz, /api/v1/version, /api/v1/_demo/* (无限流)
└── sensitive Router (auth + setup, 套 governor)
顶层套 session layer
```

改造后：

```text
top: split_for_parts() 出最终 Router
        |
        +-- API Router (utoipa-axum 收集)
        |      ├── /healthz, /version, /_demo/*
        |      └── sensitive (auth + setup, governor)
        |      顶层 session layer
        |
        +-- /api/openapi.json    (axum::routing::get)
        +-- /api/docs            (utoipa-redoc Redoc)
```

OpenAPI doc 注册路径与 governor / session 无关 —— 它们只在 `OpenApiRouter` 收集 path 时被感知，运行时层次不变。

## Risks / Trade-offs

- **[utoipa-axum 0.2 相对新，可能跟 axum 0.7 / utoipa 5 边缘场景不兼容]** → 早期 cargo check 立刻能跑出来；不行则降级到方案 1（手列 paths），影响只在 router wiring 而非 handler 注解。
- **[`ApiError::IntoResponses` 自动展开后 handler 注解描述能力下降]** → 个别 handler 想给特定错误码补充 description 时，仍可在 `responses(...)` 里手列那个 status 覆盖。
- **[公开 /api/openapi.json 暴露所有 endpoint 名 + schema 给未认证扫描器]** → SwarmHive 是 self-hosted 内部部署，攻击面是部署者已知的；未来若需要"私有 API 文档"形态，加一个 `SWARMHIVE_OPENAPI_PUBLIC=false` config 开关即可，schema 层零改动。
- **[Redoc UI 多一个 dep + ~200KB 资源]** → utoipa-redoc 把 Redoc bundle 编译进 binary（单 binary 部署形态契合），构建产物 +200KB 可接受。
- **[Problem schema 的 serde rename "type" 与 utoipa 注解可能踩坑]** → 测试覆盖：openapi_surface 集成测试断言 `components.schemas.Problem.properties` 含 `type` 字段（不是 `type_uri`）。

## Migration Plan

无需 migration —— 全部新增 endpoint 与注解，不改任何现有行为。部署单 binary 后 `/api/openapi.json` 与 `/api/docs` 立即可用。

回滚：revert commit 即可，无 DB schema 改动、无 wire protocol 改动。

## Open Questions

- **Servers 列表要不要列**：当前决策不列（部署形态多变）。若 admin SPA 生成 client 时需要"已知 base URL"，再补 `servers = [{ url = "/", description = "current host" }]`。
- **未来 `add-pat-and-api-token` 引入 bearer auth 时**，OpenAPI 的 `securitySchemes` 怎么定义（`http` bearer scheme + `cookieAuth` apiKey 并存）—— 留给那个 proposal 处理，本次不预设。
