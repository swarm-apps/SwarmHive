## Context

SwarmHive 已落地 Tauri 更新链路（`update-check-tauri` endpoint + `update-sdk-core` + `registry-web-tauri`）。RN Android 是 MVP 第二条主链路（docs/09 阶段 7）。目标产品（SwarmNote-RN 等）是 **Expo-first 的侧载应用**（不走 Google Play）。

调研（两轮 workflow + 对抗审查）确认的关键事实：

- **Expo 更新天然分两层**：`expo-updates`（OTA）只换 JS bundle + assets，被 `runtimeVersion` 锁死在"原生层不变"前提，**结构性绝不下载/安装 APK**；任何原生变更（SDK 升级 / 新原生模块 / fingerprint 变）必须发新 APK。二者正交、不重叠。
- **侧载缺口**：EAS Build 给侧载用户的只是一个手动下载页 + 二维码，**没有应用内"检查→下载→安装新 APK"运行时闭环**。这正是 SwarmHive native 链路要填的那一半。
- **SDK 已就绪**：`@swarm-hive/sdk` 的 8 态引擎、`versionCodeComparator`、`ensureClientId`、`inRolloutBucket` 全 platform-agnostic 且过跨语言锚点测试。RN 落地 ≈ server endpoint（本 change）+ 薄 rnAdapter + registry-rn UI（后续 change）。

本 change 只做 **server endpoint**，是三段链路的阻塞项。现有代码锚点：`crates/swarmhive-server/src/routes/updates.rs`（`tauri()` handler 全套可复用 helper）、`crates/swarmhive-entity/src/release.rs`（已有 `android_version_code: Option<i64>`、`min_version: Option<String>` semver）、`crates/swarmhive-entity/src/artifact.rs`（`(release_id, platform, target, arch, abi)` 唯一元组）、`crates/swarmhive-api-types/src/update.rs`（Tauri DTO，无 `kind` 字段）。

## Goals / Non-Goals

**Goals:**
- 一个对齐 Tauri 端点形态、用 `versionCode` 整数闸门 + ABI 匹配的 `GET /api/v1/updates/android/:app_slug`，最大化复用 channel/rollout/telemetry helper。
- 扁平 `has_update` 响应 schema 稳定 + fixture JSON，给后续 RN SDK contract test 锚定。
- 为 RN 强制更新提供整数下限（新增 `android_min_version_code` 列）。
- 留**轻 OTA 接缝**（注释 + query 占位），不预选 OTA provider 形态。

**Non-Goals:**
- 不实现 SDK / rnAdapter / registry-rn / 安装器（后续 change）。
- 不实现 OTA（Phase 2；docs/11 两候选保持开放）。
- 不建 OTA 兼容键列 / `update_kind` telemetry 列（仅注释占位）。
- 不在 server 做 APK 签名验真（Android 安装器在安装时兜底）。

## Decisions

### D1: 扁平响应 + `has_update` boolean + 统一 200（不用 Tauri 的 204）
Tauri 用 204 absence 语义是因为 plugin-updater 的 dynamic 协议如此约定。RN SDK 是 JSON-based、要显式 `has_update` boolean。统一 200（`has_update:false` 时省略 `download_url`）更适合 RN 客户端解析。RN 不需要 Tauri 的 `swarmhive` 私有命名空间（那是为不破坏 Tauri 官方契约；本端点是 SwarmHive 自有协议）。channel 显式不存在仍 404、release 缺失/未 published → `has_update:false`（对齐 Tauri 语义）。
**备选**：嵌套对齐 Tauri / 204 absence —— 均增加 RN 解析负担，否决。

### D2: `versionCode` i64 整数闸门，不复用 Tauri 的 semver
`current_version_code < release.android_version_code`（i64 比较）。**绝不**抄 Tauri 的 `strip_v` + `semver::Version::parse`。`android_version_code` 为 None 的 release 视为非 RN release 跳过。这与 SDK 的 `versionCodeComparator` vs `semverComparator` 分工一致。
**风险**：实现者误抄 Tauri 闸门 —— 在 tasks 显式标注，代码复用要精准切分 shared helper vs 平台分支。

### D3: `match_rn_artifact` —— ABI 匹配，允许跨 ABI 降级，不做 signature gating
filter `platform == ReactNativeAndroid` → 精确 `abi` → fat APK（`abi=None`，兼容所有）→ 单 untargeted fallback。**允许跨 ABI 降级**（client 要 arm64-v8a、只有 armeabi-v7a 时返回 v7a，arm64 设备向下兼容；对齐 proposal §2「v7a/x86_64 fallback」意图）。**不做** Tauri 的 `tauri_signature(a).is_some()` gating——APK 真伪由 Android 安装器在安装时验 v2/v3 签名兜底。
**关键**：把"不 gate"锚在 **kind 级**（native-package 不 gate / 未来 ota-bundle 必须应用层验签）而非 **platform 级**——注释写明，防止未来 OTA 沿 RN 线接入时错误继承"RN 不 gate"。
**备选**：严格只精确 ABI（拒降级）—— 比 proposal 意图更严、可能拒掉可用包，否决。

### D4: 新增 `android_min_version_code: Option<i64>`，不复用 `min_version`(semver)
现有 `min_version` 是 semver（Tauri 用 `semver::Version::parse` 比较）。RN 强更下限是整数。复用同列会逼 handler 把 semver 字符串 parse 成整数（`'18.5'` 该是什么 versionCode？语义错位且脆弱）。两条独立下限最干净。`upgrade_type` 由 server 计算（非客户端决定）；调高 `android_min_version_code` 即 **kill switch**（retroactively 强更所有低于它的客户端，无需发新 APK）。
**备选**：复用 `min_version` 要求运维填整数字符串 —— 语义污染，否决。

### D5: 不可解析的 `current_version_code` → 400
对齐 Tauri handler 对 invalid semver 造 typed 400 的既定模式（fail-loud）。用 `i64`（serde 反序列化失败默认 422/400）或 `Option<i64>` + 手工校验返结构化 RFC 9457 错误——tasks 里定。
**备选**：宽松 `has_update:false` —— 掩盖客户端 bug，否决。

### D6: 灰度静默失效的双重防御
直连单机部署无 `X-Forwarded-For` → IP 为 None → `rollout<100` 时静默变 100%。防御：(a) **SDK 侧**（后续 change）`createRnAdapter` 内部必带 `ensureClientId` 的 `client_id` query；(b) **server 侧**保持 Tauri 同款 `client_id → IP → 命中+warn` 三级回退（一致性）。本 change 的 query 显式含 `client_id?`。
**备选**：`rollout<100` 且无 client_id 时保守拒绝 —— 改变 Tauri 既定语义、过严，记 Open Question 不在本 change 拍。

### D7: 轻 OTA 接缝（server 侧仅注释/占位）
用户拍板"轻接缝、形态留给未来"。本 change 的 server 侧动作：
- `release.rs` doc 注释：OTA 可下发性靠 `runtime_version`/fingerprint 精确匹配（≠ `android_version_code` 整数闸门）；Phase 2 OTA 另立兼容键，**不在本 change 建列**。
- endpoint 留 `runtime_version?` query 占位（MVP 不消费）——避免日后 breaking + 为 SwarmHive 独有价值（"runtimeVersion 错配 → 干净的 native 强更信号"，EAS Update 给不出这个）预留。
- **不**在 wire 加 `update_kind` 字段：`/updates/android` 端点路径本身即 native-package 判别；未来 OTA 走独立 `/updates/ota` 端点。`kind` 判别字段属 SDK 的 `ReleaseInfo`（后续 change）。
- **不**为 OTA 加 telemetry 专列：审查指出 `add-telemetry-events` 的 `update_event` 已有 `platform` 列 + `metadata_jsonb`，OTA 走 jsonb 即可（守"第二个 consumer 再抽象"）。
**OTA provider 形态不预选**：docs/11 的两候选（Expo Updates Provider 自实现 / External Sync 外部同步）保持开放，留给未来 `add-ota-expo-updates-server` 占位 change 裁。

### D8: APK 不加 minisign，靠 Android 签名 + sha256 完整性预校验
Android 安装器在安装时强制 v2/v3 整包签名校验 + 同密钥更新约束（`INSTALL_FAILED_UPDATE_INCOMPATIBLE`）+ 降级保护（`INSTALL_FAILED_VERSION_DOWNGRADE`），已覆盖篡改/冒充/回滚三大威胁。Tauri 需 minisign 是因为桌面产物无等价 OS 级验真。响应里的 `sha256` 定位为**传输完整性预校验 + fail-fast UX**（跳安装器前校验下载文件），不是信任锚。
**备选**：叠加 minisign 对齐 Tauri —— 冗余（改过的 APK 既过不了 Android 签名）+ 增密钥管理负担，否决。

## Risks / Trade-offs

- [实现者误抄 Tauri semver 闸门 / signature gating] → tasks 显式标注三处平台分支；集成测试覆盖 i64 闸门 + 无 gating。
- [`android_min_version_code` 缺失逼 handler 错用 semver] → 本 change 必含 migration 子项，先于 handler。
- [AAB 被误当 APK 上传] → AAB 只能走 Play；CLI/文档引导出 APK（`eas build` / `gradle assembleRelease`），本 change 文档补一句。
- [per-ABI split APK 的 versionCode offset] → Google 推荐每 ABI +offset，会影响整数闸门比对；文档提示，匹配逻辑按 artifact 行的 `android_version_code` 实际值走。
- [fat APK 写入侧未定] → 本 change 只覆盖**读取侧**匹配（`abi=None` = universal）；写入侧（CLI 如何产 `abi=None` 行、与 per-ABI 行共存优先级）记 Open Question，归 CLI/发布侧。
- [灰度静默失效] → D6 双重防御。
- [keystore 漂移] → 开发者换 keystore 不走 v3 key rotation，老用户更新撞 `INSTALL_FAILED_UPDATE_INCOMPATIBLE` 只能卸载重装丢数据。签名私钥全程在开发者/CI 侧（不进 SwarmHive）；docs/04 + CLI publish 文档显式警告。本 change 不处理，记风险。
- [runtimeVersion fingerprint 命门（未来 OTA）] → fingerprint policy 下 runtimeVersion 是 client 本地算的 hash，server 无法预测，CLI 必须从 `expo export` 产物可靠提取并与二进制内嵌值逐字符一致，否则 OTA 永远 204。记进未来 OTA 占位 change，本 change 仅 `runtime_version?` 占位。

## Migration Plan

- `release` 表加 `android_min_version_code BIGINT NULL`，走项目既有 schema-sync（非 sea-orm-migration crate，见 backend.md）。
- 向后兼容：存量 release 该列为 NULL → 无强更下限 → `upgrade_type` 默认 `Prompt`（absent ⇒ prompt，写进响应 schema 契约）。
- 回滚：列可空、无数据回填，删列即回滚；endpoint 是新增路由，移除不影响 Tauri。

## Open Questions

- `force-required` 用户拒绝系统安装确认后的产品行为（阻断启动 / 只读 / 仅提示）—— UI/状态机层，归 registry-rn change + 产品拍板。
- staleness / 渐进强更字段（先可选 N 天再转 force，对标 Google `update-priority`）—— 是否现在给 wire 留 `Option` 字段以免 fixture 锚定后 breaking，还是按"第二个 consumer 再抽象"完全推迟。倾向推迟，记此处。
- fat APK 写入侧（CLI 如何产 `abi=None` universal 行 + 与 per-ABI 行共存匹配优先级）—— 归 CLI/发布侧，本 change 只定读取侧。
- OTA provider 形态（自实现 Expo Updates 协议 vs External Sync）—— 留给 `add-ota-expo-updates-server` 占位 change；docs/11 两候选保持开放，不在本 change 预选。
