# TODO

> 追踪已知但本轮未做的事项。完成后从这里删掉。

## GitHub Release 下载源(`add-github-release-source`,已发版 v0.7.0 的后续)

已归档 change:`openspec/changes/archive/2026-07-12-add-github-release-source/`。以下是当时刻意
descope / 推迟的项。

### Phase 3(设计里已列为"后续")
- [ ] **Tauri 桌面第二源**:本轮桌面 descope,因为 `plugins.updater.endpoints[]` 是 manifest 层
      check-time failover 且指向同一台服务器,对"S3 字节下载失败→切 GitHub"是 no-op。真方案 =
      独立的 GitHub-Pages 静态 `latest.json` update server(作为 endpoints[1],SwarmHive/S3 整体
      宕机时仍活)。另开 change。
- [ ] **区域自动路由**:按 IP/Geo 让国内默认走 OSS、海外默认 GitHub(现状:只有显式 `?source=` +
      自动 fallback)。
- [ ] **通用外部镜像 / 多镜像编排**:数据模型对"外部 URL"通用,但目前只实现 GitHub Release 一种源、
      每 app 单一 GitHub 源(不做多镜像加权 / 测速)。

### 服务端加固
- [ ] **liveness 探测用 per-app token**:`services/mirror.rs` 目前匿名探测 GitHub REST(未认证
      60/hr/IP 限额),`github_source.access_token_encrypted` 已建模但 probe 暂未使用它。高流量目录
      场景需接上 token 提限额(见 mirror.rs 顶部 doc)。
- [ ] **无 digest 时的 size-only 兜底**(xhigh 评审 finding #11,PLAUSIBLE):GitHub 资产无 `digest`
      字段时,`probe()` 退化为 size 匹配 → 理论上同尺寸不同字节的资产会被判 live(公开目录那个无验签
      按钮有风险)。GitHub digest 现已普遍存在,暂列为可接受风险;可改为无 digest 即判 not-live。
- [ ] **`download_intent.source` 汇总进 rollup**:已加 `update_event.source` 原始列(可查),但未展开进
      `event_rollup_day`(那里的 `source` 是 server/client 事件源,语义不同,未复用)。备用源健康度目前
      只能查原始表,规模化时可加独立 rollup 维度。
- [ ] **mirror liveness slots 缓存**:`MirrorCache` 超 50k 条时整体 clear(粗暴兜底),可换 LRU。

### 客户端 CI/CD
- [ ] **SwarmDrop 桌面 CI 接 `github-mirror-url`**:本轮未接。原因:桌面给 SwarmHive 发布的只有 Tauri
      updater bundle(action 排除 `.dmg/.msi/.deb`),而镜像消费方是网页目录(只展示 Installer|Universal)、
      RN、以及本轮 descope 的 Tauri updater —— 落到 Updater artifact 上**无消费方**;加之 action 每
      target 可能发多产物会撞 `--mirror-url` 单产物约束。需等 Phase 3 桌面第二源,或桌面 CI 也把安装包
      (Installer/Universal)发到 SwarmHive(网页目录才有可挂镜像的对象)后再接。
      (SwarmDrop-RN 移动端已接:`swarm-apps/SwarmDrop-RN` PR #2 合并。)
