## ADDED Requirements

### Requirement: Android 更新检查 endpoint

系统 SHALL 提供 `GET /api/v1/updates/android/:app_slug`，接收客户端自报的 `current_version_code`(i64)、`current_version_name`、可选 `channel`/`abi`/`client_id`/`runtime_version`，按 app→channel→已发布 release→匹配 artifact 的链路返回扁平 JSON。响应统一 HTTP 200（不使用 Tauri 的 204 absence 语义），用 `has_update` boolean 区分有无更新。

#### Scenario: 有可用更新
- **WHEN** 客户端 `current_version_code` 低于默认（或指定）channel 当前 release 的 `android_version_code`，且存在匹配 ABI 的 APK artifact
- **THEN** 返回 200 + `{has_update:true, version_name, version_code, upgrade_type, min_version_code?, download_url, release_notes?, size_bytes, sha256}`

#### Scenario: 已是最新
- **WHEN** 客户端 `current_version_code` 不低于当前 release 的 `android_version_code`
- **THEN** 返回 200 + `{has_update:false}`，且 MUST NOT 包含 `download_url`

#### Scenario: channel 显式不存在
- **WHEN** 请求带 `channel=` 指向一个该 app 不存在的 channel
- **THEN** 返回 404

#### Scenario: 无已发布 release
- **WHEN** 目标 channel 没有指向任何 `Published` 状态的 release
- **THEN** 返回 200 + `{has_update:false}`（不返 download_url）

### Requirement: versionCode 整数版本闸门

系统 SHALL 用 `versionCode` 整数比较（绝不用 semver 解析）判定是否有更新与是否强制。`android_version_code` 为 NULL 的 release SHALL 被视为非 RN release 跳过。`current_version_code` 解析失败 SHALL 返回 400。

#### Scenario: 整数比较判更新
- **WHEN** `current_version_code = 18`，release `android_version_code = 21`
- **THEN** 判定有更新（21 > 18），不经任何 semver 解析

#### Scenario: 不可解析的 version code
- **WHEN** `current_version_code` 不是合法整数（如 `"18.5"` / `"abc"`）
- **THEN** 返回 400 结构化错误，而非静默当作无更新

#### Scenario: 非 RN release 跳过
- **WHEN** 当前 release 的 `android_version_code` 为 NULL（如纯 Tauri release）
- **THEN** 该 release 不参与 RN 更新判定，返回 `{has_update:false}`

### Requirement: 基于 android_min_version_code 的强制更新

系统 SHALL 在 `release.android_min_version_code > current_version_code` 时把 `upgrade_type` 计为 `force`，否则 `prompt`。该下限 SHALL 是独立于 semver `min_version`（Tauri 用）的整数列。运维调高 `android_min_version_code` SHALL 能 retroactively 强更所有低于它的客户端（kill switch），无需发新 APK。

#### Scenario: 触发强制更新
- **WHEN** `current_version_code = 15`，release `android_min_version_code = 18`
- **THEN** 响应 `upgrade_type = "force"`，`min_version_code = 18`

#### Scenario: 无下限默认 prompt
- **WHEN** release 的 `android_min_version_code` 为 NULL
- **THEN** 响应 `upgrade_type = "prompt"`，且不含（或 null）`min_version_code`

### Requirement: ABI artifact 匹配

系统 SHALL 按 `platform == ReactNativeAndroid` 过滤 artifact，再按「精确 `abi` → fat APK(`abi=NULL`，兼容所有) → 单 untargeted fallback」选取，并 SHALL 允许跨 ABI 向下降级匹配（如 arm64 设备可接受 armeabi-v7a）。系统 SHALL NOT 对 RN artifact 做签名 gating（APK 真伪由 Android 安装器在安装时验签兜底）。

#### Scenario: 精确 ABI 命中
- **WHEN** 客户端 `abi=arm64-v8a`，release 有 `arm64-v8a` 的 APK
- **THEN** 返回该 APK 的 download_url

#### Scenario: fat APK 兜底
- **WHEN** 客户端 `abi=x86_64`，release 只有一个 `abi=NULL` 的 fat APK
- **THEN** 返回该 fat APK（视为兼容所有 ABI）

#### Scenario: 跨 ABI 降级
- **WHEN** 客户端 `abi=arm64-v8a`，release 只有 `armeabi-v7a` 的 APK
- **THEN** 返回 `armeabi-v7a` APK（arm64 设备向下兼容），而非 `has_update:false`

### Requirement: 复用灰度分桶

系统 SHALL 复用既有 `in_rollout_bucket`（blake3）做灰度分桶，分桶 key 取 `client_id`（query）→ `X-Forwarded-For` IP → 命中并 warn 三级回退，与 Tauri endpoint 同算法同语义。

#### Scenario: client_id 分桶
- **WHEN** release `rollout_percent = 50`，请求带 `client_id`
- **THEN** 按 `in_rollout_bucket(client_id, 50)` 命中则返更新、未命中返 `{has_update:false}`，结果与 Tauri 端点对同一 client_id 一致

### Requirement: 轻 OTA 接缝（前向兼容占位）

系统 SHALL 接受但不消费 `runtime_version` query（MVP 占位），不因其存在或缺失改变 native APK 判定。本 endpoint SHALL 只服务 native-package，OTA 留给未来独立 endpoint。

#### Scenario: runtime_version 占位不影响判定
- **WHEN** 请求带或不带 `runtime_version` query
- **THEN** native APK 更新判定结果不受其影响（MVP 不消费该参数）

### Requirement: 更新检查埋点

系统 SHALL 在每次检查时经 `tracing target:"telemetry"` 落 `update_check`，在判定有更新时落 `update_available`，字段对齐 `update_event`（含 `platform="react-native-android"`、匿名 `client_id`、app/release/artifact 标识），不新增 OTA 专列。

#### Scenario: 落检查埋点
- **WHEN** 任意一次 `/api/v1/updates/android/:app_slug` 请求
- **THEN** 落一条 `update_check` telemetry，`platform="react-native-android"`
