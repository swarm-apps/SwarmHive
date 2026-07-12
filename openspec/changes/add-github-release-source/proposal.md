# add-github-release-source

> 目标能力:让 **GitHub Release 成为一等下载源**,与 S3-compatible 存储平级共存,
> 甚至在完全没有配置 S3 后端时也能独立承担分发——SwarmHive 依然提供控制面价值
> (下载统计、update-check、channel、release notes、灰度)。核心动机是**提高系统
> 可用性、降低接入门槛**,并顺带解决"阿里云 OSS 匿名下载 APK 受限"这类下载失败场景。

## Why

现状:分发链路**只有一条腿**——S3-compatible 对象存储(`storage-and-presign-upload`)。
`/download` 路由在没有活跃 backend 时直接返回 409;artifact 的 `storage_backend_id` /
`object_key` 都是非空列;没有活跃 S3 就无法上传、无法分发,SwarmHive 的一切控制面价值
(统计 / 更新检查 / channel)也随之不可用。

两个真实痛点:

1. **可用性 / 接入门槛**:用户必须先搭好一套 S3-compatible 存储(RustFS / OSS / R2…)
   才能用 SwarmHive。但很多 swarm-apps 应用的 CI **本来就把同一批产物发到了 GitHub
   Release**(`SwarmDrop-RN/.github/workflows/release.yml` 同一 run 同时发 SwarmHive 和
   GitHub Release)。这些不可变、免费、带 CDN 的公开 URL 完全可以直接当下载源——
   **不搭 S3 也能用 SwarmHive 做统计和更新分发**。
2. **下载可靠性**:阿里云 OSS 默认 `*.aliyuncs.com` 域名对 `.apk` 等类型有强制/受限行为,
   RN 应用内更新的下载器有时拿到的是 OSS 错误页而非 APK(`expo-downloader` 的
   `assertApkDownload` 正是为此区分)。需要一条**备用下载源**在主源失败时接管。

docs/07(镜像策略)早已把 "GitHub Releases 作为 fallback" 与 "后续:多存储后端 / 区域路由"
列为规划方向;README 也写明 "GitHub Releases 降级为构建产物备份源或 fallback 下载源"。
本 change 把这一规划落地为一等的、可独立承担分发的下载源。

## What Changes

### 1. 投递位置模型:artifact 从"绑定单一 S3 对象"→"一到多个投递位置"

- **BREAKING(schema,内部)**:`artifact.storage_backend_id` 与 `artifact.object_key`
  改为**可空**。新增 `artifact.mirror_url: Option<String>`(外部下载 URL,当前即 GitHub
  Release 资产 URL,**逐 artifact verbatim 记录**)。
- **不变量**:每个 artifact 至少有一个可用投递位置——S3 对象(两列都有)**或/和** 外部
  `mirror_url`。二者可并存(镜像 / fallback),也可只有其一(GitHub-only,无 S3)。
- 控制面(update-check / channel / release notes / 灰度 / 埋点)本就与源无关,继续照跑。

### 2. GitHub 资产 URL 逐 artifact verbatim 记录(复用 `.sig` 同一通道)

CI 早已在 `release.yml:182` 算出确切 URL(资产被 CI 重命名为 `mobile-{tag}-{base}.apk`,
**与 `artifact.filename` 不相等**,所以服务端拼模板会 404;verbatim 记录结构上消灭此问题):

- `CompletePart` 新增可选 `mirror_url`(`Option` + `#[serde(default)]`,线格式前后兼容),
  与既有 `signature` 字段同源同通道。
- 写入唯一落点 `upsert_artifact` 落 `mirror_url`;`on_conflict.update_columns` 同步纳入,
  **重传语义**:随字节更新或缺失时清空(不残留指向旧字节的死链)。
- **store-time 校验**:记录时即校验 `mirror_url` 属于 allowlist 主机(`github.com`)+ 该 app
  配置的仓库,拒绝异源 URL(公开目录那个无校验镜像按钮靠这层兜)。

### 3. 无需 S3 的 artifact 注册路径(支撑 GitHub-only)

现有 `presign → PUT → complete` 依赖活跃 S3 后端。新增一条**不上传字节**的注册通道:
客户端(CLI/CI)直接登记 artifact 元数据 + `mirror_url` + 客户端提供的 `sha256` / `size` /
`signature`(字节存在 GitHub,完整性仍由客户端 minisign/sha256/keystore 兜底)。两条写路径
最终都汇入同一个 `upsert_artifact`。

### 4. 下载源解析:`/download` 不再强依赖活跃 S3 后端

- `/download/{app}/{version}/{artifact_id}` 加可选 `?source=oss|github`(缺省 = 既有行为)。
  解析:`github` / 无可用 S3 → 用 `mirror_url` 302(GitHub 恒公开,跳过 `url_mode`);
  否则走活跃 backend。**仅在完全无可用源时才 409**。
- `download_intent` 埋点新增 **`source` 维度**(落库,非仅日志)→ 备用源健康度可观测,
  避免"备份源在你出事时才发现是死的"。

### 5. 服务端 liveness + digest 校验(处理 draft 窗口与资产漂移)

暴露 / 使用某个 GitHub 镜像前,后台/惰性地比对 GitHub 资产 digest 与 `artifact.sha256`,
带 **TTL + single-flight + 负缓存**(挡未认证 60/时限额与放大攻击)。它同时解决:

- **draft 窗口**:GitHub Release 在 CI 后段才从 draft 转正,匿名访问期间 404 → 校验探测不到
  就"不暴露该镜像",而非把用户 302 进 GitHub 404。
- **资产漂移**:被替换 / 删除 / 重新 draft → digest 不符或探测失败 → 停止提供该镜像。

### 6. 每 app 的 GitHub 源配置

新增 per-app 配置(仿 `oauth_provider` 运行时配置范式):`enabled`、`owner`、`repo`、
`tag_template`(默认 `v{version}`)、可选 `access_token_encrypted`(**仅用于**服务端 liveness/
digest 探测与规避未认证限额;**不用于**分发——no-proxy 302 模型下客户端无 token,私有仓库
资产无法投递,故私有仓库分发不在范围)。密文经 `state.secret_key` 加密,对外只暴露
`token_set: bool`。owner/repo 即 store-time allowlist 的来源。

### 7. 多源展示与 RN 下载 failover(契约 + 实现)

- 公开目录 `DownloadCatalog.artifacts[]` 每项新增 `sources: [{kind, url}]`(S3 主 + GitHub 镜像),
  网页/registry 下载组件据此渲染 "OSS(国内)" + "GitHub(海外/备用)" 两个入口。
- `AndroidUpdateResponse` 新增 `mirror_urls: Vec<String>`;`packages/sdk` 的 `normalizeAndroid`
  透传;`registry-rn` 的 `rn-adapter` 主源失败按序切换镜像。**换源触发点**:`assertApkDownload`
  已能区分真 APK 与错误页;并补上 sha256 校验失败也触发换源(而非 hard abort)。

### 8. Admin

`releases-page-ui` 的 artifact 表新增只读 **source badge**(S3 / GitHub);app 详情下新增
精简的 **GitHub 源配置**表单(owner/repo/token/enabled + 一个 "Test" 动作:dry-render tag +
对最新 release HEAD/digest 探测,配置期即暴露 404/漂移)。

## Capabilities

### New Capabilities

- `github-release-source`:GitHub Release 作为一等下载源的完整行为契约——per-app 源配置、
  逐 artifact verbatim `mirror_url` 记录与 store-time 校验、无 S3 的 artifact 注册路径、
  `/download` 的多源解析(镜像 / 独立 / fallback,不强依赖活跃 backend)、liveness+digest
  校验(draft 窗口 + 漂移)、`download_intent` 的 `source` 维度、公开目录 `sources[]` 与
  `AndroidUpdateResponse.mirror_urls[]` 契约、RN 下载 failover。

### Modified Capabilities

- `app-release-artifact`:artifact 投递位置模型从"单一 S3 对象"扩为"≥1 投递位置";
  `storage_backend_id`/`object_key` 可空 + 新增 `mirror_url`;`api::Artifact` 相应可空。
- `storage-and-presign-upload`:`/download` 解析增加源选择、不再强依赖活跃 backend
  (仅无源时 409);新增不上传字节的 artifact 注册端点;`download_intent` 加 `source`。
- `update-check-rn-android`:`AndroidUpdateResponse` 新增 `mirror_urls[]`(响应主流程不变,
  仅多一个可选数组字段)。
- `update-sdk-core`:RN 适配器 `download()` 支持按序多源 failover(主源失败 / sha256 不符
  时切换),`ReleaseInfo` 携带镜像候选。

## Impact

- **Code(server)**:entity `artifact`(3 列改动)+ 新 `github_source` 表;`api-types`
  `artifact`/`upload`/`download`/`update` DTO 扩展;`routes/download.rs` 源解析、
  `routes/uploads*`(注册路径)、新 `routes/github_source.rs`(CRUD)、`services/` 新增
  mirror-liveness 校验;`routes/updates.rs`(android 出 `mirror_urls`);`services/telemetry`
  的 `source` 维度。
- **Code(CLI/CI)**:`swarmhive-cli` 新 `--mirror-url` / 注册子命令;外部
  `swarm-apps/swarmhive-action`(独立仓库)加 input 转发 `release.yml:182` 的 URL。
- **Code(SDK/registry/admin)**:`packages/sdk` `normalizeAndroid` + `ReleaseInfo`;
  `registry-rn` 的 `rn-adapter`/`expo-downloader` 多源;admin source badge + 源配置表单。
- **DB**:`artifact` 三列(可空化 + 新增,生产 `auto_sync=false` → 真 migration);
  新表 `github_source`;`event_rollup_day` 的 `source` 维度沿用既有列或新增。
- **API**:新增 GitHub 源 CRUD + artifact 注册端点;`/download` 加 `?source`;
  catalog/update 响应扩字段 → OpenAPI drift gate 触发,重生成 `schema.gen.ts`。
- **Deps**:server 需一个 GitHub REST 轻客户端(可用既有 `reqwest`);无重量级新依赖。
- **配置**:GitHub 源为 per-app DB 配置,非全局 config 段。

## Non-goals

- **Tauri 桌面 failover 不在本轮**。`plugins.updater.endpoints[]` 是 manifest 层 check-time
  failover 且指向同一台服务器,对"S3 字节下载失败→切 GitHub"是 no-op;桌面真第二源需要
  独立的 GitHub-Pages 静态 `latest.json` update server,另开 change。桌面本轮维持 S3。
- **私有仓库分发**:no-proxy 302 模型下客户端无 token,私有资产无法投递;PAT 仅服务端探测用。
- **区域自动路由**(按 IP/Geo 默认 OSS/GitHub):留待后续;本轮源选择靠显式 `?source` + fallback。
- **通用外部镜像 / 多镜像编排**:数据模型对"外部 URL"通用,但本轮只实现 GitHub Release 一种源,
  且每 app 单一 GitHub 源(不做多镜像加权 / 测速)。
- **既有 S3 上传链路语义变更**:presign→PUT→complete 行为不变,仅可选携带 `mirror_url`。

## Depends on

- `storage-and-presign-upload`(archived)—— download 路由 / artifact 写入唯一落点 / storage 抽象。
- `app-release-artifact`(archived)—— artifact 实体与唯一约束。
- `update-check-rn-android` / `update-sdk-core`(archived)—— RN 更新响应与 SDK 适配器。
- `add-telemetry-events` —— `download_intent` 落库与 rollup(`source` 维度挂在其上)。

## Maps to docs

- [docs/07-storage-and-delivery.md](../../../docs/07-storage-and-delivery.md) 「镜像策略」「下载入口」「文件路径规范」(落地后按实情修订)。
- [docs/05-ecosystem.md](../../../docs/05-ecosystem.md) / README「GitHub Releases 作为 fallback / 备份源」。
- [docs/10-telemetry.md](../../../docs/10-telemetry.md) `download_intent` 的 `source` 维度。
- openspec `add-ota-provider`(proposal)—— 不可变资产 URL 与 fallback 动机。

## Acceptance

- **GitHub-only(无任何活跃 S3 后端)**:注册一个仅带 `mirror_url` 的 artifact →
  `/download/{app}/{version}/{id}` 返回 302 到 GitHub URL(不 409);`download_intent`
  落库且 `source=github`;update-check(RN)对该 release 返回 `has_update` 且 `download_url`
  可用、`mirror_urls` 含 GitHub URL。
- **镜像 / fallback(S3 + GitHub 并存)**:`?source=github` 302 到镜像、`?source=oss`(缺省)
  302 到 S3;两条都记 `download_intent` 且 `source` 维度正确。
- **verbatim / 文件名重命名**:CI 传入被重命名的 `mobile-v{ver}-*.apk` URL → 原样落库、
  原样 302(不因 `artifact.filename` 不同而 404)。
- **store-time 校验**:注册一个非 `github.com` 或非本 app 仓库的 `mirror_url` → 被拒(4xx),
  公开目录不出现该镜像。
- **liveness / draft 窗口**:`mirror_url` 指向尚为 draft(匿名 404)的资产 → 目录/更新响应
  **不暴露**该镜像;转正后暴露。digest 与 `artifact.sha256` 不符 → 不暴露。
- **重传去 stale**:同 identity 重传新字节但不带 `mirror_url` → 旧 `mirror_url` 被清空。
- **RN failover**:主源(OSS)下载返回错误页 / sha256 不符 → 适配器切到 `mirror_urls[0]`
  完成安装;两源皆失败才报错。
- **可空化不回归**:既有纯 S3 artifact(两列有值、无 mirror)下载、update-check、yank 全部
  行为不变;`updates.rs` 对可空 `storage_backend_id` 的日志不 panic。
- **门禁**:`cargo test --workspace` / clippy / `pnpm lint` / typecheck / OpenAPI drift gate 全绿。
