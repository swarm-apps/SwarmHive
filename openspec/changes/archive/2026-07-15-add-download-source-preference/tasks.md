# Tasks —— add-download-source-preference

> 顺序即依赖顺序。1~3 是治本主线(做完即可修生产),4~6 是配置入口,7~8 验收。

## 1. Schema + Entity

- [x] 1.1 [code] 新 migration `crates/swarmhive-migration/src/m20260715_000001_source_preference.rs`(raw SQL,
      不 import entity):`github_source ADD COLUMN prefer_for_platforms jsonb NOT NULL
      DEFAULT '[]'::jsonb`;注册进 `crates/swarmhive-migration/src/lib.rs` 的 `migrations()`
- [x] 1.2 [code] `swarmhive-entity/src/github_source.rs`:加 `pub prefer_for_platforms: Json`;
      `From<&Model> for api::GithubSourceView` 同步透传。
      **实现偏离**:原写 `#[sea_orm(column_type = "JsonBinary")] Vec<api::Platform>`,但本仓
      既有约定是裸 `Json` + 转换处 `serde_json::from_value(..).unwrap_or_default()`
      (先例:`app.platforms` 同构)。照先例走,并把解码收敛进 Model 的
      `preferred_platforms()` / `prefers_github()` 两个方法

## 2. api-types

- [x] 2.1 [code] `api-types/src/github_source.rs`:`GithubSourceView` 加
      `prefer_for_platforms: Vec<Platform>`;`CreateGithubSourceRequest` 加
      `#[serde(default)] prefer_for_platforms: Option<Vec<Platform>>`。**类型必须是
      `Platform` 而非 `String`** —— 靠 serde 拿到 store-time 枚举校验(design D5)。
      **实现偏离**:①本仓无 `UpdateGithubSourceRequest`(PUT 复用 Create 走 upsert);
      ②请求侧用 `Option<Vec<_>>` 而非 `Vec<_>` —— 缺省即保留,与既有 `enabled` 三态一致
      (先例:`UpdateAppRequest.platforms`)

## 3. 源解析(治本主线)

- [x] 3.1 [code] 偏好判定 `prefers_github(platform)` 落在 **entity 的 `github_source::Model`
      方法**上,不在 `services/mirror.rs`。**实现偏离**:它是纯数据派生(不查库、不判
      enabled),放 service 层没有理由;mirror.rs 只留真正做 I/O 的 `source_row()` 与纯策略
      `source_enabled(Option<&Model>)`
- [x] 3.2 [code] `routes/download.rs resolve_and_redirect`:入口取一次 `github_source` 行并
      向下游透传(净增 0 查询,见 design D3);`order` 三级优先改造。**务必拆开既有的
      `_ => [Oss, Github]` 分支** —— `Some(Oss)` 必须显式匹配,否则显式 `?source=oss` 会被
      配置劫持
- [x] 3.3 [code] `routes/download.rs download_catalog`:`sources` 数组按该 artifact 的
      platform 偏好排序(不再硬编码先 oss 后 github)
- [x] 3.4 [code] `routes/updates.rs android`:`mirror_urls` 语义收敛为「主源之外的其余可用源,
      按 `order` 排列」(design D4 表格);偏好 github 时填 `?source=oss`,缺省时保持
      `?source=github` 不变

## 4. 写侧校验

- [x] 4.1 [code] `routes/github_source.rs` create/update:`prefer_for_platforms` 透传落库;
      去重(同一 platform 列两次无意义);未知 platform 由 serde 挡下 → 确认返回 422 而非 500

## 5. admin UI

- [x] 5.1 [code] `apps/admin/src/lib/api/github-source.ts`:类型同步 `prefer_for_platforms`
- [x] 5.2 [code] `apps/admin/src/routes/_auth/apps/$slug/source.tsx` 的 `SourceDrawer`:加一组
      platform 复选框(`ProFormCheckbox.Group`),文案说明「勾选的平台优先从 GitHub 下载」;
      lingui 中英双语
- [x] 5.3 [code] 源配置页主区展示当前偏好(未配时显式说明「全部平台优先 OSS」,避免空态
      被读成"没配置=坏了")

## 6. CLI

- [x] 6.1 [code] `crates/swarmhive-cli/src/commands/source.rs`:`set` 加
      `--prefer-platform <PLATFORM>`(可重复);`show` 输出该字段

## 7. 测试

- [x] 7.1 [test] `download.rs` 单测:三级优先矩阵 —— 显式 `?source=oss` 不被 github 偏好劫持
      (**这条直接盯 3.2 的分支拆开陷阱**);显式 `?source=github`;偏好命中;偏好未命中该
      platform;无源行。
      **实现增益**:为可测把顺序策略抽成纯函数 `pub(crate) fn source_order(requested,
      prefers_github)`,顺带消掉了 resolve / catalog / updates 三处各写一遍 if-else 的重复
      —— 三处现共用它,不会再在某个分支上分道扬镳
- [x] 7.2 [test] server 集成(testcontainers):配 `["react-native-android"]` 后裸 `/download`
      302 到 GitHub 且 `download_intent.source=github`;同 app 的 tauri-desktop 产物仍 302 到 OSS
      —— `android_preference_routes_to_github_without_diverting_desktop`。两半在同一用例里
      (只测 android 那半的话,把 `prefers_github` 降格成 app 级 bool 也照样绿);另含
      catalog `sources` 按偏好排序(android → `[github, oss]`)与偏好 PUT/GET 往返 + 省略保留
- [x] 7.3 [test] server 集成:偏好 github + mirror 未过 liveness → 落回 OSS 不 409;
      偏好 github + `enabled=false` → 落回 OSS ——
      `github_preference_falls_back_to_oss_when_mirror_not_live`(**两种失败形态都测**:
      draft 窗口 + digest 漂移)与 `github_preference_falls_back_to_oss_when_source_disabled`。
      后者的镜像**故意是活的** —— 镜像若是死的,`enabled` 闸门整个删掉测试也照样绿
- [x] 7.4 [test] 回归:未配置的 app,`/download` 302 目标、catalog `sources` 顺序、
      `mirror_urls` 内容与本 change 前**逐字节一致**(这是"存量零变化"承诺的兑现)
      —— `unconfigured_app_keeps_pre_change_download_behavior`。APK **挂活镜像**(= swarmdrop-rn
      上线前的真实形态):镜像活着才锁得住"缺省仍 OSS 优先",镜像若死,缺省顺序就算被改成
      GitHub 优先 302 也照样落 OSS、回归就漏了。断言 catalog `sources == [oss, github]`
      与 `mirror_urls == ["?source=github"]` 的完整 URL 逐字节
- [x] 7.5 [test] `updates.rs`:偏好 github 时 `mirror_urls == ["?source=oss"]`;
      GitHub-only 时 `mirror_urls == []` —— `mirror_urls_omits_github_when_it_is_primary`。
      GitHub-only 分支实测**符合 D4 表格**(GitHub 是主源 → 不重复列进镜像),并额外断言
      裸 302 确实落 GitHub(否则"GitHub 是主源"只是空口)。**线上形状注意**:`mirror_urls` 是
      `skip_serializing_if = "Vec::is_empty"`,空集是**键缺席**而非 `[]`,故断言键缺席
      (`== json!([])` 会假过 —— serde_json 取不存在的键得 `Null`)

> **7.x 阻塞(已解决)**:`services/mirror.rs` 的私有 `probe` 曾硬编码
> `https://api.github.com`,`MirrorCache` 的 `slots`/`client` 均私有且只暴露
> `is_mirror_live` —— 外部 test crate 既改不了 API base、也塞不进 `live=true` 缓存,故
> **GitHub 候选在 hermetic 测试里恒不可用**,本 change 的头号验收标准(配了偏好 → 302 到
> GitHub)零集成覆盖。(对比 `oauth_smoke` 能 wiremock GitHub:它的 URL 是 DB 列。)
>
> **决定**:加注入缝 `MirrorCache::with_api_base(impl Into<Arc<str>>)`,`probe` 改打
> `{api_base}/repos/...`,缺省仍 `https://api.github.com` → production 行为零变化。
> 这是**为可测性改生产代码**,取舍理由:不加的话,这个 change 唯一真正要修的那条路径
> (偏好 → GitHub)就只能靠手工对着真实 api.github.com 验;而代价只是一个字段 + 一个
> 构造器。**注意它不是 GitHub Enterprise 支持** —— `validate_mirror_url` 仍把资产 host
> 钉在 `github.com`,GHE base 无资产可投。
>
> wiremock 要喂的形状(照 `probe` 实现):`assets[]` 里有一项其 `browser_download_url`
> == artifact 的 `mirror_url`、`state == "uploaded"`、`digest == "sha256:<hex>"`(缺
> digest 时回退比 `size`)。

## 8. 文档 + 发布

- [x] 8.1 [docs] `docs/07`「镜像策略」「下载入口」:改写死顺序的描述为可配置偏好;补
      「阿里云 OSS 匿名 APK 受限 → 建议 android 配 GitHub 优先」的实操指引
- [x] 8.2 [docs] `openapi_surface` 白名单同步(新字段进 OpenAPI —— 归档 change 踩过这个 CI 坑)
- [x] 8.3 [code] server 发版;**上线后给 swarmdrop-rn 配 `["react-native-android"]` 并用
      `download_intent.source` 埋点验证生效**(存量客户端应立刻恢复,无需发版)
      —— 已发 server 0.8.0 / cli 0.9.0 / api-types 0.8.0(PR #9,merge 9abe130),部署至
      生产并配好偏好。**生产逐条验证通过**:裸 `download_url` 302 → GitHub、显式
      `?source=oss` 仍强制 OSS(分支陷阱在真实环境验证)、RN 响应 `mirror_urls ==
      ["?source=oss"]`(主源不重复列自己)、catalog `sources == ["github","oss"]`;
      存量 0.7.16 客户端(sdk 0.1.0 无 failover)取到的字节首 4 位 `504b0304` = 真 APK,
      **零改动恢复** —— 本 change 的核心承诺兑现。
      **遗留**:GitHub 在中国大陆的实测速度仍未知(手头只有东京出口数字,不作数)。
      待用 `download_intent.source` 的 per-source 下载量/成败查证;若国内确实慢,阿里云
      错误信息里的 "please use CNAME instead"(绑自定义域名解除 APK 限制)是更好的主路,
      届时 `--clear-prefer-platforms` 切回即可,无需发版。
