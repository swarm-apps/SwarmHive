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
