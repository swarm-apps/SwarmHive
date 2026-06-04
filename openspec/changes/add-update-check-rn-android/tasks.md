## 1. 数据模型 / migration

- [ ] 1.1 [code] `crates/swarmhive-entity/src/release.rs` 加 `android_min_version_code: Option<i64>` 列（确认 `android_version_code` 已存在、复用）；走项目 schema-sync（非 migration crate）。doc 注释写 OTA 接缝约束：OTA 下发靠 `runtime_version`/fingerprint 精确匹配，不复用 `android_version_code`；Phase 2 OTA 另立兼容键，不在此建列
- [ ] 1.2 [test] 启动 server 跑 schema-sync，确认 `release` 表新增列、存量行该列为 NULL（向后兼容：无下限 → prompt）

## 2. api-types DTO

- [ ] 2.1 [code] `crates/swarmhive-api-types/src/update.rs` 加 `AndroidUpdateQuery`（`current_version_code: i64`、`current_version_name: String`、`channel/abi/client_id/runtime_version: Option<String>`）+ `AndroidUpdateResponse`（扁平：`has_update: bool` + 有更新时 `version_name/version_code/upgrade_type/min_version_code?/download_url/release_notes?/size_bytes/sha256`），serde + `utoipa::ToSchema`
- [ ] 2.2 [code] 确认 `UpgradeType`（现 Prompt|Force）够用；`min_version_code` 与 `release_notes` 为 Option（absent ⇒ prompt 写进 schema 契约语义）

## 3. server handler

- [ ] 3.1 [code] `routes/updates.rs` 新增 `android` handler 并 `router()` 改 `routes!(tauri, android)`；复用 `find_app_by_slug` / channel→pointer→`Published` release / `in_rollout_bucket` / `forwarded_ip` / `download_url` / 两段 telemetry helper
- [ ] 3.2 [code] 版本闸门：i64 整数比较 `current_version_code < android_version_code`（**绝不**抄 Tauri 的 `strip_v`+`semver::parse`）；`android_version_code` 为 None 的 release 跳过；不可解析 `current_version_code` → 400 结构化错误
- [ ] 3.3 [code] `match_rn_artifact(&artifacts, abi)`：filter `Platform::ReactNativeAndroid` → 精确 abi → fat APK(`abi=None`) → 单 untargeted fallback → 允许跨 ABI 降级；**不做** signature gating。注释写明"native-package 不 gate 因 Android 安装器兜底；未来 ota-bundle kind 另需应用层验签"（规则锚 kind 级非 platform 级）
- [ ] 3.4 [code] `upgrade_type`：`android_min_version_code > current_version_code → Force` else `Prompt`；组装 `AndroidUpdateResponse`，`has_update:false` 时绝不含 `download_url`
- [ ] 3.5 [code] 灰度分桶 key 三级回退 `client_id`(query) → XFF IP → 命中+warn（与 Tauri 同语义）；`runtime_version` query 接收但不消费（占位）
- [ ] 3.6 [code] telemetry：`update_check`（每次）/ `update_available`（有更新时），`platform="react-native-android"`，字段对齐 `update_event`

## 4. 集成测试 + fixture（对齐 acceptance）

- [ ] 4.1 [test] `tests/update_check_rn_android_smoke.rs`：发 Android release + APK → 调 endpoint 返完整 schema → `/download` 302 到 S3
- [ ] 4.2 [test] 整数闸门：`current < android_version_code` → 有更新；`current >= ` → `has_update:false` 无 download_url；`android_version_code=None` 跳过
- [ ] 4.3 [test] 强更：`current < android_min_version_code` → `upgrade_type=force`；`android_min_version_code=None` → prompt（含 kill-switch：调高 min 后低版本客户端转 force）
- [ ] 4.4 [test] ABI：精确命中 / fat APK(`abi=None`) 兼容所有 / 跨 ABI 降级（arm64 拿 v7a）/ 无匹配 → `has_update:false`
- [ ] 4.5 [test] 不可解析 `current_version_code`（`"abc"`）→ 400；灰度 `client_id` 分桶命中/未命中与 Tauri 对同一 client_id 一致
- [ ] 4.6 [test] 落 `fixtures/android_update_response.json`（has_update true/false 两版）给后续 RN SDK contract test 锚定

## 5. gates 与文档

- [ ] 5.1 [test] `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` 全绿；`cargo tree -p swarmhive-cli` 边界守护不回归
- [ ] 5.2 [test] `pnpm --filter @swarm-hive/admin openapi` 重生成 `schema.gen.ts` 无意外 drift（新增 Android DTO 进 OpenAPI）；`openapi_surface` 测试加 android endpoint
- [ ] 5.3 [docs] docs/04-platform-support.md RN Android 段补：APK-not-AAB 引导（`eas build`/`gradle assembleRelease`）、per-ABI split versionCode offset 提示、keystore 漂移警告（私钥在开发者侧、换 keystore 撞 `INSTALL_FAILED_UPDATE_INCOMPATIBLE`）
- [ ] 5.4 [docs] 代码注释用中文（对齐 CLAUDE.md 约定：update-check-rn-android feature 注释即用中文）
