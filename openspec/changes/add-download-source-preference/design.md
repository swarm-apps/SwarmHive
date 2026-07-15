# Design —— add-download-source-preference

## D1. 数据流:偏好在 302 收口,协议不可见

```text
                          ┌───────────────────────────────────────────┐
                          │ admin SPA  apps/$slug/source              │
                          │   platform 复选框 → prefer_for_platforms  │
                          └───────────────┬───────────────────────────┘
                                          │ PUT /api/v1/apps/:slug/source
   swarmhive-cli                          │ (app:update)
   `source set --prefer-platform …` ──────┤
                                          ▼
                          ┌───────────────────────────────────────────┐
                          │ swarmhive-server  routes/github_source.rs │
                          │   校验 platform 取值 ∈ 已知枚举           │
                          └───────────────┬───────────────────────────┘
                                          ▼
                          ┌───────────────────────────────────────────┐
                          │ github_source.prefer_for_platforms jsonb  │
                          └───────────────┬───────────────────────────┘
                                          │
              ┌───────────────────────────┴───────────────────────────┐
              ▼                                                       ▼
┌─────────────────────────────┐                     ┌─────────────────────────────┐
│ routes/download.rs          │                     │ routes/updates.rs           │
│  resolve_and_redirect       │                     │  android → mirror_urls      │
│  order = 显式?source        │                     │  primary = 裸 /download     │
│        > prefer(platform)   │                     │  mirrors = 其余源 ?source=  │
│        > [Oss, Github]      │                     └──────────────┬──────────────┘
│  ↓ 按序取第一个"可用"       │                                    │
│  Oss:  object_key + backend │                                    │
│  Github: mirror_url + live  │                                    │
└──────────────┬──────────────┘                                    │
               │ 302                                               │ JSON
               ▼                                                   ▼
        ┌──────────────┐                          ┌────────────────────────────┐
        │ OSS / GitHub │                          │ 客户端                     │
        └──────────────┘                          │  SDK 0.1.0(无 failover)   │
                ▲                                 │    → 只跟裸 URL 的 302 ✅  │
                └─────────────────────────────────┤  SDK 0.3.0+(有 failover)  │
                          302 收口                 │    → [primary, ...mirrors] │
                                                  └────────────────────────────┘
```

**核心不变量**:客户端协议**零变化**。偏好只改变 302 的目标,不改变任何 URL 的形状。
这是存量 SDK 0.1.0 客户端能零改动受益的全部原因 —— 也是 `add-github-release-source`
D4 立下、本 change 兑现的设计资产。

## D2. Schema + Entity + 迁移

### 迁移(raw SQL,不 import entity —— 沿用本仓既有约定)

`migration/src/m20260715_000001_source_preference.rs`:

```sql
ALTER TABLE github_source
  ADD COLUMN prefer_for_platforms jsonb NOT NULL DEFAULT '[]'::jsonb;
```

- `NOT NULL DEFAULT '[]'` → 存量行自动获得「无偏好」= 当前行为,**无需数据回填**。
- **不加索引**:该列永远只在已按 `app_id`(已有 UNIQUE)定位到单行后读取,从不作谓词。
- 注册进 `migration/src/lib.rs` 的 `migrations()`。

### Entity

`swarmhive-entity/src/github_source.rs`:

```rust
/// JSONB array of `api::Platform`(kebab 串)。空 = 全部 platform 走 OSS 优先(缺省 = 现状)。
/// 只在按 app_id 定位到本行后读取,不作查询谓词,故不建索引。
pub prefer_for_platforms: Json,
```

> **实现订正**:原设计写的是 `#[sea_orm(column_type = "JsonBinary")] Vec<api::Platform>`,
> 但本仓 8 处 jsonb 列(`app.platforms` / `oauth_provider.scopes` / …)一律用裸 `Json`,
> 在转换处 `serde_json::from_value(..).unwrap_or_default()`。照先例走,不为一个列引入
> 第二种范式。`app.platforms` 与本列同构(都是 `Vec<Platform>` 的 jsonb),直接对照它。

解码收敛进 Model 的两个方法,避免每个读点各自 `from_value`:

```rust
/// 损坏 JSON 降级为空 = 降级为 OSS 优先 = 旧行为(同 app.platforms 的 best-effort 范式)。
/// 降级方向也是安全的那侧:坏配置绝不会把流量导向没被显式配置过的地方。
pub fn preferred_platforms(&self) -> Vec<api::Platform>
pub fn prefers_github(&self, platform: api::Platform) -> bool
```

`From<&Model> for api::GithubSourceView` 用 `preferred_platforms()` 透传。

### 为什么不是新表

`app_source_order(app_id, platform, order)` 这类新表要付一整套 CRUD + 更重的 admin UI,
换来的表达力(有序多源)当前**无处消费** —— 只有 oss/github 两个源时,"有序列表"退化成
一个 bool。等真有第三个源(多存储后端 / 区域路由)时,`prefer_for_platforms: Vec<Platform>`
可平滑升格为 `source_order: Map<Platform, Vec<SourceKind>>`,届时再迁移。

## D3. 源解析:三级优先 + 语义不变的 fallback 循环

```rust
// 候选顺序:显式 ?source(最高,既有契约)> per-platform 配置偏好 > 缺省 [Oss, Github]。
use api::DownloadSourceKind::{Github, Oss};
let order = match source {
    Some(Github) => [Github, Oss],
    Some(Oss) => [Oss, Github],
    None if prefers_github => [Github, Oss],
    None => [Oss, Github],
};
```

**注意既有代码的陷阱**:现状是 `Some(Github) => [Github, Oss], _ => [Oss, Github]` ——
`Some(Oss)` 与 `None` 共用一个分支。加配置时必须**拆开**,否则显式 `?source=oss` 会被
配置偏好劫持,破坏既有契约。

> **实现订正**:上面这段 `match` 最终抽成了 `download.rs` 里的纯函数
> `pub(crate) fn source_order(requested, prefers_github) -> [DownloadSourceKind; 2]`。
> 动机是可单测(原地内联在 async + AppState 的函数里测不了),但收益不止于此:
> `/download` 的 302 解析、catalog 的 `sources` 排序、`updates.rs` 的 `mirror_urls` 本来
> **各写了一遍同样的 if-else**,现在三处共用它。同一条策略散在三处,迟早在某个分支上
> 分道扬镳 —— 这正是本 change 要修的那类问题,不该在修它的过程中再造一个。

`prefers_github` 的取值落在 **entity 的 Model 方法**上(见 D2),不在 service 层:它是
纯数据派生,不查库、不做 I/O。**且不检查 `enabled`** —— 禁用源的 Github 候选会在 liveness
gate 处落空并自动走到 Oss,安全性由 fallback 循环的构造保证;在这里重复判定等于把同一个
不变量放进第二个地方,而两处迟早会不一致。

`services/mirror.rs` 相应重构为 I/O 与策略分离:

```rust
pub async fn source_row(db, app_id) -> Option<github_source::Model>  // 唯一的 I/O
pub fn source_enabled(src: Option<&Model>) -> bool                   // 纯策略
pub async fn mirror_serveable(state, src: Option<&Model>, art) -> bool
```

**查询预算**:`resolve_and_redirect` 目前不查 `github_source`。本 change 在其入口处取一次
该行(`app_id` UNIQUE 索引单行命中),同时喂给 `prefers_github` 与下游的 liveness 判定,
**净增 0 次查询**。三个调用点(catalog / resolve / updates)本来就各需要这一行,重构后
每处取一次并向下透传。

## D4. `mirror_urls` / `sources` 的对称化

两处都从「先 oss 后 github」的硬编码,改为「按 `order` 排列已验证可用的源」:

| 场景 | `download_url`(裸) | `mirror_urls` |
| --- | --- | --- |
| 未配偏好(缺省) | 302 → OSS | `["?source=github"]`(mirror 活时)← **与本 change 前逐字节一致** |
| 偏好 github + mirror 活 | 302 → GitHub | `["?source=oss"]`(有 S3 对象 + 活跃 backend 时) |
| 偏好 github + mirror 死 | 302 → OSS(自动落回) | `[]` |
| GitHub-only(无 S3) | 302 → GitHub | `[]` |

客户端候选链恒为 `[偏好源, 其余源...]`,与 SDK 0.3.0 `rn-adapter` 的
`[release.url, ...mirrorUrls]` 语义天然吻合,**SDK 侧零改动**。

已知冗余(可接受):偏好 github 但 mirror 恰好探测失败时,裸 URL 已落回 OSS,而
`mirror_urls` 此时为 `[]` —— 不会重复试。反之偏好 oss 时若 OSS 302 后客户端下载失败,
客户端会用 `?source=github` 重试,正确。唯一浪费路径是 302 解析与客户端重试之间
mirror 状态翻转,属罕见竞态,不值得为它加协议复杂度。

## D5. 校验:platform 取值收敛

`prefer_for_platforms` 是 client 可写的自由数组,必须 store-time 校验每个元素 ∈ 已知
`api::Platform` 枚举(`tauri-desktop` / `react-native-android`),异值 422。理由:未知
platform 静默存下去 = 永远不生效的死配置,用户会以为配了却没效果 —— 这类"配置了但无声
失效"的 bug 排查成本极高。serde 对 enum 的反序列化天然给出这层保护,**只要类型定成
`Vec<api::Platform>` 而不是 `Vec<String>`**。

## D6. 不做的事

- **不改 `?source` 的 URL 形状**:`DownloadSourceKind` 枚举、query 名、302 语义全部不动。
- **不给 catalog 加"推荐源"字段**:排序即表达,不引入新的协议概念。
- **不碰 liveness/digest gate**:偏好只决定"先问谁",可用性判定仍是既有那一套。
