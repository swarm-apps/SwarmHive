# tasks

> 实施顺序：schema/DTO 地基（1、2）→ error 横切（3）→ endpoint 逻辑（4）→ 测试（5）→ 文档（6）。
> 多数为纯 server 端改动；**不动 CLI**、不动已上传产物数据。
> 编号后的 (Fn) 标注对应对抗 review 修复项。

## 1. 依赖 + release schema 扩展

- [x] 1.1 [code] 根 `Cargo.toml` `[workspace.dependencies]` 加 `semver = "1"`；`crates/swarmhive-server/Cargo.toml` 加 `semver.workspace = true`
- [x] 1.2 [code] `crates/swarmhive-entity/src/release.rs` Model 加 `min_version: Option<String>` + `rollout_percent: Option<i16>`（两列 nullable，走 schema-sync 安全路径，**不加** `#[sea_orm(default)]` / partial index）；`From<&Model> for api::Release` 同步两字段
- [x] 1.3 [code] `crates/swarmhive-api-types/src/release.rs`：`Release` DTO 加 `min_version: Option<String>` + `rollout_percent: Option<i16>`；`UpdateReleaseRequest` 加这两个 `#[serde(default)]` 字段
- [x] 1.4 [code] `routes/releases.rs::create_release`：(a) `release::ActiveModel` 补 `min_version: Set(None)` + `rollout_percent: Set(None)`；(b) **(F3) 入口加 semver 校验**——`semver::Version::parse(strip_v(&req.version))` 失败 → 422 typed `invalid-release-version`，杜绝坏 version 在分发时被静默 204
- [x] 1.5 [code] `routes/releases.rs::update_release`：处理两新字段——`rollout_percent` 限 `Some(1..=100)`（越界/0 → 422 typed `invalid-rollout-percent`）；`min_version` 非空时 `semver::Version::parse(strip_v(..))` 校验（失败 → 422 typed `invalid-min-version`）；通过则 `Set`。**(F6) 沿用 `Some → Set / None → 不动` 范式，不支持 PATCH 回 NULL**（清空走边界值：`min_version="0.0.0"` / `rollout_percent=100`）

## 2. api-types：Tauri update 响应 DTO

- [x] 2.1 [code] 新建 `crates/swarmhive-api-types/src/update.rs`：
  - `TauriUpdateResponse { version: String, #[serde(skip_serializing_if="Option::is_none")] pub_date: Option<String>, url: String, signature: String, #[serde(skip_serializing_if="Option::is_none")] notes: Option<String>, swarmhive: TauriUpdateExtensions }`
  - `TauriUpdateExtensions { upgrade_type: UpgradeType, #[serde(skip_serializing_if="Option::is_none")] min_version: Option<String>, rollout_percent: i16, channel: String }`
  - `enum UpgradeType { Prompt, Force }` + `#[serde(rename_all = "lowercase")]`
  - 全部 `#[derive(Serialize, Deserialize, ToSchema)]`；加 round-trip 单测锁 `upgrade_type` wire = `"prompt"` / `"force"`（api-types 已有 `serde_json` dev 可用）
- [x] 2.2 [code] `crates/swarmhive-api-types/src/lib.rs` `pub mod update;` + re-export `TauriUpdateResponse` / `TauriUpdateExtensions` / `UpgradeType`

## 3. Error 横切适配

- [x] 3.1 [code] **(F9) `error.rs` 的 `ApiErrorResponses` 加 `400` 变体**：`#[response(status=400, description="Request validation failed (e.g. malformed current_version).", content_type="application/problem+json")] BadRequest(Problem)`；并在 `ApiError` 的 `status()` / `type_uri()` / `title()` match 补对应分支（`ApiError::typed(StatusCode::BAD_REQUEST, ..)` 已支持任意 status，缺的只是 OpenAPI doc 枚举，否则 SPA codegen 漂移）

## 4. Server：updates endpoint

- [x] 4.1 [code] **(F7) `routes/apps.rs` 加 `pub(crate) async fn find_app_by_slug<C: ConnectionTrait>(db, slug) -> Result<app::Model, ApiError>`**（纯 `WHERE slug=?`，单组织下 slug 全局唯一；公开 endpoint 无 org_id）
- [x] 4.2 [code] 新建 `crates/swarmhive-server/src/routes/updates.rs`：`router()` + `tauri` handler 骨架（公开、无 `Principal`；`Query(TauriUpdateQuery)` 解析 `current_version`/`target`/`arch`/`channel?`/`client_id?`）
- [x] 4.3 [code] **(F1) `updates.rs` 私有 `find_default_channel(db, app_id) -> Option<channel::Model>`**（`WHERE app_id AND is_default=true` 取 `.one()`，**返回 Option**，不 unwrap）；handler 步骤 1-4：`find_app_by_slug`（none → 404）→ channel（指定 name 不存在 → 404；未指定且无默认 → **204**）→ `channel_release` 指针（无 → 204）→ release（非 `published` → 204）。`find_channel` 现为私有，updates.rs 独立写自己的 helper
- [x] 4.4 [code] **(F3) semver 闸门**：私有 `strip_v(s) = s.strip_prefix('v').unwrap_or(s)`（只削一个）；**两边都 strip** 再 parse。`current_version` parse 失败 → 400 typed `invalid-current-version`；`rel.version` parse 失败 → `warn!` + 204；`rel.version <= current_version` → 204
- [x] 4.5 [code] **(F7) `parse_tauri_triple(&str) -> Option<(String,String)>`**（D1 算法，`universal-apple-darwin` 的 arch 段返回 `"universal"`）+ `match_tauri_artifact`：先过滤掉无 `tauri_signature` 的 artifact（**(F15) `art.signature_metadata.as_ref().and_then(|j| j.0.get("tauri_signature")).and_then(|v| v.as_str())`**）→ 精确 `(os,arch)` → `(darwin,"universal")` 对任意 arch 放行 → 单 untargeted fallback → 否则 None（空 artifact 集合也安全返 None）
- [x] 4.6 [code] **(F4) `in_rollout_bucket(key, percent)`**（blake3 前 8 字节 LE % 100；`>=100` 短路 true、`<=0` false）；分桶 key 三级回退 `client_id` → 请求 IP（`x-forwarded-for`）→ **都无则命中 + `tracing::warn!("rollout bucketing bypassed: no client_id/ip")`**
- [x] 4.7 [code] handler 收尾：`upgrade_type = if min_version>current { Force } else { Prompt }`；构造 `TauriUpdateResponse`——`url = download::download_url(&state.config.server.base_url, slug, version, art.id)`、`signature` 同 4.5 取值、**(F9) `pub_date = rel.published_at.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))`**（产出 `...Z`）、`rollout_percent = rel.rollout_percent.unwrap_or(100)`；返回 `(OK, Json(..))` 或 `StatusCode::NO_CONTENT`
- [x] 4.8 [code] **(F5) 埋点字段对齐 telemetry**：入口 `tracing::info!(target:"telemetry", event="update_check", app_id, channel, current_version, platform="tauri-desktop", target, arch, anonymous_client_id=client_id)`；命中更新后 `event="update_available"` 附 `release_id`(Uuid)、`artifact_id`(Uuid)、`storage_backend_id`（从 `art` 取）
- [x] 4.9 [code] **(F12) `lib.rs::api_routes()` merge `routes::updates::router()`**（公开、不限流，`download` 后并列）；`routes/mod.rs` 加 `pub mod updates;`
- [x] 4.10 [code] 重生成 OpenAPI codegen：`pnpm --filter @swarm-hive/admin openapi`（dump-openapi → `/tmp` → `schema.gen.ts`）。**注**：CLAUDE.md 顶部 `> apps/admin/src/lib/api/openapi.json` 是过时路径，真实 codegen 产物是 `schema.gen.ts`

## 5. 测试

- [x] 5.1 [test] 新建 `crates/swarmhive-server/tests/update_check_tauri_smoke.rs`：复刻 `app_release_smoke` 的 `boot()`/`oneshot`/`setup_owner`/`create_app` harness；**(F8) helper 须先 insert 一行 `storage_backend`（dummy active）满足 artifact FK**，再 insert `artifact`（带 `signature_metadata = {"tauri_signature": "<multiline>"}`）+ promote channel。endpoint 只字符串拼 `download_url`，**无需真实对象存储**
- [x] 5.2 [test] 有更新：promote stable → 0.4.5（带 darwin/aarch64 签名 artifact）→ GET `?current_version=0.4.0&target=darwin&arch=aarch64` → 200 + 断言 `version`/`url` 指向 `/download/...`/`signature` 非空/`swarmhive.channel="stable"`/`upgrade_type="prompt"`；另测 `current_version=v0.4.0`（前导 v）同样 200
- [x] 5.3 [test] **204 矩阵**：无指针、release=draft、release=yanked、`current_version` 相等/更高、**无默认 channel(F1)**、**零 tauri artifact(F2)**、跨平台无匹配、匹配 artifact 无签名 —— 每条断言 `204` + 空 body
- [x] 5.4 [test] `404`（未知 app slug / `channel=nightly`）+ `400`（`current_version=not-a-version`，断言 `type`=`.../invalid-current-version`）
- [x] 5.5 [test] triple 匹配：release 同时有 `aarch64-apple-darwin` + `x86_64-pc-windows-msvc` → `(darwin,aarch64)` 命中 mac、`(windows,x86_64)` 命中 win；**(F7) `universal-apple-darwin` → `(darwin,x86_64)` 与 `(darwin,aarch64)` 都命中**；单 `target IS NULL` artifact → 任意 `(target,arch)` fallback
- [x] 5.6 [test] 强制更新：release 0.5.0 + `min_version=0.4.0` → `current_version=0.3.0` 得 `force`、`current_version=0.4.2` 得 `prompt`
- [x] 5.7 [test] **(F8) 灰度**：release `rollout_percent=50` → 用 ~300 个不同 `client_id` 统计 200 占比落在 **`[40%,60%]`** 宽松带（避免统计 flaky）+ 同一 `client_id` 两次结果一致；`rollout_percent=100`/NULL → 全量；无 client_id/IP → 命中
- [x] 5.8 [test] `PATCH release { min_version, rollout_percent }` 设置生效（后续 check 反映）；`rollout_percent=0`/`150` → 422；`min_version="bad"` → 422；**(F3) `create_release` version=`"latest"` → 422 `invalid-release-version`**
- [x] 5.9 [test] `routes/updates.rs` `#[cfg(test)]` 纯函数单测：`parse_tauri_triple`（三平台 × 两 arch + **universal(F7)** + 非法返回 None + `strip_v`）、`in_rollout_bucket`（边界 0/100 + 确定性）

## 6. docs / memory / openspec 同步

- [x] 6.1 [docs] `dev-notes/knowledge/backend.md` 加「Tauri 更新检查 endpoint」段：target triple 错配根因与解析、灰度三级回退 + **直连部署需 client_id(F4)**、204 契约决策表、signature 透传与无签名跳过、release schema 两新列的 schema-sync 取舍、**client_id↔anonymous_client_id 命名映射(F11)**
- [x] 6.2 [docs] `docs/04-platform-support.md` Tauri 段 + `docs/03-architecture.md` 更新检查流程：落地真实 endpoint + flat 响应 + 204 语义
- [x] 6.3 [docs] `CLAUDE.md` server endpoints 清单加 `/api/v1/updates/tauri/:app_slug`；涉及英文注释顺手中文化时更新「已转范围」时间戳
- [x] 6.4 [docs] `openspec/changes/README.md` 依赖图标注 `add-update-check-tauri` 已 apply
- [x] 6.5 [docs] memory：triple 映射 / 灰度直连约束若判定为非显然决策，往 `memory/` 追一条（链接 [[project-platform-scope]] / [[project-storage-model]]）
- [x] 6.6 [code] 质量门：`cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + `pnpm --filter @swarm-hive/admin typecheck`（openapi.json 变更后）全绿

## 跨 proposal 联动

- [x] 7.1 **(F11)** `grep -rn "default_channel\|darwin-x86_64\|anonymous_client_id\|client_id" openspec/changes/ docs/`：对齐 `add-update-check-rn-android`（应给其 endpoint 也加 `client_id` query，否则 update_check 埋点缺 `anonymous_client_id` 与 telemetry 列不齐）/ `add-telemetry-events`（确认 `update_event` 列名与本 change emit 一致）；把残留旧措辞（`app.default_channel`、合并 target 串）改成本 change 的实际决策
