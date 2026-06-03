# tasks

> 实施顺序:scaffold(1) → 类型 codegen(2) → ports/算法(3-4) → engine(5) → checkUpdate(6) → react(7) → 测试(8) → CI 守护(9) → docs(10)。
> 全程零平台依赖;不写任何平台 adapter / hook / UI(那是 registry change)。

## 1. scaffold packages/sdk

- [x] 1.1 [code] 新建 `packages/sdk/package.json`:name `@swarm-hive/sdk`、`type: module`、`exports` map(`"."` → `./dist/index.js`+types、`"./react"` → `./dist/react.js`+types)、`dependencies`: `zustand` / `@noble/hashes` / `semver`、`peerDependencies.react` + `peerDependenciesMeta.react.optional=true`(D6)
- [x] 1.2 [code] `packages/sdk/tsup.config.ts`:entry `src/index.ts` + `src/react.ts`,format ESM,`dts: true`,`treeshake`,`clean`;加 `build` / `dev` / `typecheck`(`tsc --noEmit`) / `test` script
- [x] 1.3 [code] `packages/sdk/tsconfig.json`(继承根或自带,`moduleResolution: bundler`、`strict`);确认 `pnpm install` 后 `packages/sdk` 进 workspace(`packages/*` 已配)
- [x] 1.4 [code] biome 纳入(根 `pnpm lint` 覆盖 `packages/sdk`,生成目录 `src/generated` 加进 biome ignore 或 codegen 后 format)

## 2. 类型 codegen(OpenAPI 单一来源,D5)

- [x] 2.1 [code] `packages/sdk` 加 `codegen` script:`cargo run -p swarmhive-server --bin dump-openapi --quiet > /tmp/swarmhive-openapi.json && openapi-typescript /tmp/swarmhive-openapi.json -o src/generated/schema.ts && biome check --write src/generated/schema.ts`(复刻 admin)
- [x] 2.2 [code] 跑 codegen,确认 `src/generated/schema.ts` 含 `TauriUpdateResponse` / `TauriUpdateExtensions` / `UpgradeType`;`src/types.ts` 从中取并薄封装成跨平台 `ReleaseInfo`

## 3. ports + 公共类型(D1)

- [x] 3.1 [code] `src/ports.ts`:`UpdateAdapter` interface(`check` / `download(onProgress)` / `install` / `storage: KeyValueStorage` / `compare`)+ `KeyValueStorage` / `DownloadHandle` / `CheckContext`
- [x] 3.2 [code] `src/types.ts`:`ReleaseInfo`(归一化 version/versionCode?/url/signature/notes/pubDate/upgradeType/minVersion/rolloutPercent/channel)、`UpdateStatus`(8 态 union)、`UpgradeType`、`Progress`、`UpdateError`(class)

## 4. 纯算法(与 server 同语义,D3/D4)

- [x] 4.1 [code] `src/compare.ts`:`semverComparator`(`strip_v` + `semver.gt`,对齐 server `add-update-check-tauri` 口径)+ `versionCodeComparator`(整数 >)
- [x] 4.2 [code] `src/rollout.ts`:`inRolloutBucket(clientId, percent)` —— `@noble/hashes` blake3 → `DataView.getBigUint64(0, true)` 前 8 字节 LE → `% 100n < percent`,`>=100→true`/`<=0→false` 短路(**逐位对齐 server `in_rollout_bucket`**)
- [x] 4.3 [code] `src/client-id.ts`:`ensureClientId(storage)` —— 读 storage,无则生成 uuid v4 写回(灰度稳定标识,server 灰度的前提)

## 5. engine —— zustand vanilla 状态机(D2)

- [x] 5.1 [code] `src/engine.ts`:`createUpdateEngine(adapter, opts)` 用 `zustand/vanilla` `createStore`;state `{status, release, progress, error, upgradeType, currentVersion}`
- [x] 5.2 [code] actions:`check()`(checking → adapter.check → compare → up-to-date|available|force-required;node/异常 → error)、`download()`(downloading → adapter.download(onProgress 更新 progress)→ ready;失败 → error)、`install()`(adapter.install)、`postpone(ttlMs?)`、`retry()`、`acknowledgeError()`
- [x] 5.3 [code] dismiss-TTL(默认 24h,`postpone` 写 storage,同版本过期前不再标 available;**force 绕过**)+ `recheckIntervalMs`(默认 12h,`check` 节流);`check()` 幂等(供平台回前台事件复用)(D8)

## 6. checkUpdate —— Tauri check(D7)

- [x] 6.1 [code] `src/check-update.ts`:`checkUpdate(opts)` GET `/api/v1/updates/tauri/:app_slug?current_version&target&arch&channel?&client_id?`;`204 → null`;`!ok → throw UpdateError`;`200 → normalize(TauriUpdateResponse) → ReleaseInfo`(`upgradeType` 直接取 `swarmhive.upgrade_type`,无数字映射)

## 7. ./react 订阅层

- [x] 7.1 [code] `src/react.ts`:`useUpdateEngine(engine)` 用 `zustand` 的 `useStore(engine.store, selector)`;`react` 为 optional peer;core 不 import react

## 8. 测试(Acceptance 硬指标)

- [x] 8.1 [test] engine 状态机单测(mock adapter):8 态全部合法转移 + available/force-required 分支 + download error→retry→checking + dismiss-TTL + force 绕过 dismiss
- [x] 8.2 [test] comparator 单测:`semverComparator`(`0.4.5>0.4.0`、`v0.4.0` 容忍、相等不更新)、`versionCodeComparator`(`21>18`、`21==21` 不更新)
- [x] 8.3 [test] **rollout 跨语言一致性单测**:用与 server `update_check_tauri_smoke::rollout_bucketing` **相同的 client_id 样本**(`client-0`..`client-N` + 固定 percent),断言 TS `inRolloutBucket` 命中集合与 server 一致(固化一组已知命中/未命中样本作回归闸门)
- [x] 8.4 [test] `checkUpdate` 单测(mock fetch):204→null、200 flat JSON→ReleaseInfo(upgradeType 取自 swarmhive)、4xx→UpdateError
- [x] 8.5 [test] `ensureClientId` 单测:首次生成 + 二次稳定返回同值
- [x] 8.6 [test] 测试框架用 vitest(与 admin 一致);`pnpm --filter @swarm-hive/sdk test` 接入

## 9. CI 守护 —— 零平台依赖

- [x] 9.1 [code] 加守护(脚本或测试):断言 `@swarm-hive/sdk` 的依赖树无 `@tauri-apps/*` / `expo-*` / `react-native`(同 CLI `cargo tree | grep sea-orm` 范式);可在 package.json 加 `assert:no-platform-deps` script + CI 调用
- [x] 9.2 [code] 确认 `"."` 与 `"./react"` 两个子入口 build 产物均带 `.d.ts`,import resolution 正确

## 10. docs / memory / openspec 同步

- [x] 10.1 [docs] **重写 `docs/14-sdk-ui.md`**:包结构 4 npm → **1 npm(`@swarm-hive/sdk`,ports/engine/算法/类型,零平台依赖)+ 2 registry(平台 adapter+hook+UI 源码)**;加 ports & adapters 段(`UpdateAdapter` 接口);接入流程改为 `pnpm add @swarm-hive/sdk` + `shadcn add` adapter/组件;hooks API 改为 npm `createUpdateEngine`/`useUpdateEngine` + registry `useUpdate`;保留状态机/组件清单/样式/i18n/registry host/非目标
- [x] 10.2 [docs] 更新 `memory/project-sdk-ui-split.md`:包结构修订(4→1+2)、ports/adapter 模式、平台 adapter 进 registry 的理由
- [x] 10.3 [docs] `dev-notes/knowledge/` 加(或并入)前端 SDK 段:packages/sdk 的 ports/adapter 边界、zustand vanilla engine、blake3 跨语言对齐 server、codegen 链路、零平台依赖守护
- [x] 10.4 [docs] `openspec/changes/README.md` 依赖图 + 阶段映射加 SDK 层节点(`add-update-sdk-core` → `add-registry-web-tauri` → `add-update-check-rn-android` + `add-registry-rn`)
- [x] 10.5 [code] 质量门:`pnpm lint` + `pnpm --filter @swarm-hive/sdk typecheck` + `pnpm --filter @swarm-hive/sdk build` + `pnpm --filter @swarm-hive/sdk test` 全绿

## 跨 proposal 联动

- [x] 11.1 标注后续依赖:`add-registry-web-tauri` 将实现 `tauriAdapter: UpdateAdapter` + `useUpdate` + UI 组件 + server `/r/*.json` host,消费本 change 的 ports;若实现 adapter 时发现 `UpdateAdapter` 接口缺字段,回头改本 change 的 ports(接口仍年轻,见 design R1)
