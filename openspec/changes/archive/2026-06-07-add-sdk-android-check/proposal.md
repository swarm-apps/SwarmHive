# add-sdk-android-check

> **状态**：stub（scope 已定，design/specs/tasks 待 `/opsx:propose` 补全）。RN 主线三段的**第二段**。

## Why

`add-update-check-rn-android` 定稿了 server 的 `GET /api/v1/updates/android/:app_slug` 扁平响应。SDK 侧需要一个**对称于 Tauri** 的入口来消费它：Tauri 的 `checkUpdate`/`normalizeTauri` 在 SDK core（不在 adapter 里），RN 的 HTTP check + normalize 也应放 core，让 rnAdapter 只负责真正平台相关的 `download`/`install`/`storage` 三个 port。

## What Changes

- `packages/sdk/src/check-update.ts` 旁新增 `checkUpdateAndroid(opts): Promise<ReleaseInfo | null>` + `normalizeAndroid(wire)`：把 `AndroidUpdateResponse` 映射到 `ReleaseInfo`（`version=version_name`、`versionCode=version_code`、`url=download_url`、`signature=sha256`、`minVersion=String(min_version_code)`、`has_update:false → null`）。
- Android wire 类型经 OpenAPI codegen（与 admin 同链路）；可加 contract test 锚定 Change 1 的 fixture JSON。
- **轻 OTA 接缝**：`ReleaseInfo` 加 `kind?: 'native-package' | 'ota-bundle'`（默认/MVP 恒 `native-package`，`normalizeAndroid` 据端点设 `native-package`）——纯加性、避免 registry-rn 铺开后给 `ReleaseInfo` 加字段变 breaking。
- **不动** core 引擎 / `versionCodeComparator` / `inRolloutBucket` / `ensureClientId`（已 platform-agnostic 且过跨语言锚点测试）；不动 8 态机；不拆 `signature` 字段。

## Capabilities

### Modified Capabilities
- `update-sdk-core`：新增 `checkUpdateAndroid`/`normalizeAndroid` 导出 + `ReleaseInfo.kind?` 判别字段（轻 OTA 接缝）。引擎/ports/比较器/rollout 行为不变。

## Impact

- `packages/sdk/src/`：`check-update.ts`（或新 `check-update-android.ts`）+ `types.ts`（`ReleaseInfo.kind?`）+ generated wire 类型 + 测试。
- 不触碰 server / registry / admin。

## Non-goals

- 不实现 rnAdapter / RN UI 组件（拆 `add-registry-rn`）。
- 不实现 OTA（`add-ota-provider`，Phase 2）。
- 不改 8 态机 / 不加 `confirmApplied` port / 不拆 signature 字段（轻接缝原则，OTA 真做时再评估）。

## Depends on

- `add-update-check-rn-android`（server 响应 schema 定稿后才能 codegen + normalize + contract test）。

## Maps to docs

- [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) SDK / hooks / 类型单一来源。
