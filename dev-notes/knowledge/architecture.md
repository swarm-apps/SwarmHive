# 架构

## 概览

SwarmHive 的顶层设计决策——crate 边界、存储抽象、部署形态、上传链路、SDK / registry 分发等"涉及多文件多 crate"的硬约束。改动这些前必须先看这里。

## Crate 边界

### 4 crate 拓扑（硬约束）

```text
swarmhive-api-types  serde DTO + utoipa::ToSchema     CLI + server 共用
swarmhive-entity     sea-orm Entity + From<api-types>  仅 server 系依赖
swarmhive-server     lib + bin: 业务/storage/auth/    lib 可被集成测试 import
                     mail/routes/SPA embed
swarmhive-cli        clap + reqwest + indicatif      不依赖 entity / sea-orm
```

**不准依赖**（CI 应有回归测试）：

- `api-types` 不依赖 sea-orm / axum / tokio / reqwest（薄共享层）
- `entity` 不依赖 axum / tokio
- `cli` 不依赖 entity / sea-orm（**关键**：`cargo tree -p swarmhive-cli | grep sea-orm` 必须无输出）
- `api-types` 不反向依赖 entity（避免环）

**Why**：CLI 与 server 业务零重叠；唯一真正共享的是 HTTP DTO，由 api-types 承担。引入 core 类的"业务容器"会拖累 CLI 编译时间。详见 `openspec/changes/add-crate-restructure/`。

**相关文件**：`Cargo.toml`、各 crate `Cargo.toml`、`docs/03-architecture.md` "Rust crate 边界（硬约束）" 段。

### server lib + bin 两 target

`swarmhive-server` 同时声明 `[lib]`（`swarmhive_server::*`）和 `[[bin]]`（`src/bin/server.rs`）。集成测试用 `use swarmhive_server::build_router;`。

**正确做法**：
- 新增业务模块都加到 `src/lib.rs` 的 `pub mod` 列表
- bin 保持瘦身：tokio main + tracing-subscriber + 加载 config + `build_router(state)` + serve
- 业务逻辑、router 装配、middleware 装配都在 lib 里

**相关文件**：`crates/swarmhive-server/src/lib.rs`、`crates/swarmhive-server/src/bin/server.rs`。

### api-types ↔ entity 转换的归属

`impl From<&entity::Model> for api_types::User` 这种转换**写在 entity crate 里**，不在 server 也不在 api-types。

**Why**：转换跟着 entity 字段走，字段改动一处搞定；api-types 不能反向依赖 entity（环）。

**相关文件**：`crates/swarmhive-entity/src/*/mod.rs`。

## 存储抽象

### S3-compatible 是唯一正式存储后端

不提供 local filesystem backend。单服务器场景通过 bundled RustFS（Docker Compose profile 或 `swarmhive storage init rustfs`）解决，RustFS 仍以 S3 API 接入。

**Why**：单一抽象让 RustFS → OSS / R2 / S3 迁移只需改 config；引入 local FS backend 会破坏这个保证，并把 server 拖累成文件服务器。

**正确做法**：
- 所有 storage 操作走 `swarmhive-server::storage` 的 trait（S3 客户端用 `aws-sdk-s3`）
- 上传中转目录只用于临时缓存，不作为产物最终存储

**相关文件**：`crates/swarmhive-server/src/storage/mod.rs`、`docs/07-storage-and-delivery.md`。

### 对象路径规范

```text
apps/{app_slug}/channels/{channel}/versions/{version}/{platform}/{arch}/{filename}
```

例：`apps/swarmdrop/channels/stable/versions/0.4.5/tauri/windows-x86_64/SwarmDrop_0.4.5_x64-setup.exe`

**相关文件**：`docs/07-storage-and-delivery.md` 末段。

## CLI 上传链路（presign 直传 + complete 回调）

CLI 不走 server 中转。流程：

1. `POST /api/v1/apps/:slug/releases/:ver/uploads/presign` —— server 校验权限、生成 per-file presigned PUT URL。
2. CLI `PUT <signed-url>` 直传 S3 / RustFS / OSS（带进度条）。
3. `POST /api/v1/apps/:slug/releases/:ver/uploads/:upload_id/complete` —— server HEAD 对象校验 size/etag、写 release / artifact、返回 endpoints。

**正确做法**：
- complete 接口幂等：同 `upload_id` 重复 complete 返回相同 release_id（用 Postgres `ON CONFLICT`）
- presign URL 5–10 min 过期
- 失败重试持有 `upload_id` + `parts[]`，可重发单个 part

**不要做**：
- 不要在 server 端 stream 转发字节（CLI publish 大文件会拖死单 binary）
- 不要二次下载校验 hash（信任 client sha256 报告 + 写 audit 即可）

**相关文件**：`docs/12-cli.md` "上传形态" 段、`docs/06-cicd.md` publish 段。

## 部署形态

### Server + Admin 单 binary

Admin SPA 构建产物通过 `rust-embed` 嵌入 server binary。Axum 负责 SPA fallback（除 `/api/*` 和 `/r/*` 外都回 index.html）。

**Dev 与 prod 不同**：
- Dev：Vite :5173 代理 `/api` 和 `/healthz` 到 Rust :3030
- Prod：单 binary，Admin SPA 已嵌入

**正确做法**：所有 API 路径放 `/api/...` 下，registry JSON 放 `/r/...`，让 SPA fallback 不会误匹配。

**相关文件**：`apps/admin/vite.config.ts`、`crates/swarmhive-server/src/lib.rs`。

### Single-server bundled

`docker compose --profile bundled-storage up -d` 同机起：server（嵌入 admin SPA）+ **Postgres** + RustFS + nginx/caddy。Postgres 保存所有结构化数据。

**Why**：用户决定一刀切 Postgres only，不保留 SQLite 路径，避免 SQL 方言双轨维护成本（详见 [backend.md](backend.md) Postgres only 条）。

**相关文件**：`docs/03-architecture.md` "部署方式" 段（compose profile 待 add-storage-and-presign-upload 落地）。

## 平台主线

**只覆盖 Tauri 桌面 + React Native Android**。iOS / Electron / Flutter / Web 热更新**明确不做**。OTA（Expo Updates / CodePush-compatible）是 provider 扩展层，MVP 不实现，只在 ProviderConfig 留扩展点。

**不要做**：
- 不要把 OTA-specific 假设（runtime_version、bundle、diff package）烤进 core types
- 不要因为某个用户问"能不能加 iOS"就开始改架构

**相关文件**：`docs/04-platform-support.md`、`docs/11-ota-providers.md`。

## SDK / Registry 分发（前端 npm 侧）

SwarmHive 自己**不发 UI**。客户端 UI 通过两条独立轨：

- **SDK 包**（npm）：`@swarmhive/sdk-core` + `/react` 子入口、`@swarmhive/tauri`、`@swarmhive/react-native`。零 UI 依赖，只暴露状态机 + hooks。
- **shadcn registry**：`packages/registry-web`（Tailwind v4 + Radix）、`packages/registry-rn`（NativeWind 4 + @rn-primitives）。UI 组件**源码**通过 `pnpm dlx shadcn@latest add` 拉进用户项目。

**正确做法**：SDK 包永远不 import 任何 UI/样式库（Tailwind / Radix / NativeWind）。文案 prop 注入，SDK 不依赖 i18n 框架。

**不要做**：
- 不要把 UI 组件塞进 SDK npm 包（会让用户项目主题冲突）
- 不要让 SDK 引入 i18n 框架（让用户自己注 react-i18next / Lingui）

**相关文件**：`docs/14-sdk-ui.md`、`packages/*`（待实现，目录预留）。

## 单组织 + 完整 RBAC

MVP **不**做多租户。所有核心表预留 `org_id`，但只有默认 Organization。5 角色（Owner / Admin / Release Manager / Developer / Viewer），权限按 verb-scoped permission 颗粒度（`release:publish`、`storage:manage` 等）鉴权。

**相关文件**：`docs/13-rbac.md`。
