# add-update-sdk-core

## Why

4 个真实 consumer（SwarmDrop / SwarmNote × 桌面 / RN）现在各自散装实现更新逻辑、**都连第三方 ToolSetLink**（`api.upgrade.toolsetlink.com`，SwarmNote 桌面的 key 甚至还是 `TODO_REPLACE`）。剖析四份实现发现:它们的更新状态机**自然收敛成同一套 8 态**——这验证了 docs/14 的 SDK 抽象是真实需求,不是纸上设计。SwarmHive 要把这套散装实现抽象成统一 SDK,并让客户端从 ToolSetLink 迁到自托管的 SwarmHive endpoint（Tauri endpoint 已由 `add-update-check-tauri` 落地）。

**架构修订（docs/14 从 4 个 npm 包改为 1 npm + 2 registry）**:`@swarm-hive/sdk` 是唯一 npm 包,纯 headless、**零平台依赖**,采用 ports & adapters。平台 adapter（Tauri/RN）+ 绑定它的 hook + UI 组件全部进 shadcn registry（后续 change），不进 npm 包。动机:① 平台适配代码因 Tauri/Expo 版本、权限、native 配置差异**本就需要用户能改源码** → registry 源码分发比锁进 npm 合适;② npm 包零平台依赖,彻底消除 optional peer deps 复杂度;③ 业界印证——7 个主流更新方案 6 个 headless,shadcn registry 分发先例充足,"逻辑留 npm 走 semver、UI/适配留 registry"正好对冲 copy-paste 升级分叉。

本 change 只做 `packages/sdk`（headless 地基,独立单测可验收）;registry / 平台代码 / 真实 app 迁移是后续 change。

## What

### 1. scaffold `packages/sdk`（`@swarm-hive/sdk`）

进 pnpm workspace（`packages/*` 已配），库构建输出 ESM + `.d.ts`,`exports` map:`"."`（core）与 `"./react"`（纯 React 订阅层）。零运行时平台依赖。

### 2. ports —— `UpdateAdapter` 接口（npm↔registry 的唯一契约）

```ts
interface UpdateAdapter {
  check(ctx): Promise<ReleaseInfo | null>;            // 打 SwarmHive endpoint,归一化
  download(release, onProgress): Promise<DownloadHandle>;
  install(handle): Promise<void>;
  storage: KeyValueStorage;                            // client_id / dismiss-TTL 持久化
  compare(current, candidate): boolean;               // semver(Tauri) / versionCode(RN)
}
```

+ 类型:`ReleaseInfo` / `UpdateStatus`（8 态）/ `UpgradeType`（prompt|force|silent）/ `Progress` / `UpdateError`。

### 3. engine —— `createUpdateEngine(adapter, opts)`

framework-agnostic 8 态状态机（idle/checking/up-to-date/available/force-required/downloading/ready/error）+ dismiss-TTL + 重试 + 回前台重检钩子（吸收 SwarmDrop-RN 最成熟的 backgrounded / dismiss / AppState 联动设计）。

### 4. 纯算法（与 server 同语义,可同输入同输出测试）

- 版本比较:semver（Tauri）+ versionCode 整数（RN），可插拔 comparator 统一"是否有更新"。
- 灰度分桶:`blake3(client_id)` 前 8 字节 LE `% 100 < percent`,**必须逐位对齐 server 的 `in_rollout_bucket`**。
- `client_id` 生成 + 通过 `storage` 持久化。

### 5. `checkUpdate` —— Tauri check

打 `GET /api/v1/updates/tauri/:app_slug?current_version&target&arch&channel?&client_id?`,解析 200 flat JSON（`{version, pub_date, url, signature, notes, swarmhive:{upgrade_type, min_version, rollout_percent, channel}}`）/ 204 无更新。RN 的 `/updates/android` 待阶段 7,本次只做 Tauri。

### 6. 类型 codegen

从 server OpenAPI doc 生成 update 相关 TS 类型（复用 admin 的 `openapi-typescript` 链路）,wire 类型单一来源——server 改字段 SDK 类型自动跟。

### 7. `./react` 订阅层

`useUpdateEngine(engine)` 把 engine 状态订阅成 React state（peerDependency `react`,零平台依赖）。

## Acceptance

- `packages/sdk` build 产出 ESM + `.d.ts`,`"."` 与 `"./react"` 子入口均可被 import;`cargo`/`node` 检查包无 `@tauri-apps/*` / `expo-*` 依赖（零平台依赖）。
- 状态机单测覆盖 8 态全部合法转移 + dismiss-TTL + 强制更新绕过 dismiss。
- comparator 单测:semver `0.4.5 > 0.4.0`、versionCode `21 > 18`。
- **rollout 分桶跨语言一致**:用与 server `update_check_tauri_smoke` 灰度测试相同的 client_id 样本,TS 侧 `in_rollout_bucket` 命中集合与 server 完全一致。
- `checkUpdate` 对 mock 的 200 flat JSON / 204 各自解析正确（available / force-required / up-to-date）。
- 类型 codegen 跑通,生成的 `TauriUpdateResponse` 等类型与 server OpenAPI 一致。
- `pnpm lint` + `pnpm --filter @swarm-hive/sdk typecheck` + `pnpm --filter @swarm-hive/sdk test` 全绿。

## Non-goals

- 不做任何平台 adapter（Tauri/RN）、hook（`useUpdate`）、UI 组件——全在后续 registry change。
- 不做 RN `/updates/android` 的 check（依赖阶段 7 server endpoint）。
- 不迁移任何真实 app（SwarmDrop/SwarmNote 切换是各自仓库的后续集成步骤）。
- 不做 `/r/*.json` registry host（在 `add-registry-web-tauri`）。
- 不内置签名验证实现（Tauri 由 plugin 做、RN 由 adapter 做,均在 registry 层;sdk 只透传 signature/sha256 字段）。

## Depends on

- `add-update-check-tauri`（提供 `/api/v1/updates/tauri` endpoint + `TauriUpdateResponse` OpenAPI schema）
- `add-openapi-and-admin-client`（archived,提供 `openapi-typescript` codegen 链路范式）

## Maps to docs

- [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) —— 全文修订:包结构 4→1 npm + 2 registry、ports/adapter、hooks API。
- [memory/project-sdk-ui-split.md] —— 同步包结构修订。
- [openspec/changes/README.md](../README.md) —— 依赖图 + 阶段映射加 SDK 层节点。
