# add-openapi-and-admin-client

## Why

Admin SPA 与 CLI 都要消费 server API。手写 client 类型必然漂移；OpenAPI 自动生成在同 repo 同部署场景下是性价比最高的方案。

本 proposal 是**横切关注点**：建议在每个业务 proposal 落 handler 时同步加 utoipa 注解，而不是积压到最后一次性补齐。本 proposal 提供基础设施 + Admin client 生成脚本 + 自检流程。

## What

### 1. Server 侧

- 引入 `utoipa`、`utoipa-axum`、`utoipa-redoc`（或 swagger-ui）。
- 全部已落地 DTO 加 `#[derive(ToSchema)]`。
- 全部 handler 加 `#[utoipa::path(...)]` 描述（含 errors、scope、permission）。
- 启动期注册 `OpenApi` doc，暴露：
  - `GET /api/openapi.json`（生产开关，默认开；可关）。
  - `GET /api/docs`（Redoc UI，仅 dev / 经 auth）。

### 2. Admin client 生成

`apps/admin` 加 pnpm 脚本：

```jsonc
{
  "scripts": {
    "openapi:fetch": "node scripts/fetch-openapi.mjs",     // 调本地 server /api/openapi.json
    "openapi:gen": "openapi-typescript src/api/openapi.json -o src/api/types.gen.ts",
    "openapi": "pnpm openapi:fetch && pnpm openapi:gen"
  }
}
```

配 `openapi-fetch` 拿到全类型 client：

```ts
import createClient from "openapi-fetch";
import type { paths } from "@/api/types.gen";
export const api = createClient<paths>({ baseUrl: "/api" });
```

TanStack Query hooks 包一层（手写 → 后续若有 openapi-tanstack 成熟方案再切）。

### 3. CLI 侧

`swarmhive-cli` 通过 build script 把 `openapi.json` 转成 Rust 请求类型（candidate：openapi-generator 的 rust-axum-server 方案，或更轻量的 `progenitor`）。MVP 可以先**手写 client struct**，等业务稳定后再决定要不要全自动生成。

### 4. CI gate

- `pnpm --filter @swarmhive/admin openapi` 在 CI 中跑，diff 非空（types.gen.ts 与 git 不一致）就 fail。强制贡献者更新 client。
- 同样 lint：`cargo run --bin swarmhive-server -- --print-openapi > openapi.expected.json`，与已 commit 文件 diff。

## Acceptance

- `/api/openapi.json` 能完整列出 MVP 全部 endpoint。
- Admin SPA 使用生成的 `paths` 类型调 endpoint，类型错误能在编译期被 tsc 抓住。
- CI 能在 endpoint signature 漂移时挡住 PR。
- Redoc UI 在 dev 模式下能正确渲染（含 401 / 403 / 422 problem+json schema）。

## Non-goals

- 不暴露 OpenAPI 给外部用户作为公共 API 契约（MVP 只服务 Admin + CLI 自身）。
- 不做版本化 API（`/api/v1` 是手动维护，不切自动版本管理工具）。
- 不引入 GraphQL。
- 不强制 CLI 用自动生成 Rust client（先手写）。

## Depends on

- 任何已落地 handler 的 proposal（强烈建议从 `add-auth-and-rbac` 起就同步加注解）。

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) Admin 技术栈段（TanStack Query）。
- [CLAUDE.md](../../../CLAUDE.md) "Server ↔ Admin coupling" 段。
