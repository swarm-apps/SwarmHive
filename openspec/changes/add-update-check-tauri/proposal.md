# add-update-check-tauri

## Why

docs/04 / docs/02 把 Tauri 桌面更新作为 MVP 第一条主链路。SwarmDrop / SwarmNote 等产品的迁移验收依赖于此 endpoint 能稳定返回 Tauri updater 兼容 JSON。

## What

### 1. endpoint

```
GET /api/v1/updates/tauri/:app_slug
    Query: current_version, target, arch, channel?
    （channel 缺省为 app.default_channel）
```

返回 200（有更新）或 204（无更新），200 时 body 是 Tauri updater 兼容 JSON：

```json
{
  "version": "0.4.5",
  "pub_date": "2026-05-25T08:00:00Z",
  "url": "https://updates.example.com/download/swarmdrop/0.4.5/<artifact_id>",
  "signature": "<minisign-sig>",
  "notes": "...",
  "swarmhive": {
    "upgrade_type": "prompt",
    "min_version": "0.4.0",
    "rollout_percent": 100,
    "channel": "stable"
  }
}
```

`swarmhive.*` 扩展字段不破坏 Tauri updater 兼容。

### 2. 下载入口

```
GET /download/:app/:version/:artifact_id
```

按 docs/07 流程：记 `download_intent` → 检查可用 → 生成 public / signed URL → 记 `download_redirected` → 302。

### 3. target / arch 匹配

按 Tauri updater 约定：

| target string | platform | arch     |
| --- | --- | --- |
| `darwin-x86_64`   | macOS | x86_64 |
| `darwin-aarch64`  | macOS | aarch64 |
| `windows-x86_64`  | Windows | x86_64 |
| `linux-x86_64`    | Linux | x86_64 |
| …                 | … | … |

匹配优先级：精确 target → fallback 同 platform 同 arch（避免发布命名不一致时漏掉）。

### 4. 强制更新 / 灰度

- `min_version > current_version` → 客户端必须升级（响应 `upgrade_type=force`，扩展字段也明示）。
- `rollout_percent < 100` → 用 `anonymous_client_id` 做稳定分桶（hash(id) % 100 < percent）。

### 5. 埋点

写 `update_check` 与 `update_available`（属于 `add-telemetry-events` 范畴；本 proposal 只发事件，不实现聚合）。

## Acceptance

- 真实 Tauri v2 客户端配置 endpoint 后能检查到 release 并下载安装（contract test 用 cargo-tauri-updater testkit 或手测）。
- 多 target 同时存在时返回的 url 是正确平台 artifact。
- `min_version` 触发强制更新（响应 force）。
- rollout 50% 的版本，1000 次模拟客户端调用约 500 次拿到更新（±50 容差）。
- 集成测试覆盖：发 release → check 拿到 → /download/... 重定向到 S3 → 返回 302。

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
