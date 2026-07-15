# add-download-source-preference

> Per-app、per-platform 的下载源顺序配置 —— 把 `add-github-release-source` 里写死的
> `[oss, github]` 缺省顺序变成可配置项,兑现该 change 的 design.md D 段明确推迟的决策。

## Why

**生产事故**:阿里云 OSS 对匿名 `.apk` 下载返回 XML 错误页(而非 APK 字节)。SwarmDrop-RN
的更新链路因此**必然失败**:server 302 到 OSS → 客户端拿到 XML → 下载器识别出非 APK 抛错。

`add-github-release-source` 已经为此建好了 GitHub 镜像 + 客户端 failover,但它没能修好这个
问题,原因有两层:

1. **默认顺序写死**。`routes/download.rs` 的 `resolve_and_redirect` 里:

   ```rust
   let order = match source {
       Some(Github) => [Github, Oss],
       _ => [Oss, Github],   // ← 缺省永远 OSS 优先,无法配置
   };
   ```

   该 change 的 `design.md` D 段原话:「**缺省源顺序**:S3 优先 + GitHub fallback(推荐);
   GitHub-only 时自动 GitHub。是否允许 per-app 配置"默认源"(如国内部署想默认 GitHub-以外)?
   —— 倾向 MVP 固定 S3 优先,配置项留后。」**本 change 就是来收这个"留后"的。**

2. **抽象用错了**。client-side failover 是为**偶发失败**设计的(网络抖动、CDN 单点故障)。
   而「阿里云 OSS × APK」是**结构性不可用** —— 不是有时候失败,是永远失败。用 fallback 扛
   结构性失败,代价是每次更新都要先完整下载一遍错误页、失败、再重下,且把一个可预知的
   路由决策推迟到了最慢的地方(用户设备上、下载失败之后)。

**关键红利:本 change 不需要客户端配合**。存量 SwarmDrop-RN 装的是 `@swarm-hive/sdk@0.1.0`,
其 `rn-adapter` 是 failover 之前的旧副本(单源直下、无 `mirrorUrls`),**拿不到任何 failover**。
但它只跟 `/download/...` 的 302 走 —— 后端把该 app 的 android 缺省源翻成 GitHub,存量客户端
**零改动、不发版**立刻恢复。这正是 `add-github-release-source` design.md D4 立的设计资产
(「源切换发生在 302 目标处,协议不可见」),本 change 来兑现它。

客户端侧的加固(下载器校验回流 + failover 真正进 app)是**独立**的 `harden-rn-apk-downloader`,
两者无依赖、可并行上线;本 change 单独就能修好生产。

## What Changes

### 1. Schema:`github_source` 承载 per-platform 偏好

新增一列 `prefer_for_platforms`(jsonb,platform 字符串数组,缺省 `[]`)。语义:
**该 app 的这些 platform,GitHub 源优先于 OSS**。

选 `github_source` 作宿主而非新表,是因为「源顺序」只在 GitHub 源**存在**时才有意义
(没有 mirror 就只有 OSS 一个候选,顺序无从谈起)。行不存在 = 无偏好 = 现状。

粒度定在 **platform** 而非 app,因为事实就是 per-platform 的:OSS 卡的只有 `.apk`,
桌面 `.dmg`/`.exe` 在 OSS 上完全正常、且对国内用户比 GitHub 快得多。per-app 一刀切会把
桌面产物一起推去 GitHub,是净损失。

### 2. 源解析:显式 `?source` > 配置偏好 > 缺省 `[oss, github]`

`resolve_and_redirect` 的 `order` 从写死改为读配置。**fallback 循环语义完全不变** ——
仍是「按序取第一个可用」,mirror 没过 liveness/digest 校验就自动落回 OSS。显式 `?source=`
仍然最高优先级(既有契约不破)。

**安全性由构造保证**:`enabled=false` 的源不参与投递(`mirror::source_enabled` 已挡在
liveness 里),所以「配了 github 优先 + 源被禁用」会自动落回 OSS,不会变成死链。

### 3. `mirror_urls` 语义收敛:「GitHub 候选」→「主源之外的其余候选,按序」

现状 `mirror_urls` 只塞 GitHub。一旦缺省源可能是 GitHub,这个语义就自相矛盾了
(主源已是 GitHub,再把 GitHub 列为"镜像"会让客户端把同一个源试两遍)。

收敛为对称语义:`download_url` 保持裸入口(302 按偏好解析),`mirror_urls` = **其余**
可用源的显式 `?source=` 入口,按 fallback 顺序排列。于是客户端候选链恒为
`[偏好源, 其余源...]`。对既有 SDK 0.3.0 客户端**向后兼容** —— 它本就把 `mirror_urls`
当"按序 fallback 候选"消费,不关心里面是哪个源。

### 4. Catalog `sources` 按偏好排序

`download_catalog` 现在硬编码先 push oss 再 push github。改为按该 platform 的偏好排序,
让下载页把推荐源呈现在首位。

### 5. api-types / admin UI / CLI

- `GithubSourceView` + `CreateGithubSourceRequest` + `UpdateGithubSourceRequest` 加
  `prefer_for_platforms: Vec<Platform>`;store-time 校验取值必须是已知 platform。
- admin `apps/$slug/source.tsx` 的抽屉加一组 platform 复选框。
- CLI `source` 命令族(`swarmhive source set/show`)加对应 flag。

## Capabilities

| 能力 | 变更 |
| --- | --- |
| `github-release-source` | MODIFIED —— 源配置新增 per-platform 偏好;源解析顺序可配置;catalog 按偏好排序 |
| `update-check-rn-android` | MODIFIED —— `mirror_urls` 语义从「GitHub 候选」收敛为「其余候选按序」 |

## Impact

- **DB**:`github_source` 加一列,可空/有缺省 → 向后兼容,不需要停机。
- **既有 app 行为零变化**:缺省 `[]` = 全部 platform OSS 优先 = 当前行为。翻转必须显式配置。
  这一点是刻意的 —— GitHub Release 在国内的实测速度**尚未验证**,不能拿它当全局缺省赌注;
  让每个 app 按自己的实测单独配,风险局部化。
- **可观测性**:`download_intent` 事件已带 `source` 维度(`add-github-release-source` 建的),
  配置翻转后可直接查 per-source 下载量验证生效,并回答"GitHub 到底快不快"。
- **存量客户端**:零改动受益(见 Why)。

## Non-goals

- **区域 / IP / Geo 自动路由**。`add-github-release-source` proposal 已把它列为后续;本 change
  只做静态的、显式配置的偏好。
- **多于 `oss`/`github` 的源**。当前只有两个源,"顺序"退化为二选一。真要多源(多存储后端 /
  区域路由)时再引入有序列表,届时本列可平滑升格。
- **解决 OSS 的 APK 限制本身**。绑自定义域名 + 备案、或调 `Content-Type`/`Content-Disposition`
  是运维手段,与本 change 正交 —— 且即便解决了,per-platform 源偏好仍有独立价值。
- **客户端下载器加固**。见 `harden-rn-apk-downloader`,独立 change。

## Depends on

- `archive/2026-07-12-add-github-release-source` —— 本 change 直接续它 design.md D 段的
  未决项;复用其 `github_source` 表、`services/mirror.rs` liveness gate、`?source` 契约、
  `download_intent.source` 埋点。

## Maps to docs

- `docs/07` 「镜像策略」「下载入口」—— 需按新的可配置顺序修订。
- `add-github-release-source/design.md` D、D4 段 —— 本 change 兑现其推迟项与 302 收口红利。

## Acceptance

1. 给 app 配 `prefer_for_platforms: ["react-native-android"]` 后,裸 `/download/{app}/{ver}/{id}`
   **302 到 GitHub**,`download_intent.source = github`;同 app 的 tauri-desktop 产物仍 302 到 OSS。
2. 配了 github 优先、但 mirror 未过 liveness(draft 窗口 / digest 漂移)→ **自动落回 OSS**,不 409。
3. 配了 github 优先、但 `github_source.enabled = false` → 落回 OSS。
4. 显式 `?source=oss` 仍然强制 OSS,不被配置覆盖。
5. RN update 响应:偏好 github 时 `download_url` 裸入口解析到 GitHub、`mirror_urls = ["?source=oss"]`;
   缺省(未配)时行为与本 change 前**逐字节一致**。
6. 未配置的存量 app,所有下载路径行为不变(回归)。
