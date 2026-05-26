# tasks

## 1. Deps

- [x] 1.1 [code] `cargo search` 确认：utoipa 5.5.0 (workspace pin "5" 自动取最新)、utoipa-axum 0.2.0、utoipa-redoc 6.0.0（注意 utoipa-redoc 已脱钩到 6.x，跟 utoipa 5 兼容）
- [x] 1.2 [code] workspace `Cargo.toml` 加 `utoipa-axum = "0.2"` + `utoipa-redoc = { version = "6", features = ["axum"] }`
- [x] 1.3 [code] `crates/swarmhive-server/Cargo.toml` 引用 `utoipa-axum.workspace = true` + `utoipa-redoc.workspace = true`
- [x] 1.4 [code] `cargo check -p swarmhive-server` 通过（utoipa 5 + utoipa-axum 0.2 + utoipa-redoc 6 三者解析通过）

## 2. ApiError 错误响应抽象

- [ ] 2.1 [code] `crates/swarmhive-server/src/error.rs`：把私有 `Problem` struct 改 `pub`，加 `derive(ToSchema)`，加 `#[schema(rename = "type")]` 处理 `type_uri` → `type` 的 serde rename
- [ ] 2.2 [code] `impl utoipa::IntoResponses for ApiError`：7 个变体 → 6 个唯一 status（401/403/404/409/410/422/500），每个 status response 引用 `Problem` schema，`Content-Type: application/problem+json`
- [ ] 2.3 [test] unit test：构造 `ApiError::IntoResponses::responses()` 返回值，断言 status 集合完整 + 每个 status 引用 Problem schema

## 3. OpenApi 顶层 doc

- [ ] 3.1 [code] 新建 `crates/swarmhive-server/src/openapi.rs`：定义 `#[derive(OpenApi)] struct ApiDoc` + `info` 元数据（title="SwarmHive API", version=`env!("CARGO_PKG_VERSION")`, description 一句话）+ `tags = [health, version, auth, setup, internal]`（internal tag description 写 "Removed by add-app-release-artifact"）
- [ ] 3.2 [code] `src/lib.rs` 注册 `pub mod openapi;`

## 4. Handler 注解（按 tag 分组逐个补）

- [ ] 4.1 [code] `routes/health.rs`：handler `health` 加 `#[utoipa::path(get, path="/healthz", responses(...), tag="health")]`；`router()` 返回 `OpenApiRouter<AppState>` 用 `routes!(health)`
- [ ] 4.2 [code] `routes/version.rs`：同上，path="/api/v1/version", tag="version"
- [ ] 4.3 [code] `routes/auth.rs`：login / logout / me 三个 handler 加 `#[utoipa::path]`，request_body / responses 完整。`LoginReq`、`MeResponse` 加 `derive(ToSchema)`。router 改 `OpenApiRouter`
- [ ] 4.4 [code] `routes/setup.rs`：info / register 两个 handler 加注解。`SetupReq`、`SetupInfo` 加 `derive(ToSchema)`。router 改 `OpenApiRouter`
- [ ] 4.5 [code] `routes/demo.rs`：release_publish handler 加注解，tag="internal"，description 提到 "Removed by add-app-release-artifact"。router 改 `OpenApiRouter`

## 5. Build router 集成

- [ ] 5.1 [code] `src/lib.rs::build_router`：把所有 routes 合并到一个 `OpenApiRouter<AppState>`，调 `.split_for_parts()` 拆出 `(axum::Router, OpenApi)`
- [ ] 5.2 [code] 用 `utoipa_redoc::Redoc::with_url("/api/docs", openapi.clone())` 暴露 Redoc UI
- [ ] 5.3 [code] 用 `axum::routing::get(move || async move { Json(openapi) })` 暴露 `/api/openapi.json`
- [ ] 5.4 [code] 这两个 endpoint 装在 root（session_layer 之外、governor_layer 之外）
- [ ] 5.5 [code] `cargo clippy --workspace --all-targets -- -D warnings` 零警告

## 6. 集成测试

- [ ] 6.1 [test] 新建 `crates/swarmhive-server/tests/openapi_surface.rs`：不需要 Postgres，构造最小 AppState（或直接拉 OpenApi struct）
- [ ] 6.2 [test] `GET /api/openapi.json` → 200 + JSON parse 成功 + `info.version` 非空
- [ ] 6.3 [test] paths 包含 8 个已知 endpoint 路径
- [ ] 6.4 [test] 每个 endpoint 的 responses 含 401 / 403 / 422 / 500 四个 status 且 ref Problem schema
- [ ] 6.4 [test] components.schemas 含 User / Permission / Role / LoginReq / SetupReq / MeResponse / SetupInfo / Problem
- [ ] 6.5 [test] `GET /api/docs` → 200 + Content-Type text/html
- [ ] 6.6 [test] Problem schema 的 properties 含 `type`（不是 `type_uri`）
- [ ] 6.7 [test] 已有 5 unit + 4 auth_smoke + 2 db_smoke 全部保持通过

## 7. Docs 同步

- [ ] 7.1 [docs] [docs/03-architecture.md](../../../docs/03-architecture.md) 补一段 "OpenAPI 暴露面"：endpoint 路径、tag 划分、为什么公开、未来 securitySchemes 计划
- [ ] 7.2 [docs] [CLAUDE.md](../../../CLAUDE.md) `cargo run -p swarmhive-server` 命令注释里增加 `/api/openapi.json`、`/api/docs`
- [ ] 7.3 [docs] [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 9 任务里标 ✅ "OpenAPI 基础设施"，⏳ "admin client 生成 / CI gate / CLI client" 仍为后续 follow-up
- [ ] 7.4 [docs] [openspec/changes/README.md](../README.md) 当前进度表加 add-openapi-and-admin-client ⏳ 行；架构图末尾说明 "本期不做 admin client / CI gate / CLI client"
