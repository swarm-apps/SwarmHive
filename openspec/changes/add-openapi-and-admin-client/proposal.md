# add-openapi-and-admin-client

## Why

Admin SPA 与 CLI 都要消费 server API。手写 client 类型必然漂移；OpenAPI 自动生成在同 repo 同部署场景下是性价比最高的方案。但更紧迫的是：`add-auth-and-rbac` 已经落地了 8 个 handler 但 0 个有 utoipa 注解。proposal 自己的 Why 段写过 "建议在每个业务 proposal 落 handler 时同步加 utoipa 注解，而不是积压到最后一次性补齐" —— 我们已经积压。下一个 proposal（`add-pat-and-api-token`）又会引入 3-4 个 endpoint，再不做积压只会变大。

本 proposal 提供 **server 侧 OpenAPI 基础设施**：utoipa-axum + IntoResponses 抽象 + Redoc UI + 给现有 8 个 handler 补注解。**不做** admin client 生成 + CI gate + CLI client（拆到 follow-up），让本次能快速闭环。

## What Changes

- **新增 deps（workspace）**：`utoipa-axum = "0.2"` + `utoipa-redoc = "5"`（utoipa 5 已 pin）
- **新增模块** `crates/swarmhive-server/src/openapi.rs`：`OpenApi` struct 定义 + tag 列表 + `info` 元数据
- **扩展 `ApiError`**：`impl utoipa::IntoResponses` 让 7 个变体一次性映射到 6 个 HTTP status + problem+json schema；handler 的 `#[utoipa::path]` 只需要 `responses(..., ApiError)` 即可
- **新增 `Problem` schema**：error.rs 里现存的私有 `Problem` struct 改为 `pub` + `derive(ToSchema)`，作为 problem+json body schema 暴露
- **改 router 装配**：`build_router` 用 `utoipa_axum::router::OpenApiRouter` 替代 `axum::Router`；handler 通过 `routes!(login, logout, me)` 宏自动收集 path 到 OpenApi doc，无需手列
- **给 8 个现有 handler 加 `#[utoipa::path(...)]`**：health / version / auth × 3 / setup × 2 / demo × 1，按 tag 分组（health / version / auth / setup / internal）
- **补 server-local DTO 的 ToSchema derive**：`LoginReq` / `SetupReq` / `MeResponse` / `SetupInfo` 在 routes 文件里就地 derive
- **新增 endpoint**：
  - `GET /api/openapi.json` —— 公开（无 auth），返回完整 OpenAPI 3.1 文档
  - `GET /api/docs` —— Redoc UI，公开
- **集成测试**：`tests/openapi_surface.rs` 拉 `/api/openapi.json`，断言 paths 列出已知 endpoint + components 含 Problem schema + 各 endpoint 的 responses 含 ApiError 映射出的错误码

## Capabilities

### New Capabilities

- `openapi-surface`：server 暴露的 OpenAPI 文档面，含 endpoint 注解规范、错误响应统一映射、Redoc UI、JSON endpoint 公开策略。

### Modified Capabilities

无。本次未触及现有 spec-level 行为契约（所有 handler 的请求/响应字段不变）。

## Impact

- **Code**：`crates/swarmhive-server/src/{openapi.rs,lib.rs,error.rs,routes/*.rs}` —— 新增 openapi.rs（~80 行）、扩展 ApiError IntoResponses (~50 行)、改 build_router (~10 行)、8 个 handler 各加 #[utoipa::path] (~15 行/handler)。
- **API**：新增 2 个公开 endpoint（`/api/openapi.json`、`/api/docs`），不修改现有 endpoint 行为。
- **Deps**：workspace 新增 utoipa-axum + utoipa-redoc；server crate 引用。
- **Test**：`tests/openapi_surface.rs` 新增 ~80 行（in-process oneshot，无需 Postgres）。
- **Docs**：docs/03-architecture.md 补一段 "OpenAPI 暴露"；CLAUDE.md 启动命令段补 `/api/openapi.json` + `/api/docs`。
- **不影响**：admin SPA（不在此 proposal 接 typed client）、CLI（不在此 proposal 生成 Rust client）、CI（不在此 proposal 加 drift gate）。这三项是有意 Deferred 的下游工作，本次决定打好基础设施先用起来。

## Non-goals

- **不做 admin SPA 接入 typed client**：openapi-typescript / openapi-fetch 集成、TanStack Query 包装层 —— 留给后续 admin SPA 推进时处理（届时已有 `/api/openapi.json` 可消费）。
- **不做 CI drift gate**：`pnpm openapi && git diff --exit-code` 阻断 PR —— 同上，等 admin client 接入后再做。
- **不生成 CLI Rust client**：progenitor / openapi-generator —— proposal 原文也说 "MVP 先手写"，本次保留手写。
- **不版本化 API**：`/api/v1` 是手写路径前缀，不切自动版本管理。
- **不暴露给外部作为公共 API 契约**：MVP 只服务 Admin SPA + CLI 自身。

## Depends on

- `add-auth-and-rbac`（已归档）—— 提供 8 个待注解的 handler、`ApiError` 类型、`LoginReq` / `SetupReq` 等 DTO。

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) Admin 技术栈段 + "Server ↔ Admin coupling" 段。
- [CLAUDE.md](../../../CLAUDE.md) Common commands 段（新增 endpoint 提示）。

## Acceptance

- `cargo run -p swarmhive-server` 后 `curl -s http://localhost:3030/api/openapi.json | jq '.paths | keys'` 包含 `/healthz`、`/api/v1/version`、`/api/v1/auth/login`、`/api/v1/auth/logout`、`/api/v1/auth/me`、`/api/v1/setup/info`、`/api/v1/setup`、`/api/v1/_demo/release-publish`。
- 任意 endpoint 的 `responses` 至少含 `401`、`403`、`422`、`500` 四个 problem+json 响应（由 ApiError IntoResponses 自动带入）。
- 浏览器开 `http://localhost:3030/api/docs` 显示 Redoc UI，5 个 tag（health / version / auth / setup / internal）分组正确。
- `cargo test -p swarmhive-server` 全绿（新增 `openapi_surface` 测试通过，原有 5 unit + 4 integration + 2 db_smoke 保持通过）。
- `cargo clippy --workspace --all-targets -- -D warnings` 零警告；`cargo fmt --check` 通过。
