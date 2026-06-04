# add-ota-provider

> **状态**：future 占位（**Phase 2，不在 MVP，暂不 `/opsx:apply`**）。作用：锚定 OTA 节点 + 记录"轻接缝"约束，让 native 链路设计不无意堵死 OTA。**provider 形态不预选**。

## Why

OTA（JS bundle 热更）是 SwarmHive 的 **provider 扩展层**（docs/11，[[project-ota-strategy]]），MVP 不实现。`expo-updates`（OTA）与 native APK 链路正交不重叠：前者只换 JS/assets、被 `runtimeVersion` 锁死在"原生不变"、结构性绝不装 APK；后者在 `runtimeVersion`/fingerprint 变时发新 APK。

设此占位是为了：MVP 的 native 链路（`add-update-check-rn-android` / `add-sdk-android-check` / `add-registry-rn`）在数据模型/类型/端点上**给 OTA 留干净接缝**，避免 Phase 2 接 OTA 时破坏性大改。

## What Changes（Phase 2，方向，不展开 tasks）

**provider 形态保持开放，不预选**（docs/11 明文「避免把 SwarmHive 定位成 Expo Updates 竞争者」）：

- 候选 A：**自托管 Expo Updates v1 协议服务器**（闭环最全；但 Rust 端无成熟上游可复用、需自负 multipart/SFV + RSA manifest 签名 + directive + protocol-version 漂移维护，MEDIUM+ 长尾）。
- 候选 B：**External OTA Sync Provider**（docs/11 钦定倾向；SwarmHive 不承载协议，只同步 metadata 到外部 Expo OTA server，统一 Admin/CI/storage/analytics；协议风险外推上游）。
- 真做时二选一并相应更新 docs/11。

## 现在就要守住的接缝约束（MVP 各 change 落地时遵守）

- `runtime_version` 是 OTA 兼容键（**≠** native 的 `versionCode` 整数闸门），两套独立列不复用；MVP 仅在 `release.rs` doc 注释记约束，OTA 真做时才建可查询列。
- OTA 走**独立端点** `GET /api/v1/updates/ota/:app_slug`（不塞进 `/updates/android`）；`ReleaseInfo.kind` 区分 native-package / ota-bundle。
- signature gating 锚 **kind 级**（native-package 靠 Android 安装器兜底不 gate；ota-bundle 绕过 PackageManager，**必须**应用层验真）——不要写成 platform 级"RN 不 gate"。
- telemetry 的 OTA `kind` 走现有 `platform` 列 + `metadata_jsonb`，**不加专列**。
- **runtimeVersion fingerprint 命门**：fingerprint policy 下 runtimeVersion 是 client 本地算的 hash，server 无法预测；CLI 必须从 `expo export` 产物可靠提取、与二进制内嵌值逐字符一致，否则 OTA 永远 204。
- immutable asset URL：Expo 协议要求 published asset URL 不可变，presign 会过期 → 需稳定 `/assets/:key` 代理或 public-read object key（storage-delivery 决策，真做时定）。
- code signing 可 defer（镜像 native 的 deferred-minisign 决策）。

## Capabilities

### New Capabilities（Phase 2）
- `ota-provider`（占位）：OTA bundle 发布/分发的 provider 扩展层；形态、协议、数据模型待 Phase 2 拍板。

## Non-goals（MVP）

- **MVP 不实现本 change**（仅占位 + 约束记录）。
- 不预选 provider 形态（A 自实现 / B 外部同步保持开放）。
- 不在 MVP 建 OTA 数据列 / OTA 端点 / OTA 客户端。

## Depends on

- `add-storage-and-presign-upload`（asset 托管）。
- 关联 `add-registry-rn`（RN 线）；复用 channel/release/rollout 抽象。

## Maps to docs

- [docs/11-ota-providers.md](../../../docs/11-ota-providers.md) OTA provider 战略（两候选）。
- [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) SDK 接缝。
