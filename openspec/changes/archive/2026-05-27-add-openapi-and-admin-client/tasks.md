# tasks

## 1. Deps

- [x] 1.1 [code] `cargo search` 确认：utoipa 5.5.0 (workspace pin "5" 自动取最新)、utoipa-axum 0.2.0、utoipa-redoc 6.0.0（注意 utoipa-redoc 已脱钩到 6.x，跟 utoipa 5 兼容）
- [x] 1.2 [code] workspace `Cargo.toml` 加 `utoipa-axum = "0.2"` + `utoipa-redoc = { version = "6", features = ["axum"] }`
- [x] 1.3 [code] `crates/swarmhive-server/Cargo.toml` 引用 `utoipa-axum.workspace = true` + `utoipa-redoc.workspace = true`
- [x] 1.4 [code] `cargo check -p swarmhive-server` 通过（utoipa 5 + utoipa-axum 0.2 + utoipa-redoc 6 三者解析通过）

## 2. ApiError 错误响应抽象

- [x] 2.1 [code] `error.rs`：私有 `Problem` 改 `pub` + `derive(ToSchema)` + `#[schema(rename = "type")]` 处理 `type_uri` → wire `type` 字段
- [x] 2.2 [code] 新增 `ApiErrorResponses` enum + `derive(IntoResponses)`：7 variant 各对应一个 status（401/403/404/409/410/422/500），全部 ref `Problem` schema 与 `application/problem+json` content-type
- [x] 2.3 [test] 验证留给 group 6 的 openapi_surface 集成测试（实际遍历 OpenAPI doc 检查更可靠，比 unit 测 IntoResponses 内部数据结构更稳定）

## 3. OpenApi 顶层 doc

- [x] 3.1 [code] 新建 `crates/swarmhive-server/src/openapi.rs`：`#[derive(OpenApi)] struct ApiDoc` + info(title, description, license) + 5 个 tag（version 走 utoipa 默认 = `CARGO_PKG_VERSION`，无需显式声明）
- [x] 3.2 [code] `src/lib.rs` 注册 `pub mod openapi;`

## 4. Handler 注解（按 tag 分组逐个补）

- [x] 4.1 [code] `routes/health.rs`：`HealthResponse` derive ToSchema，handler 加 `#[utoipa::path]`（200 + 503 双响应），router 改 `OpenApiRouter`
- [x] 4.2 [code] `routes/version.rs`：新增 `VersionResponse` struct + ToSchema，handler 加注解，router 改 `OpenApiRouter`
- [x] 4.3 [code] `routes/auth.rs`：login / logout / me 加 `#[utoipa::path]`；`LoginReq`、`MeResponse` derive ToSchema；router 改 `OpenApiRouter` 用三次独立 `.routes()`（utoipa-axum `routes!(a, b, c)` 是 "同 path 不同 method 共享"，path 不同必须分开）
- [x] 4.4 [code] `routes/setup.rs`：info / register 加注解；`SetupReq`、`SetupInfo` derive ToSchema；router 改 `OpenApiRouter`
- [x] 4.5 [code] `routes/demo.rs`：release_publish 加注解，tag="internal"，description 提到 "Removed by add-app-release-artifact"，router 改 `OpenApiRouter`

## 5. Build router 集成

- [x] 5.1 [code] `src/lib.rs::build_router`：所有 routes 合并到一个 `OpenApiRouter::with_openapi(ApiDoc::openapi())`，调 `.with_state(state).split_for_parts()` 拆出 `(axum::Router, OpenApi)`
- [x] 5.2 [code] `utoipa_redoc::Redoc::with_url("/api/docs", openapi)` + `Servable` trait 提供 `Router::merge()` 集成
- [x] 5.3 [code] `axum::routing::get(...)` 闭包返回 `Json(doc)` 暴露 `/api/openapi.json`
- [x] 5.4 [code] 这两个 endpoint 装在 root（session_layer 之外、governor_layer 之外，公开无 auth）
- [x] 5.5 [code] **附带升级**：把 workspace axum 0.7 → 0.8（utoipa-axum 0.2 要求），tower-sessions 0.13 → 0.15，tower_governor 0.4 → 0.8。改 extractor 去掉 `#[async_trait]`（axum 0.8 native async fn in trait），改 `GovernorLayer::new(conf)`（字段私有）。`cargo clippy --workspace --all-targets -- -D warnings` 零警告；现有 5 unit + 4 auth_smoke + 2 db_smoke 全部保持通过

## 6. 集成测试

- [x] 6.1 [test] 新建 `crates/swarmhive-server/tests/openapi_surface.rs`：复用 testcontainer Postgres + boot() helper（跟 auth_smoke 一致），5 个 test
- [x] 6.2 [test] `openapi_json_lists_all_endpoints_with_tags_and_schemas`：8 个 endpoint paths + 5 tags + 10 schemas + info.title/version 校验 + internal endpoint description 含 removal 提示
- [x] 6.3 [test] `error_bearing_endpoints_inherit_full_api_error_response_set`：6 个走 ApiError 的 endpoint 各含 401/403/404/409/410/422/500 七个 status + ref `#/components/schemas/Problem`
- [x] 6.4 [test] `problem_schema_uses_wire_name_type_not_type_uri`：Problem.properties 有 "type" 无 "type_uri"，required = ["type","title","status","detail"]
- [x] 6.5 [test] `redoc_ui_serves_html_at_api_docs`：/api/docs 200 + text/html
- [x] 6.6 [test] `openapi_endpoints_bypass_rate_limit`：连续 50 次 fetch /api/openapi.json 不触发 429
- [x] 6.7 [test] 已有 5 unit + 4 auth_smoke + 2 db_smoke 全部保持通过
- [x] 6.8 [code] **附带修正**：utoipa 5 的 IntoResponses derive 生成 `$ref: "#/components/schemas/Problem"` 但**不**自动把 Problem 注册到 components.schemas。在 ApiDoc 用 `components(schemas(crate::error::Problem))` 显式注册；spec scenario 同步收窄到本次实际暴露的 10 个 schema（Permission / Role 留给未来 role-management 落地时）
- [x] 6.9 [code] **端到端验证**：本地 docker `postgres:17` 容器（5433 端口避开冲突）+ workspace root `config/default.toml`；`cargo run -p swarmhive-server` 启动，curl 验证 /healthz / /api/v1/version / /api/v1/setup/info / /api/openapi.json (8 paths, 5 tags, 10 schemas, Problem 含 wire-name "type") / /api/docs (text/html 14KB Redoc bundle) 全部 200 + 内容符合预期

## 7. Docs 同步

- [x] 7.1 [docs] [docs/03-architecture.md](../../../docs/03-architecture.md) 新增 "OpenAPI 暴露面" 段（在 "演进方向" 与 "仓库组织" 之间）：endpoint 路径、tag 划分、为什么公开、ApiError IntoResponses 模式 + components(schemas(Problem)) 注册技巧、utoipa-axum routes! 同 path 不同 method 的语义、未来 securitySchemes 留给 pat-and-api-token
- [x] 7.2 [docs] [CLAUDE.md](../../../CLAUDE.md) 加 docker run swarmhive-pg 启动命令 + 更新 `cargo run -p swarmhive-server` 注释（含 OpenAPI / Redoc endpoint + `config/default.toml` 读取路径 + env override 模式）+ `cargo test` 提到 openapi_surface
- [x] 7.3 [docs] [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 9 任务首项标 ✅ "OpenAPI 文档基础设施"
- [x] 7.4 [docs] [openspec/changes/README.md](../README.md) 当前进度表加 add-openapi-and-admin-client 行
