## Context

分发链路当前只有 S3-compatible 一条腿(`storage-and-presign-upload`):`/download` 在无活跃
backend 时 409;`artifact.storage_backend_id` / `object_key` 非空;`download()` 用**全局**
`active_backend()` 解析 URL(忽略 `artifact.storage_backend_id`,隐含"活跃后端持有全部对象")。

关键既有事实(实现前已核对):

- **单一间接层**:Tauri updater(`updates.rs:479`)、RN update(`:737`)、公开目录
  (`download.rs:105`)返回给客户端的 URL **全部**指向 `/download/{app}/{version}/{artifact_id}`
  这一个 302 入口,从不直给对象 URL。→ 源选择在此收口即覆盖所有面。
- **完整性**:Tauri 验 minisign 签名(对字节)、RN 验 `sha256` + Android 安装器验 keystore。
  → 任何镜像必须与 artifact 的 `sha256` **字节一致**才安全;镜像最坏只会被客户端拒绝,
  永不"静默发错字节"给验签客户端(但公开目录的网页按钮无验签,需服务端兜)。
- **文件名重命名**:`artifact.filename` = gradle 原名 `app-release.apk`;CI 上传 GitHub 时
  重命名为 `mobile-{tag}-app-release.apk`(`release.yml:170/177`)。二者不等。
- **写入唯一落点**:artifact 只在 `complete → upsert_artifact`(`uploads/service.rs:188`)写;
  `finalize_publish` 只读计数;无 artifact PATCH 端点。`CompletePart` 已带可选 `signature`。
- **yank 不删对象**(`releases.rs:537` 只翻状态)→ 可空化不破坏删除路径(本就无删除)。
- 生产 `auto_sync=false` → artifact 列改动必须走真 migration(raw SQL,migration crate 不 import entity)。

## Goals / Non-Goals

**Goals:**

- GitHub Release 成为可**独立承担分发**的一等下载源(无 S3 也能下载 + 统计 + update-check)。
- S3 与 GitHub 并存时,GitHub 作镜像 / fallback(解决 OSS 匿名下 APK 受限)。
- 源选择对客户端契约**尽量透明**(单 302 入口),RN 侧获得真实的下载层 failover。
- 备用源健康度**可观测**(`source` 埋点),draft 窗口与资产漂移**不把用户导向 404**。

**Non-Goals:**(与 proposal 一致)Tauri 桌面 failover、私有仓库分发、区域自动路由、
通用多镜像编排、既有 S3 上传语义变更。

## Decisions

### D1. GitHub 建模为 artifact 的"外部投递位置",不是第二个 storage_backend

- **选择**:artifact 增加可空的外部 `mirror_url`;`storage_backend_id`/`object_key` 可空;
  不变量"≥1 投递位置"。GitHub 是**只读、恒公开、按 app 共存**的交付源。
- **否决 GitHub-as-storage_backend**:`storage_backend` 的 active 是**全局单例**
  (`services/storage.rs:36`,activate 事务置反其它)——激活 GitHub 会把所有 app 的 S3 关掉;
  它有一堆 S3 专属非空列;`Storage` trait 是写侧形状(`presign_put`/`put_cors` GitHub 无解)。
  这三点是硬性 mismatch。

### D2. 逐 artifact **verbatim** 记录 GitHub URL(否决模板派生 / API 查询作为主路径)

- **选择**:CI 已在 `release.yml:182` 算出确切 URL,沿 `.sig` 同通道
  (`--mirror-url` → `CompletePart.mirror_url` → `upsert_artifact` → `artifact.mirror_url` 列)
  原样记录。**结构上消灭文件名重命名 404**——服务端不猜 tag、不猜 asset 名。
- **否决 `asset_template`/`tag_template` 服务端派生为主路径**:CI 改名/改 tag 会**静默 404**,
  只有配置期 Test 能发现;verbatim 后这些字段沦为"看着承重、实则没人读"的死配置。
  (`tag_template` 仍保留在源配置里,仅供 admin Test 与未来派生兜底,非分发主路径。)
- **否决 GitHub API 实时查询为主路径**:把 GitHub API(限额 + 延迟 + token)塞进下载热路径;
  仅在 D5 的 liveness/digest 校验里按需、带缓存地用。

### D3. 新增"不上传字节"的 artifact 注册路径(支撑 GitHub-only)

- **选择**:一条 register 通道,登记元数据 + `mirror_url` + 客户端提供的 `sha256`/`size`/
  `signature`,不触发 presign/PUT/HeadObject,直接汇入 `upsert_artifact`(S3 两列为 null)。
- **否决"给 GitHub-only 造一个假 S3 object_key"**:会污染 S3 语义、且无对象可 Head 校验。
  显式区分"有无 S3 对象"更诚实,也让 D4 的解析清晰。
- **完整性**:字节在 GitHub,服务端不持有 → register 信任客户端声明的 sha256;真伪由客户端
  minisign/keystore + D5 的服务端 digest 比对兜底。

### D4. 源解析在 `/download` 302 收口 + `?source` + fallback;客户端契约不变

- **选择**:`download()` 解析候选投递位置:`?source=github`(或无可用 S3)→ `mirror_url`;
  否则活跃 backend。**仅完全无源才 409**。Tauri flat JSON(单 `url`+单 `signature`)与
  RN 单 `download_url` **都继续指向本 302 入口**——源切换发生在 302 目标处,协议不可见。
- 公开目录 `sources[]` 与 RN `mirror_urls[]` 是**并存的候选清单**,让网页渲染多按钮、
  RN 做客户端 failover;它们的 `url` 仍走 `/download?source=` 间接层以**保留埋点与 D5 gate**
  (不直给 `github.com` 直链,否则丢 `download_intent` 与 liveness 兜底)。

### D5. 服务端 liveness + digest 校验(处理 draft 窗口 + 漂移),惰性 + 缓存

- **选择**:暴露/重定向到某镜像前,校验 GitHub 资产**可匿名访问**且其 digest == `artifact.sha256`。
  惰性 on-read + **TTL 缓存 + single-flight + 负缓存**;可选 per-app token 规避未认证 60/时限额。
- **否决 monotonic HEAD-once**:资产可被替换/删除/重新 draft;一次 2xx 就永久信任会**永远
  302 进 404 或发散字节**。必须可复检 + digest 比对,才能把 D8 的隐含前提变可断言。
- **否决无缓存的每请求探测**:公开高流量目录会打爆 GitHub 限额、并可被当放大器。single-flight +
  负缓存是硬要求。
- **draft 窗口**:CI 在 finalize 之后才把 GitHub Release 转正;窗口内匿名 404 → 校验探测不到
  → **不暴露**该镜像(而非导流 404),转正后自动暴露。不依赖 CI job 顺序做正确性保证。

### D6. `download_intent` 增加 `source` 维度(真列,非仅日志)

- **选择**:落库 `source`(`oss`/`github`),并在 `event_rollup_day` 展开。否则"备用源死了"
  只能在出事时发现。挂在 `add-telemetry-events` 的既有 `download_intent` 落库上。

### D7. 每 app `github_source` 实体(仿 oauth_provider),token 仅探测用

- **选择**:`github_source(app_id FK 唯一, enabled, owner, repo, tag_template,
  access_token_encrypted?, created_at, updated_at)`。密文经 `state.secret_key` 加密,对外仅
  `token_set: bool`。owner/repo = store-time allowlist 来源。**唯一性用完整 `#[sea_orm(unique)]
  app_id`**(绝不 partial index —— sea-orm rc.38 schema-sync WHERE bug)。
- **否决 App 加列**:PAT 密文属于专用实体(entity-level secret 范式,同 oauth/mail/storage)。
- **token 不用于分发**:no-proxy 302,客户端无 token → 私有资产必 401。故私有仓库分发是 Non-goal;
  token 只在 D5 服务端探测时用。

### D8. 完整性前提"字节一致",由 D5 从隐含变可断言

MVP 靠"同一 CI 同一产物 → 同 sha256 → 同 minisign"这个事实成立;D5 的 digest 比对把它变成
**服务端可断言、可持续复检**的东西。任一验签客户端遇到不一致只会拒绝,不会被投毒。

### D9. Tauri 桌面本轮 descope(endpoints[] 对本目标是 no-op)

`plugins.updater.endpoints[]` 是 manifest 层 check-time failover 且两条指向同一台服务器:
服务器活着永远走 endpoint[0],`?source=github` 那条永不被读;服务器挂了两条一起死;它也无法
表达"S3 字节下载失败→切 GitHub"。加之桌面 CI 未写 `mirror_url`。→ 桌面维持 S3,真第二源
(独立 GitHub-Pages `latest.json` update server)另开 change。

## Risks / Trade-offs

- **[外部 CI 字节漂移:GitHub 资产被重打/重签 → 与 S3 sha256 不符]** → D5 digest 比对发现即
  不暴露该镜像;验签客户端本就 fail-closed。备用源"静默失效"由 D6 `source` 埋点 + admin Test 暴露。
- **[可空化是内部 schema breaking]** → migration 为 relax(DROP NOT NULL)+ 加列,向后兼容;
  既有纯 S3 行两列有值不受影响。**回滚不完全可逆**:一旦存在 GitHub-only 行(两列为 null),
  无法直接恢复 NOT NULL —— 记录于 Migration Plan。
- **[`updates.rs:460/717` 记录 `storage_backend_id` 的日志遇 null panic]** → 改 `?`/`Option` 显示。
- **[重传遗留 stale mirror 指向旧字节]** → `on_conflict.update_columns` 明确纳入 `mirror_url`,
  随字节更新;缺失时清空(与 `signature_metadata` 的"缺失保留"相反,因为 mirror 与字节强绑定)。
- **[RN 换源仅在 404/HTML 触发,漏掉"同仓库有效但错误的 APK"]** → 补 sha256 校验失败也触发
  换源(依赖 SDK 下载后真的算 sha256,不能只 precheck)。
- **[GitHub 未认证限额 / 被当放大器]** → D5 single-flight + 负缓存 + 可选 token;探测只在
  惰性 on-read 触发,不做无界后台风暴。
- **[链式重定向:`/download` 302 → github.com 302 → objects.githubusercontent.com 签名过期 CDN]**
  → 客户端需跟随跨域重定向(浏览器 / reqwest / expo-downloader 默认跟随);记录于验收注意。
- **[dev schema-sync 与 prod migration 双轨建 `github_source` 漂移]** → 建表 SQL 的列类型 / FK /
  唯一约束名必须与 entity 派生一致;dev 下若 schema-sync 与 migration 同建需环境门控避免 "already exists"。

## Migration Plan

1. **migration(swarmhive-migration,raw SQL)**:
   - `ALTER TABLE artifact ADD COLUMN mirror_url text`(可空)。
   - `ALTER TABLE artifact ALTER COLUMN storage_backend_id DROP NOT NULL`;`object_key` 同。
   - `CREATE TABLE github_source(...)` + `app_id` 唯一约束(与 entity `#[sea_orm(unique)]` 对齐)。
   - 注册进 `migration/src/lib.rs` `migrations()`。
2. **部署顺序**:先发能容忍 `mirror_url = null` / 可空两列的 **server**(读侧向后兼容,既有行为不变);
   再发带 `--mirror-url` 的 **CLI** 与更新后的 **swarmhive-action**;最后接 SDK/registry 与 admin。
3. **回滚**:server 回滚即忽略新列/新表,既有 S3 分发不受影响(GitHub 源只是不再被解析)。
   **不可逆点**:若已产生 GitHub-only 行,`object_key`/`storage_backend_id` 无法恢复 NOT NULL——
   回滚需先清理/迁移这些行,或保留列可空。

## Open Questions

- **缺省源顺序**:S3 优先 + GitHub fallback(推荐);GitHub-only 时自动 GitHub。是否允许 per-app
  配置"默认源"(如国内部署想默认 GitHub-以外)?—— 倾向 MVP 固定 S3 优先,配置项留后。
- **liveness 校验落点**:惰性 on-read + 缓存(MVP 推荐)vs 后台周期 reconciler(可选增强)。
- **swarmhive-action input 形态**:单 `--mirror-url` vs per-file map(多 ABI split 时);MVP 单 ABI,
  先支持"每次 publish 一个 artifact 一个 mirror-url",多 artifact 走多次 publish(与现流一致)。
- **register 端点鉴权**:复用 `ArtifactUpload` 权限即可(与 complete 同级)。
