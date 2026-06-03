# add-update-check-tauri

## Why

docs/04 / docs/02 把 Tauri 桌面更新作为 MVP 第一条主链路。SwarmDrop / SwarmNote 等产品的迁移验收依赖于此 endpoint 能稳定返回 Tauri updater 兼容 JSON。

## What

### 1. endpoint

```
GET /api/v1/updates/tauri/:app_slug
    Query: current_version (semver), target (OS 名: darwin/windows/linux),
           arch (x86_64/aarch64/i686/armv7), channel?, client_id?
    （channel 缺省为 app 的 is_default channel；client_id 用于灰度稳定分桶）
```

> Tauri updater dynamic endpoint 注入的是**分离**的 `{{target}}`(纯 OS 名)+ `{{arch}}`,**不是** `darwin-aarch64` 合并串。endpoint URL 配成
> `.../updates/tauri/swarmdrop?current_version={{current_version}}&target={{target}}&arch={{arch}}&channel=stable&client_id=<sdk-uuid>`。

返回 **200**（有更新,flat JSON）或 **204 No Content**（无更新,空 body——不要返回带空 version 的 JSON）。200 body 是 Tauri v2 updater **dynamic server** 兼容的 flat 形态(顶层直接 url+signature,**不是** static JSON 文件的 platforms map):

```json
{
  "version": "0.4.5",                     // 必填,合法 semver
  "pub_date": "2026-05-25T08:00:00Z",     // 可选,RFC 3339
  "url": ".../download/swarmdrop/0.4.5/<artifact_id>",  // 必填,直链(复用现有下载入口)
  "signature": "<.sig 文件完整原文>",     // 必填,minisign .sig 全文(多行),非路径/URL
  "notes": "...",                          // 可选
  "swarmhive": {                           // best-effort 扩展命名空间(非 Tauri 官方契约)
    "upgrade_type": "prompt",              // prompt | force
    "min_version": "0.4.0",
    "rollout_percent": 100,
    "channel": "stable"
  }
}
```

`version`/`url`/`signature` 必填,`pub_date`/`notes` 可选。Tauri updater 用 serde 忽略未知字段,`swarmhive.*` 扩展放独立命名空间既不破坏兼容、又避免与未来 Tauri 标准字段撞名。详见 [design.md](design.md) D5 / D9。

### 2. 下载入口（已实现，本 change 复用）

```
GET /download/:app/:version/:artifact_id
```

**此 endpoint 已由 `add-storage-and-presign-upload` 实现**（`routes/download.rs`：记 `download_intent` → 按 backend `url_mode` 生成 public / signed URL → 302；yanked release → 404）。本 change **不重写**，只复用 `download::download_url(base_url, slug, version, artifact_id)` helper 拼出响应里的 `url` 字段。

### 3. target / arch 匹配（server 端解析 Rust target triple）

**关键事实**：CLI 上传 Tauri 产物时 `artifact.target` 存的是 **Rust target triple**（`aarch64-apple-darwin`、`x86_64-pc-windows-msvc`），`artifact.arch` 恒为 `None`（见 `publish.rs::plan_artifacts`）。而 updater 注入的是**分离**的 OS 名 + arch。两者无法直接比对，故 **server 端把 triple 解析成 `(os, arch)`** 再匹配（不改 CLI、不动已上传数据）：

| updater 请求 `(target, arch)` | 解析自 artifact triple | 命中 |
| --- | --- | --- |
| `(darwin, aarch64)` | `aarch64-apple-darwin` | ✓ |
| `(darwin, x86_64)`  | `x86_64-apple-darwin`  | ✓ |
| `(windows, x86_64)` | `x86_64-pc-windows-msvc` | ✓ |
| `(linux, x86_64)`   | `x86_64-unknown-linux-gnu` | ✓ |

匹配优先级：**① 精确**（解析后 `(os, arch)` 相等）→ **② 单平台 fallback**（release 恰好只有一个 `tauri-desktop` artifact 且 `target IS NULL`，即用户没传 `--target` 的常见单平台场景）→ 否则 **204**。算法见 [design.md](design.md) D1。

### 4. 强制更新 / 灰度（依赖 release 新增 schema 字段）

`release` entity 新增两列（原 proposal 隐含、未明说——本次补齐）：

- `min_version: Option<String>`（semver 强制更新下限；NULL = 无下限）
- `rollout_percent: Option<i16>`（1–100 灰度；NULL = 视作 100 全量）

两列都用 `Option` 走 schema-sync 安全路径（避免 `NOT NULL DEFAULT` 在 sea-orm 2 rc.38 schema-sync 的回填坑），语义靠代码 `.unwrap_or(100)` 兜底。设置入口：扩 `UpdateReleaseRequest` 经 `PATCH /api/v1/apps/:slug/releases/:version`；`api::Release` DTO 同步加两字段。详见 [design.md](design.md) D3。

行为：

- `min_version > current_version`（semver）→ `swarmhive.upgrade_type=force`，否则 `prompt`。
- `rollout_percent < 100` → 用 query 的 `client_id` 做稳定分桶（`blake3(client_id) % 100 < percent`）。`client_id` 缺失时回退到请求 IP；IP 也无则视作命中**并发 `tracing::warn!`**（rollout 是渐进放量、非访问控制，不把无标识客户端永久挡住）。`rollout_percent` 为 100/NULL 时整段短路。
- **部署约束**：bundled 单机直连部署通常无反代注入 `x-forwarded-for`，IP 取不到 → 若 SDK 不传 `client_id`，灰度会整段旁路（50% 实际变 100%）。要让直连部署灰度生效，**SDK 必须传 `client_id`**。
- **命名映射**：wire 层 query/响应叫 `client_id`，telemetry 落库列叫 `anonymous_client_id`（同一匿名标识两端）。详见 [design.md](design.md) D2。
- server 自做 semver 比较决定 200 vs 204 是 **SHOULD**（Tauri updater 默认仍有内置版本检查兜底），收益是省一次下载并对回滚客户端行为确定。

### 5. 埋点

写 `update_check` 与 `update_available`（属于 `add-telemetry-events` 范畴；本 proposal 只发事件，不实现聚合）。

## Acceptance

- 真实 Tauri v2 客户端配置 endpoint 后能检查到 release 并下载安装（contract test 用 cargo-tauri-updater testkit 或手测）。
- 有更新 → `200` flat JSON；无更新（无指针 / draft|yanked / 版本不更高 / 无匹配 artifact / 不在灰度桶）→ `204 No Content` 空 body。
- 多 target 同时存在时，`(darwin, aarch64)` 请求解析 `aarch64-apple-darwin` triple，返回正确平台 artifact 的 url。
- `min_version > current_version` 触发强制更新（`swarmhive.upgrade_type=force`）。
- rollout 50% 的版本，1000 个不同 `client_id` 调用约 500 次拿到更新（±50 容差），同一 `client_id` 重复调用结果确定。
- `PATCH release { min_version, rollout_percent }` 设置生效；`rollout_percent` 越界（0 / 150）→ `422`；NULL rollout 的老 release 按全量分发。
- 匹配到的 artifact 缺 `tauri_signature` → `204`（不返回会验签失败的 update）。
- 集成测试覆盖：发 release + 带签名 artifact → check 拿到 200 → `/download/...` 302 到对象存储。

## Non-goals

- 不实现 latest.json 静态文件输出（动态 endpoint 已覆盖）。
- 不实现 minisign 客户端校验（仍由 Tauri updater 自身完成）。
- 不实现 partial / delta update（Tauri 当前协议不支持）。

## Depends on

- `add-storage-and-presign-upload`

## Maps to docs

- [docs/04-platform-support.md](../../../docs/04-platform-support.md) Tauri 段。
- [docs/03-architecture.md](../../../docs/03-architecture.md) 更新检查流程 / Tauri。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 6。
