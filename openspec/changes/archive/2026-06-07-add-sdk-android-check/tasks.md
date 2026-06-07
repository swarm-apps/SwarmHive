## 1. wire 类型 codegen

- [x] 1.1 [code] 跑 `pnpm --filter @swarm-hive/sdk codegen`（`dump-openapi → openapi-typescript → biome`）重生成 `packages/sdk/src/generated/schema.ts`；确认拉入 `AndroidUpdateResponse`（含 `has_update`/`version_name`/`version_code`/`upgrade_type`/`min_version_code`/`download_url`/`release_notes`/`size_bytes`/`sha256`）
- [x] 1.2 [test] diff review 生成产物：除 `AndroidUpdateResponse` + `Release.android_min_version_code`（Change 1 带出的预期加性 drift）外无意外改动；biome 格式化通过

## 2. ReleaseInfo.kind? 轻 OTA 接缝

- [x] 2.1 [code] `types.ts` 给 `ReleaseInfo` 加 `kind?: "native-package" | "ota-bundle"`，doc 注释写明「缺省 ⇒ native-package；`"ota-bundle"` 留 Phase 2 `add-ota-provider`，MVP 无路径产出」（纯加性，不动其余字段）

## 3. checkUpdateAndroid + normalizeAndroid

- [x] 3.1 [code] 新建 `packages/sdk/src/check-update-android.ts`（与 `check-update.ts` 平行，头注对称）；定义 `CheckUpdateAndroidOptions`：`baseUrl`/`appSlug`/`currentVersionCode: number`/`currentVersionName: string`/`abi?`/`channel?`/`clientId?`/`runtimeVersion?`/`fetchImpl?`
- [x] 3.2 [code] `normalizeAndroid(wire): ReleaseInfo | null`：`has_update:false → null`；映射 `version←version_name`/`versionCode←version_code`/`url←download_url`/`signature←sha256`/`notes←release_notes(?? undefined)`/`upgradeType←upgrade_type`(未知枚举兜底 `"prompt"`，复用 `VALID_UPGRADE_TYPES`)/`minVersion←String(min_version_code)`(present 时)/`kind="native-package"`；`channel = opts.channel ?? "default"`（端点不回显 channel）
- [x] 3.3 [code] `checkUpdateAndroid(opts): Promise<ReleaseInfo | null>`：构造 query(`current_version_code`/`current_version_name`/`abi?`/`channel?`/`client_id?`/`runtime_version?`)；注入 `fetchImpl ?? fetch`；网络异常 → `throw UpdateError(..., "check", cause)`；`!res.ok` → `throw UpdateError(\`...HTTP ${status}\`, "check")`；**不**判 204；`200 → normalizeAndroid(body)`
- [x] 3.4 [code] `index.ts` 加 `export * from "./check-update-android"`（核心入口导出 `checkUpdateAndroid`/`normalizeAndroid`/`CheckUpdateAndroidOptions`）

## 4. contract test + fixture

- [x] 4.1 [test] 在 `packages/sdk` 内 vendor 一份同形状 fixture（`available`/`no_update`/`force` 三版 JSON，注释指回 `crates/swarmhive-server/tests/fixtures/android_update_response.json` 为源）
- [x] 4.2 [test] `check-update-android.test.ts`（vitest，注入 mock fetch）：① available → 全字段映射 + `kind="native-package"`；② `has_update:false` → `null`；③ `upgrade_type="force"` → `upgradeType="force"`；④ `400` → `UpdateError` phase `"check"`；⑤ `runtimeVersion` 透传进 query 且不改归一化结果；⑥ query 串含 `current_version_code`/`abi`/`client_id` 期望键

## 5. gates 与约定

- [x] 5.1 [test] `pnpm --filter @swarm-hive/sdk typecheck` + `test` + `build`(tsdown) 全绿；`pnpm --filter @swarm-hive/sdk assert:no-platform-deps` 通过（未引入 `expo-*`/`react-native`/`@tauri-apps/*`）
- [x] 5.2 [test] `pnpm lint`(biome) 通过；`generated/schema.ts` 仅加性 drift
- [x] 5.3 [docs] 新增代码注释用中文（对齐 CLAUDE.md：SDK 更新链路注释即用中文）；`check-update-android.ts` 头注与 `check-update.ts` 对称（标注 wire 来自 OpenAPI codegen、与 204-less 端点的差异）
