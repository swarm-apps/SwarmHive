## Context

RN 主线三段的第二段。第一段 `add-update-check-rn-android` 已定稿 server 的
`GET /api/v1/updates/android/:app_slug` 扁平响应（`AndroidUpdateResponse`：`has_update` +
有更新时其余 Option 字段；no-update 统一 200 出 `{has_update:false}`，**不**用 204）。

SDK core（`packages/sdk`，archived `add-update-sdk-core`）已铺好与平台无关的底座：
8 态引擎、`UpdateAdapter` ports、`semverComparator` + **`versionCodeComparator`（已存在）**、
`inRolloutBucket`、`ReleaseInfo`，以及 Tauri 侧的 `checkUpdate`/`normalizeTauri`。RN 侧缺的
**只是消费端点的对称入口**——一个 HTTP check + wire→`ReleaseInfo` 归一化，放 core（与
`checkUpdate` 同层），让未来 rnAdapter 只负责真正平台相关的 `download`/`install`/`storage`。

关键约束：

- SDK 有**独立的** codegen（`pnpm --filter @swarm-hive/sdk codegen`：
  `dump-openapi → openapi-typescript → biome`，产物 `src/generated/schema.ts`），与 admin 的
  `schema.gen.ts` 是两份。Change 1 只 regen 了 admin 那份，SDK 这份**还没有**
  `AndroidUpdateResponse`——本段必须先 regen。
- SDK 是零平台依赖纯 JS 包（`dependencies` 仅 `zustand`/`@noble/hashes`/`semver`），本段产出
  不得引入任何 `expo-*`/`react-native`/`@tauri-apps/*`。

## Goals / Non-Goals

**Goals:**

- 导出 `checkUpdateAndroid(opts): Promise<ReleaseInfo | null>` + `normalizeAndroid(wire)`，对称
  `checkUpdate`/`normalizeTauri`，消费 `/updates/android/:app_slug`。
- 把 `AndroidUpdateResponse` 经 SDK 自己的 codegen 拉进 `generated/schema.ts`。
- `ReleaseInfo` 加**轻 OTA 接缝** `kind?: 'native-package' | 'ota-bundle'`（纯加性）。
- contract test 锚定 Change 1 的 wire 形状（available / no-update / force），与 server fixture 同源。

**Non-Goals:**

- 不实现 rnAdapter / RN UI 组件 / expo-module 安装器（→ `add-registry-rn`）。
- 不实现 OTA / `expo-updates` / `checkUpdateOta`（→ `add-ota-provider`，Phase 2）。
- **不动** 8 态引擎、`UpdateAdapter` ports、`versionCodeComparator`、`inRolloutBucket`、
  `ensureClientId`（均已 platform-agnostic 且过跨语言锚点测试）。
- 不拆 `signature` 字段、不加 `confirmApplied` port（轻接缝原则，OTA 真做时再评估）。

## Decisions

### D1. `checkUpdateAndroid` 放新文件 `check-update-android.ts`，不挤进 `check-update.ts`

与 Tauri 入口平行的独立文件，各自聚焦一个端点；经 `index.ts` 的 `export *` 统一导出。
**备选**：追加进 `check-update.ts`——否决，会把两个端点的 query/归一化差异混在一个文件，
后续 OTA 第三个 check 入口也无处安放。

### D2. no-update 信号：`has_update:false`（200 体内），**不是** 204

这是与 Tauri 的结构性差异（Change 1 设计：RN 统一 200 便于 SDK 单分支解析）。
`normalizeAndroid` 见 `has_update:false` 直接返回 `null`；`checkUpdateAndroid` 不像
`checkUpdate` 那样判 `res.status === 204`。`4xx/5xx`（含 400 不可解析 versionCode）→
`throw UpdateError(..., "check")`，与 Tauri 一致。

### D3. wire→`ReleaseInfo` 映射

| ReleaseInfo | ← AndroidUpdateResponse | 备注 |
|---|---|---|
| `version` | `version_name` | 显示版本名 |
| `versionCode` | `version_code` | 整数闸门主键（`versionCodeComparator` 消费） |
| `url` | `download_url` | `has_update:false` 时 absent → 该路径已返回 null |
| `signature` | `sha256` | RN 用 sha256 占 signature 槽（Tauri 用 minisign，复用同字段） |
| `notes` | `release_notes` | `?? undefined` 归一 null |
| `upgradeType` | `upgrade_type` | 同 Tauri：未知枚举值运行时兜底 `'prompt'` |
| `minVersion` | `String(min_version_code)` | `ReleaseInfo.minVersion` 是字符串（semver/versionCode 共用槽） |
| `kind` | —（恒 `'native-package'`） | 见 D4 |
| `rolloutPercent`/`pubDate` | —（不出） | 端点不下发；保持 undefined |

`channel` 由 `ReleaseInfo.channel` 必填——Android 端点响应不回显 channel 名，故由
**opts.channel ?? 'default' 兜底**写入（与请求一致），避免破坏 `ReleaseInfo.channel: string` 契约。

### D4. `ReleaseInfo.kind?` 语义：缺省即 native-package，只有显式 `'ota-bundle'` 才是 OTA

`normalizeAndroid` 显式写 `kind: 'native-package'`。**不回头改** `normalizeTauri`——
约定 **`kind` 缺省 ⇒ native-package**，消费方只需判 `release.kind === 'ota-bundle'`。
好处：纯加性、零 blast radius（不碰已 archived 的 Tauri 归一化路径），又给 Phase 2 OTA 留下
唯一判别位。**备选**：两个 normalizer 都写 native-package——否决，徒增对 Tauri 路径的改动而
语义等价（缺省约定已覆盖）。

### D5. contract test 用 vendored fixture，不跨 crate 读 server 路径

Change 1 落了 `crates/swarmhive-server/tests/fixtures/android_update_response.json`。SDK 测试
**不**跨 workspace 相对路径读它（脆、CI 工作目录不稳）。改为在 `packages/sdk` 内 vendor 一份
同形状 JSON（注释指回 server 源），断言 `normalizeAndroid(available/no-update/force)` 的输出。
wire 形状的单一事实源仍是 server `AndroidUpdateResponse`（codegen 保证类型对齐；fixture 仅锚值）。

### D6. `CheckUpdateAndroidOptions` 形状镜像 server `AndroidUpdateQuery`

`baseUrl` / `appSlug` / `currentVersionCode: number`（序列化进 query 串）/
`currentVersionName: string` / `abi?` / `channel?` / `clientId?` / `runtimeVersion?` / `fetchImpl?`。
`runtimeVersion?` 接收并透传进 query（**OTA 接缝占位**，server 当前忽略）——让 OTA 真做时
SDK 调用方无需改签名。

## Risks / Trade-offs

- **[SDK codegen 依赖 Rust 工具链]**（`dump-openapi` 要 `cargo run`）→ 缓解：与已有 Tauri
  codegen 同一条链，本地/CI 已具备；regen 后产物入库，消费侧 typecheck 不再需 Rust。
- **[regen 带出额外 drift]**（除 `AndroidUpdateResponse`，Change 1 还给 `Release` 加了
  `android_min_version_code`）→ 缓解：预期内、纯加性；biome 统一格式；diff review 确认只增不改。
- **[`kind` 缺省约定被穷尽 switch 漏掉]**（消费方 `switch(kind)` 无 default）→ 缓解：spec 明文
  「缺省 ⇒ native-package」，且 MVP 无任何路径产出 `'ota-bundle'`，实际只有一个分支。
- **[Android 响应不回显 channel]** → 缓解：D3 用 `opts.channel ?? 'default'` 兜底，
  契约 `ReleaseInfo.channel: string` 不破。

## Migration Plan

纯加性 npm 包表面扩展，无数据模型/无 server 改动/无版本破坏：

1. regen `generated/schema.ts`（拉入 `AndroidUpdateResponse`）。
2. 加 `ReleaseInfo.kind?`、`check-update-android.ts`、contract test。
3. `pnpm --filter @swarm-hive/sdk {typecheck,test,build}` + `assert:no-platform-deps` 全绿。

回滚：撤销本段 commit 即可，无外部副作用（无人 publish 消费前）。

## Open Questions

- 无阻塞项。`checkUpdateOta` 入口与 `kind:'ota-bundle'` 的产出留给 `add-ota-provider`（Phase 2），
  本段只埋判别位不实现。
