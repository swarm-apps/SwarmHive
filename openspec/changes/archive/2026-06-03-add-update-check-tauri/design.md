# design

## Context

`add-storage-and-presign-upload` 落地后,「发布 → 存储 → 上传产物 → 下载入口」已通,但**客户端拿不到「该不该更新」的判断**——缺一个 Tauri updater 能直接配置的 endpoint。本 change 补上这条主链路的最后一公里。

开工前对现状代码的核对结论(决定了本 change 的真实增量,避免重复造轮子):

| proposal 设想 | 现状 | 本 change 动作 |
|---|---|---|
| 下载入口 `GET /download/:app/:version/:artifact_id` | **已实现**(`routes/download.rs` + `download_url()` helper,302 + yanked→404 + 路径自洽) | **复用** `download::download_url(base, slug, version, id: Uuid)`,不重写 |
| channel 缺省 = `app.default_channel` | `app` 无该字段;默认 channel 是 `channel.is_default=true` 标记(`channel.rs:21` 确认) | 新写 `find_default_channel(db, app_id) -> Option`(D6) |
| channel 当前 release | `channel_release[channel_id] → release_id` 指针(`get_channel_release` 是现成范本) | 复用查询 pattern |
| target/arch 匹配 | artifact 存的是 **Rust target triple**(`aarch64-apple-darwin`),`arch` 列恒 `None`(`publish.rs::plan_artifacts`) | server 端解析 triple → (os, arch)(D1) |
| signature | 已存在 `artifact.signature_metadata`(`Option<Json>`)的 `tauri_signature` key(`uploads/service.rs:182` 写入,`storage_smoke.rs:856` 验证) | 原样透传,取值走 `Json.0`(D5) |
| `min_version` / `rollout_percent` | release entity **没有**这两列 | 扩展 `release` schema(D3) |
| 埋点 `update_check` / `update_available` | 无持久层(telemetry proposal 范畴) | 只发结构化 `tracing` 事件,字段对齐 telemetry 列(D8) |

## Goals / Non-Goals

**Goals:**

- 公开 endpoint `GET /api/v1/updates/tauri/:app_slug`,返回 Tauri v2 updater **dynamic server** 兼容响应
- 严格遵守 Tauri 协议:有更新 → `200` + flat JSON;无更新 → `204 No Content` 空 body
- target/arch → artifact 精确匹配 + universal-apple-darwin + 单平台 fallback
- `min_version` 强制更新 + `rollout_percent` 灰度分桶
- 发结构化埋点事件占位,字段名与 `add-telemetry-events` 的 `update_event` 列**逐一对齐**(D8)

**Non-Goals:**

- 不实现 `latest.json` 静态文件输出(动态 endpoint 已覆盖)
- 不实现 server 端 minisign 验签(Tauri updater 自身完成)
- 不实现 delta / partial update(Tauri 协议不支持)
- 不改 CLI 上传时的 target/arch 存储格式(纯 server 端解析,不动已上传数据)
- 不做 telemetry 持久化 / 聚合 / Admin 展示(telemetry proposal 负责)
- 不支持把 `min_version` / `rollout_percent` PATCH 回 `NULL`(用边界值替代清空,见 D3)
- 不给 server 补 `ConnectInfo<SocketAddr>` 兜底 IP(直连部署的灰度约束见 D2 / R3,作为已知限制)
- 不实现 RN Android endpoint(`add-update-check-rn-android` 负责;`routes/updates.rs` 为它预留同文件落点)

## 数据流

```text
  Tauri app (plugin-updater, dynamic endpoint)
  endpoints = ["https://hive.example.com/api/v1/updates/tauri/swarmdrop
                ?current_version={{current_version}}&target={{target}}&arch={{arch}}
                &channel=stable&client_id=<sdk-local-uuid>"]
        │
        │  GET  current_version=1.2.3  target=darwin  arch=aarch64  channel=stable  client_id=…
        ▼
  ┌──────────────────────────────────────────────────────────────────────┐
  │ routes::updates::tauri  (公开 · 不限流 · 无 Principal)                  │
  │                                                                        │
  │  1. app   ← find_app_by_slug(:app_slug)      ── none ─────────► 404    │
  │  2. chan  ← ?channel 指定 name,否则 find_default_channel               │
  │           ── 指定 channel 不存在 ───────────────────────────► 404      │
  │           ── 未指定且无 is_default channel ─────────────────► 204      │
  │  3. ptr   ← channel_release[chan.id]         ── 无指针 ───────► 204    │
  │  4. rel   ← release[ptr.release_id]                                     │
  │           ── status != published (draft/yanked) ────────────► 204      │
  │  5. semver: rel.version > current_version ?  ── 否 ──────────► 204      │
  │           ── current_version parse 失败 ────────────────────► 400      │
  │           ── rel.version parse 失败 ── warn! ───────────────► 204      │
  │  6. art   ← match_tauri_artifact(rel, target, arch)   (D1)             │
  │           ── 无 tauri artifact / 无匹配 / 匹配项无 signature ► 204      │
  │  7. rollout: in_bucket(client_id|ip, rel.rollout) ?  ── 否 ──► 204      │
  │  8. upgrade_type = if min_version > current_version { force }           │
  │                    else { prompt }                       (D4)          │
  │  9. tracing: update_check + update_available             (D8)          │
  │ 10. body = TauriUpdateResponse {                                        │
  │        version, pub_date(=published_at, RFC3339 Z), notes(=notes),      │
  │        url = download_url(&cfg.server.base_url, slug, version, art.id), │
  │        signature = art.signature_metadata.0["tauri_signature"], (D5)    │
  │        swarmhive { upgrade_type, min_version, rollout_percent, channel }│
  │     }                                                                   │
  └──────────────────────────────────────────────────────────────────────┘
        │
        ▼  200 + JSON (有更新)   │   204 No Content (无更新,空 body)
  Tauri updater: 内置 semver 复核 → 下载 url → minisign 验 signature → 安装
```

## Decisions

### D1. Tauri target triple ↔ (os, arch) 映射(本 change 的核心难点)

**冲突**:Tauri updater 把 `{{target}}` 替换成**纯 OS 名** `darwin`/`windows`/`linux`、`{{arch}}` 替换成 `x86_64`/`aarch64`/`i686`/`armv7`(官方文档确认);而 CLI 上传时 `artifact.target` 存的是 **Rust target triple**(`aarch64-apple-darwin`),`artifact.arch` 恒为 `None`。两者无法直接 join。

**决策**:**server 端解析 triple**,不改 CLI、不动已上传数据。

```rust
/// Rust target triple `<arch>-<vendor>-<sys>[-<env>]` → (os, arch),
/// 对齐 Tauri updater 注入的 {{target}}/{{arch}} 取值。
/// 特例:universal-apple-darwin 的 arch 段返回 "universal"(非真实 arch),
/// 由 match_tauri_artifact 据此对 darwin 的任意 arch 放行。
fn parse_tauri_triple(triple: &str) -> Option<(String, String)> {
    let arch = triple.split('-').next()?.to_string(); // aarch64 / x86_64 / i686 / armv7 / universal
    let os = if triple.contains("darwin") || triple.contains("apple") {
        "darwin"
    } else if triple.contains("windows") {
        "windows"
    } else if triple.contains("linux") {
        "linux"
    } else {
        return None;
    };
    Some((os.to_string(), arch))
}
```

**匹配优先级**(`match_tauri_artifact`,只在带 `tauri_signature` 的 `tauri-desktop` artifact 中选):

1. **精确**:`parse_tauri_triple(art.target) == (q_target, q_arch)` → 选它。
2. **universal(仅 macOS)**:`art.target` 解析出 `(darwin, "universal")`(即 `universal-apple-darwin`,Tauri 官方支持的 macOS universal binary)→ 对 `q_target=darwin` 的**任意** `q_arch` 放行。
3. **单平台 fallback**:无以上命中,但该 release 恰好**只有一个** `tauri-desktop` artifact 且其 `target IS NULL`(没传 `--target` 的单平台场景)→ 选它。
4. 都不满足(含 release 无任何 `tauri-desktop` artifact)→ `204`。

> fallback 只在「唯一且无 target」时触发,多个 untargeted artifact 绝不瞎选(避免把 macOS 包发给 Windows)。**无签名 artifact 在匹配前先被过滤掉**(D5)——未签名包返回了客户端也会验签失败。

### D2. 灰度分桶 + client_id 来源

proposal 第 4 节要 `hash(anonymous_client_id) % 100 < percent`,但**原 query 没有 client_id 参数**。

**决策**:endpoint 加**可选** query `client_id`。Tauri SDK(后续 `sdk-core`)本地生成一次 uuid v4 存 `$APP_DATA/swarmhive_client_id`,每次 check 携带。

```rust
fn in_rollout_bucket(key: &[u8], percent: i16) -> bool {
    if percent >= 100 { return true; }      // 全量短路
    if percent <= 0   { return false; }     // 防御:不应出现(PATCH 已拒 0)
    let h = blake3::hash(key);               // blake3 已在 workspace
    let n = u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap());
    (n % 100) < percent as u64
}
```

**分桶 key 三级回退**:`client_id`(query) → 否则请求 IP(`x-forwarded-for`,已有 `RequestCtx::from_headers`) → **都没有则视作命中(放行)但发 `tracing::warn!("rollout bucketing bypassed: no client_id/ip")`**。

> **直连部署灰度约束(F4)**:SwarmHive 主打的 bundled 单机形态通常**无反代注入 `x-forwarded-for`**,`RequestCtx` 取不到 IP(代码注释:`direct deployment will get None until ConnectInfo wiring lands`)。此时若 SDK 不传 `client_id`,三级回退直落「视作命中」,`rollout_percent=50` 实际变 100%。这是有意的「渐进放量优先于精确管控」取舍,但**必须可观测**(故 warn)。要让直连部署灰度真正生效,**SDK 必须传 client_id**——这条写进 spec 作为部署约束,不再当成隐性哲学。给 server 补 `ConnectInfo` 兜底 IP 是更大改动,留 R3。

**命名映射拍板(F11)**:wire 层(query / 响应)字段叫 **`client_id`**;telemetry 落库列叫 **`anonymous_client_id`**——二者是同一匿名标识的 wire / storage 两端。`add-update-check-rn-android` 的 endpoint 也应加 `client_id` query,否则其 `update_check` 埋点缺 `anonymous_client_id`、与 telemetry 列不齐(写进 tasks 6.1)。

### D3. release schema 扩展 + 写入入口 + 清空语义

`release` entity 加两列:

```rust
pub min_version: Option<String>,     // 强制更新下限(semver);NULL = 无下限
pub rollout_percent: Option<i16>,    // 1-100 灰度;NULL = 视作 100 全量(代码层 .unwrap_or(100))
```

**为什么都用 `Option` 而非 `NOT NULL DEFAULT 100`**:项目走 schema-sync(无 migration crate)。`NOT NULL DEFAULT` 加列对已有行的回填在 sea-orm 2 rc.38 的 schema-sync 下不可靠。`Option<i16>` 加列必定 sync 成功(老行自动 NULL),语义用代码 `rollout.unwrap_or(100)` 兜底。**不要**为这两列加 partial unique index。

**写入入口**:扩 `UpdateReleaseRequest`(api-types `release.rs`)加 `min_version` / `rollout_percent`,`PATCH /api/v1/apps/:slug/releases/:version` 设置。`api::Release` DTO 同步加两字段。

- 校验:`PATCH` 时 `rollout_percent ∈ Some(1..=100)`(`Some(0)` / 越界 → 422 typed `invalid-rollout-percent`);`min_version` 非空时必须 `semver::Version::parse` 通过(否则 422 typed `invalid-min-version`)。
- **清空语义(F6)**:沿用现有 `update_release` 的 `Some → Set / None → 不动` 范式(单层 `Option`,`null` 与缺省都解析成 `None` = 不改)。因此**本 change 不提供「PATCH 回 NULL」**。误设的纠正走**边界值替代**:`min_version` 误设 → 设为 `"0.0.0"`(≤ 任何 current → 永远 `prompt`,等效无下限);`rollout_percent` 误设 → 设为 `100`(全量)。这避免引入 double-Option 的 serde/utoipa 复杂度;边界值能覆盖全部纠错需求。spec 显式声明该限制。

### D4. semver 比较

引入 `semver = "1"` 到 `[workspace.dependencies]`(server 用)。

- **版本归一**:比较前对**两边**都 `s.strip_prefix('v').unwrap_or(s)`(只削**一个**前导 `v`,不用 `trim_start_matches('v')`——后者会把 `vvv1.2.3` 误削)。SwarmHive 内部统一存无 `v`,但 query / 历史 release 都可能带,两边同口径才正确(F3)。
- `rel.version > current_version`(`semver::Version::parse` 后比较)。
- **解析失败**:`current_version` 失败 → `400` typed `invalid-current-version`;`rel.version` 失败 → `tracing::warn!` + `204`(保守不分发坏版本)。
- **源头校验(F3)**:`routes/releases.rs::create_release` 入口加 `semver::Version::parse(strip_prefix('v'))` 校验,非法 → 422 typed `invalid-release-version`。从创建处杜绝坏 version,而非只在分发时静默吞掉。(现有 `app_release_smoke` 用的 `0.4.5`/`1.0.0` 均合法,不受影响。)
- `min_version`(若非 NULL,归一后)`> current_version` → `upgrade_type = force`,否则 `prompt`。

> server 自做 semver 比较是 **SHOULD**(updater 默认仍有内置版本检查兜底)。收益:无更高版本直接 `204` 省一次下载,且对回滚客户端行为确定。

### D5. signature 透传 + 无签名处理

`signature` 字段 = `.sig` 文件**完整原文**(多行),Tauri 官方明确「不是 url/路径,是文件内容字符串」。已在 `artifact.signature_metadata = {"tauri_signature": <sig 全文>}` 存好。

- 取值(sea-orm `Json` 是 `serde_json::Value` 的 newtype wrapper,**经 `.0` 解包**):
  `art.signature_metadata.as_ref().and_then(|j| j.0.get("tauri_signature")).and_then(|v| v.as_str())`。
- **无 signature 的 tauri artifact**:返回缺签名的 update 会让客户端**验签失败**。决策:匹配阶段就跳过无 `tauri_signature` 的 artifact → 可能整体落 `204` + `tracing::warn!`。
- **可观测性已知限制(F10)**:发布者忘传 `.sig` 时客户端只见 `204`(无更新),server warn 日志发布者通常看不到 → 静默失败。**缓解前移**留 follow-up(CLI publish 上传 `tauri-desktop` 产物时检测缺 `.sig` 并 warn / Admin 详情页标记「无签名不可分发」)。本 change 范围内记为 known limitation,不动 CLI。

### D6. 公开 endpoint 如何定位 app + 默认 channel

- update endpoint 无 `Principal`,拿不到 `org_id`(现有 `find_app` 需要 `org_id`)。单组织 MVP 下 `app.slug` 在 `(org_id, slug)` 复合唯一约束下等价于全局唯一。新写 `pub(crate) find_app_by_slug(db, slug)`(纯 `WHERE slug=?`),放 `routes/apps.rs`。
- `find_default_channel(db, app_id)`:`WHERE app_id AND is_default=true` 取 `.one()` → **返回 `Option<channel::Model>`**。`make_sole_default` 只保证「设新默认时旧默认清掉」,**不**保证「恒有一个默认」(运维可把 stable 的 `is_default` PATCH 成 false,或删默认 channel)。故 handler 必须显式处理 `None` → `204`(语义:该 app 无可服务的默认 channel),**不 unwrap**(F1)。
- 注:`routes/releases.rs::find_channel` 当前是**私有** `async fn`(F6-integration),`updates.rs` 不能直接调;`find_default_channel` 在 `updates.rs` 内独立写(语义也更内聚)。

### D7. 模块落点

- 新建 `crates/swarmhive-server/src/routes/updates.rs`(vertical slice,单文件)。当前仅 `tauri` 一个 endpoint + 私有 helper(`parse_tauri_triple` / `match_tauri_artifact` / `in_rollout_bucket` / `find_default_channel`),预计 < 250 LOC,**不拆 service**。`add-update-check-rn-android` 后续在**同文件**加 `android` handler。
- `lib.rs::api_routes()` merge `routes::updates::router()`——公开、**不限流**(跟 `download`/`version` 并列,不进 `sensitive_routes`)。
- api-types 新建 `update.rs`:`TauriUpdateResponse { version, pub_date?, url, signature, notes?, swarmhive }` + `TauriUpdateExtensions { upgrade_type, min_version?, rollout_percent, channel }`。`upgrade_type` 用 `#[serde(rename_all="lowercase")]` enum `{ Prompt, Force }`。

### D8. 埋点占位(字段对齐 telemetry 列,避免返工)

与 `download.rs::download_intent` 同范式,发结构化 `tracing` 事件;字段名**逐一对齐** `add-telemetry-events` 的 `update_event` 列集,telemetry proposal 落库时零改 emit 点(F5):

- 入口 `update_check`:`event`、`app_id`、`channel`、`current_version`、`platform="tauri-desktop"`、`target`、`arch`、`anonymous_client_id`(= 入参 `client_id`,见 D2 命名映射)。
- 命中更新(返回 200 前)`update_available`:附 `release_id`(Uuid)、`artifact_id`(Uuid)、`storage_backend_id`(从 `art` 取)。

> `target="tauri"`(纯 OS) 维度归入 `update_event.target` 列;`abi` 列对 Tauri 恒空(RN 才用)。emission 不阻塞、不影响响应。

### D9. 200 vs 204 / 404 / 400 决策表(契约单一来源)

| 条件 | 响应 |
|---|---|
| app slug 不存在 | `404` |
| 指定的 `channel` 不存在 | `404` |
| `current_version` 非合法 semver | `400` typed `invalid-current-version` |
| 未指定 channel 且 app 无 `is_default` channel | `204` |
| channel 无指针(从未 promote) | `204` |
| 指向的 release 非 `published` | `204` |
| `rel.version` 解析失败 | `204` (+ warn) |
| `rel.version <= current_version` | `204` |
| release 无任何 `tauri-desktop` artifact / 无 (os,arch) 匹配 | `204` |
| 匹配到的 artifact 无 `tauri_signature` | `204` (+ warn) |
| 不在 rollout 灰度桶 | `204` |
| **以上全通过** | `200` + flat JSON |

### D10. 现有横切设施的两处适配(integration review)

- **`ApiErrorResponses` 加 `400` 变体(F9)**:`error.rs` 的 `ApiErrorResponses`(喂 utoipa OpenAPI doc)当前只枚举 401/403/404/409/410/422/500,**无 400**。本 change 的 `invalid-current-version` 走 400,需补一个 `#[response(status=400, ...)] BadRequest(Problem)` 变体,并在 `status()`/`type_uri()`/`title()` 的 match 补对应分支,否则 OpenAPI doc 缺 400 → SPA codegen 与实际 router 漂移。`ApiError::typed(StatusCode::BAD_REQUEST, ...)` 构造器已支持任意 status(`error.rs:78`)。
- **测试 artifact 的 FK 前置(F8)**:`artifact.storage_backend_id` 是 `belongs_to storage_backend` 的 `NOT NULL Uuid`;schema-sync 建 FK 约束。集成测试直接 insert artifact 前**必须先 insert 一行 `storage_backend`**(test helper 造一个 dummy active backend)。endpoint 本身只字符串拼 `download_url`,**不碰** storage handle,故无需真实对象存储。

## Risks / Open questions

- **R1 triple 解析覆盖面**:`parse_tauri_triple` 覆盖 darwin/windows/linux + universal-apple-darwin。主流 triple(`aarch64-apple-darwin`/`x86_64-pc-windows-msvc`/`x86_64-unknown-linux-gnu`/`universal-apple-darwin`)均可解析;冷门 triple 返回 `None` → 不匹配。triple 单测覆盖三平台 × 两 arch + universal + 非法。
- **R2 rollout_percent NOT NULL 取舍**:选 `Option<i16>` 牺牲 DB 层强约束,改由 PATCH handler(`1..=100`)+ 读取 `.unwrap_or(100)` 双兜底。切到真正 migration crate 后可收紧为 `NOT NULL DEFAULT 100 CHECK (1<=x<=100)`。
- **R3 直连部署 IP 缺失(F4)**:bundled 单机直连无 `x-forwarded-for` → 灰度仅靠 client_id 生效,无 client_id 则全量 + warn。补 `ConnectInfo<SocketAddr>` 兜底 IP 是更彻底的解,留后续(需配合 `into_make_service_with_connect_info`)。
- **R4 与 telemetry 的接口**:D8 emit 字段已按 `update_event` 列对齐(`anonymous_client_id`/`platform`/`release_id`/`storage_backend_id`),telemetry proposal 落库时应零改 emit 点。
- **R5 无签名静默失败可观测性(F10)**:本 change 仅 server warn;发布者侧前移检测(CLI/Admin)留 follow-up。
