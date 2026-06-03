# design

## Context

docs/14 原规划 4 个 npm 包（sdk-core / sdk-core-react / tauri / react-native）+ 2 registry。剖析 4 个真实 app 后修订为 **1 npm + 2 registry + ports/adapter**:

- 平台适配代码（Tauri plugin-updater 包装、RN PackageInstaller）因宿主环境差异**本就需要用户改源码** → registry 源码分发,不锁进 npm。
- npm 包零平台依赖,`optional peerDependencies` 的工程复杂度消失。

本 change 只做 npm 包 `@swarm-hive/sdk` 的 headless 地基:ports（接口契约）+ engine（状态机）+ 纯算法 + checkUpdate + 类型 codegen + react 订阅层。registry / 平台 adapter / UI 是后续 change。

## Goals / Non-Goals

**Goals:**

- `@swarm-hive/sdk` 纯 headless、零平台依赖、ports & adapters
- 8 态状态机引擎 framework-agnostic,可被 Tauri/RN adapter 复用
- 版本比较可插拔（semver / versionCode），灰度分桶**跨语言对齐 server**
- 类型从 server OpenAPI codegen,wire 类型单一来源

**Non-Goals:**

- 不做平台 adapter / hook（`useUpdate`）/ UI（全在后续 registry change）
- 不做 RN check（依赖阶段 7 server）
- 不做 `/r/*.json` host、不迁移真实 app

## 分层与数据流

```text
  App 业务层(SwarmDrop/SwarmNote 各自的 about 页 / 启动挂载)
        │  import { useUpdate } from "@/components/swarmhive/use-update"  (registry 源码)
        ▼
  ┌─────────────────────────────────────────────────────────────┐
  │ registry-web / registry-rn  (后续 change,源码复制进 app)       │
  │   tauriAdapter / rnAdapter : UpdateAdapter   ← 实现 ports      │
  │   useUpdate() = useUpdateEngine(createUpdateEngine(adapter))   │
  │   <PromptUpdateDialog/> <ForceUpdateDialog/> ...              │
  └───────────────────────────────┬─────────────────────────────┘
                                  │ 依赖(npm semver)
        ┌─────────────────────────▼─────────────────────────────┐
        │ @swarm-hive/sdk  (本 change · 零平台依赖)               │
        │                                                        │
        │  "."  core:                                            │
        │    UpdateAdapter (ports)  ← 唯一契约                    │
        │    createUpdateEngine(adapter) → 8 态状态机             │
        │    compare: semverComparator / versionCodeComparator   │
        │    inRolloutBucket(clientId, percent)  ← 对齐 server    │
        │    checkUpdate(endpoint, params) → ReleaseInfo | null   │
        │    types (OpenAPI codegen): TauriUpdateResponse ...      │
        │  "./react":  useUpdateEngine(engine) → React state      │
        └───────────────────────────┬───────────────────────────┘
                                    │ checkUpdate() fetch
                                    ▼
              GET /api/v1/updates/tauri/:app_slug   (add-update-check-tauri)
              200 flat JSON | 204 No Content

  类型单一来源:
    server OpenAPI doc ──dump-openapi──▶ openapi-typescript ──▶ packages/sdk/src/generated/schema.ts
```

## Decisions

### D1. ports & adapters —— `UpdateAdapter` 是 npm↔registry 唯一契约

```ts
export interface KeyValueStorage {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
}

export interface DownloadHandle { release: ReleaseInfo; /* 平台不透明句柄 */ }

export interface UpdateAdapter {
  /** 打 SwarmHive endpoint(或复用平台原生 check),归一化成 ReleaseInfo;无更新返 null */
  check(ctx: CheckContext): Promise<ReleaseInfo | null>;
  /** 下载(带进度回调);Tauri 内部可直接 downloadAndInstall,RN 下 APK 到缓存 */
  download(release: ReleaseInfo, onProgress: (p: Progress) => void): Promise<DownloadHandle>;
  /** 安装(+重启);Tauri relaunch / RN PackageInstaller */
  install(handle: DownloadHandle): Promise<void>;
  /** 持久化 client_id / dismiss-TTL / lastCheckedAt */
  storage: KeyValueStorage;
  /** "candidate 是否比 current 新":semver(Tauri) / versionCode(RN) */
  compare(current: string, candidate: ReleaseInfo): boolean;
}
```

engine 只依赖这个接口。`check` 放进 adapter（而非 engine 内置 fetch）是因为 Tauri 可能复用 plugin-updater 的 `check()`、RN 走自定义 fetch,两者归一化路径不同。`checkUpdate` 纯函数(D7)是给 adapter 复用的便捷实现,但 adapter 可不用。

**接口稳定性**:这是整个 SDK 的脊梁,一旦 registry 铺开就难改。本 change 把它定准,字段宁少勿多;扩展走可选字段。

### D2. engine —— zustand vanilla 状态机

**选 zustand vanilla（`zustand/vanilla` 的 `createStore`）**,不手写 reducer、不引 XState:

- 4 个真实 app 都用 zustand,认知一致、迁移摩擦最小。
- `zustand/vanilla` framework-agnostic（不依赖 React/DOM/RN）,放 npm 不破坏零平台依赖。
- `./react` 用 `zustand` 的 `useStore(engine.store, selector)` 做订阅。

8 态转移:

```text
        check()                    compare 无更新
  idle ────────▶ checking ──────────────────────▶ up-to-date
                   │                                  │ check()(回前台/手动)
                   │ compare 有更新                     ▼
                   ├──────────▶ available ───┐      checking
                   │            (prompt)      │ download()
                   ├──────────▶ force-required┤────────▶ downloading ──progress──▶ ready
                   │            (force)        │              │ error            (install→重启)
                   └──────────▶ error ◀───────┴──────────────┘
                                  │ retry()
                                  └──▶ checking
```

- `force-required` 与 `available` 都可 `download()`;区别只在 UI（force 阻塞、不可 dismiss）。
- engine 暴露 actions:`check()` / `download()` / `install()` / `postpone(ttlMs?)` / `retry()` / `acknowledgeError()`;state:`status` / `release` / `progress` / `error` / `upgradeType` / `currentVersion`。

### D3. 版本比较 —— 可插拔 comparator

engine 不内置版本语义;`adapter.compare(current, candidate)` 注入。sdk 提供两个现成实现给 adapter 复用:

- `semverComparator`:`semver.gt(strip_v(candidate.version), strip_v(current))`——与 server `add-update-check-tauri` 同口径（单个前导 `v`、`semver` crate 同算法,TS 用 `semver` npm）。
- `versionCodeComparator`:`candidate.versionCode > current`（整数,RN）。

> server 已对 Tauri 做 semver 比较决定 200/204,但 SDK 仍本地比一次:① Tauri updater 默认也复核;② RN 链路 server 未定,SDK 比较是兜底;③ `min_version` → force 的推导在 SDK 侧（`compare(min_version, current)`）。

### D4. 灰度分桶 —— 跨语言逐位对齐 server（硬约束）

server `routes/updates.rs::in_rollout_bucket`:`blake3(key)` → 前 8 字节 `u64::from_le_bytes` → `% 100 < percent`。TS 必须**完全一致**:

```ts
import { blake3 } from "@noble/hashes/blake3";   // 纯 JS,跨 Tauri webview + RN,无 native
export function inRolloutBucket(clientId: string, percent: number): boolean {
  if (percent >= 100) return true;
  if (percent <= 0) return false;
  const h = blake3(new TextEncoder().encode(clientId));               // Uint8Array(32)
  const n = new DataView(h.buffer, h.byteOffset, 8).getBigUint64(0, true); // LE u64,对齐 from_le_bytes
  return (n % 100n) < BigInt(percent);
}
```

- key = `client_id` 的 UTF-8 bytes（server `client_id.as_bytes()` 同）;`@noble/hashes` 的 blake3 与 Rust `blake3` crate 同算法。
- **验收硬指标**:用与 server `update_check_tauri_smoke` 灰度测试相同的 client_id 样本（`client-0`..`client-N`）,TS `inRolloutBucket` 命中集合必须与 server 完全一致——否则同一用户在 server 端在桶、SDK 端判断不一致,灰度失真。单测固化一组已知样本的命中/未命中。

### D5. 类型 codegen —— OpenAPI 单一来源

复用 admin 的链路:`cargo run --bin dump-openapi → /tmp → openapi-typescript → packages/sdk/src/generated/schema.ts → biome format`。加 `pnpm --filter @swarm-hive/sdk codegen` script。sdk 从 `schema.ts` 取 `components["schemas"]["TauriUpdateResponse"]` 等,薄封装成 `ReleaseInfo`（归一化跨平台:Tauri 的 `version` 与未来 RN 的 `versionCode` 合一）。

> 生成全量 schema（同 admin）但只 re-export update 子集;不另搞 codegen 工具,与 admin 保持同一范式,server 改 wire 字段两处 codegen 都自动跟。

### D6. build 工具 + exports + 零平台依赖

- **build:tsup**（esbuild,成熟,原生支持多 entry + dts + ESM）。entry:`src/index.ts`（core）、`src/react.ts`。输出 ESM only（现代 Tauri/RN bundler 都吃 ESM）。
- `package.json`:
  - `exports`:`"."` → core、`"./react"` → react 子入口（各带 `types`）。
  - `dependencies`:`zustand`、`@noble/hashes`、`semver`（全平台无关纯 JS）。
  - `peerDependencies`:`react`（仅 `./react` 子入口需要,`peerDependenciesMeta.react.optional = true`,core 不依赖）。
  - **无** `@tauri-apps/*` / `expo-*` / `react-native` —— CI 加 `assert no platform deps` 守护（同 CLI 的 `cargo tree | grep sea-orm` 范式）。

### D7. checkUpdate —— Tauri 解析

```ts
async function checkUpdate(opts): Promise<ReleaseInfo | null> {
  const res = await fetch(url, { ... });
  if (res.status === 204) return null;                // 无更新
  if (!res.ok) throw new UpdateError(...);            // 400/404/5xx
  const body = await res.json() as TauriUpdateResponse;
  return normalize(body);  // → { version, url, signature, notes, pubDate, upgradeType, minVersion, rolloutPercent, channel }
}
```

`upgrade_type` 直接取 `swarmhive.upgrade_type`（已是 `prompt|force`,不再像 ToolSetLink 那样 `0|1|2|3` 数字映射——迁移净简化）。

### D8. dismiss-TTL + 回前台重检（吸收 SwarmDrop-RN 设计）

engine opts 暴露:`dismissTtlMs`（默认 24h,`postpone()` 写 storage,过期前同版本不再弹）、`recheckIntervalMs`（默认 12h,`check()` 节流）。回前台重检不在 engine 内（需平台 AppState/window focus 事件）,但 engine 暴露幂等 `check()` 供 adapter 在平台事件里调。强制更新（force-required）**绕过 dismiss**。

## Risks

- **R1 ports 接口锁定过早**:registry 未落地就定接口,可能漏字段。缓解:接口字段宁少勿多、留可选扩展;本 change 先用一个 in-repo fixture adapter（mock）验证 engine,真实 adapter 在下个 change 暴露问题时回头改（接口仍年轻,代价可控）。
- **R2 blake3 跨语言**:`@noble/hashes` 与 Rust `blake3` 若某版本行为差异会让灰度失真。缓解:D4 的样本一致性单测是回归闸门。
- **R3 codegen 与 admin 重复生成**:两个 package 各跑 openapi-typescript。可接受（各自独立、无 cross-import）;未来若想共享可提一个 `packages/api-schema`,但现在不预先抽象（YAGNI,符合项目"等第二 consumer"原则——admin + sdk 恰是第二个,但收益不足以现在抽）。
- **R4 ESM only**:若某 app 的 bundler 不支持,再加 CJS。Tauri（Vite）/ Expo（Metro）都吃 ESM,暂不需要。
