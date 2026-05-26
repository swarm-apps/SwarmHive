# design — add-pat-and-api-token

## Context

`add-auth-and-rbac` 已经把 Session（浏览器 cookie 路径）跑通：`SeaOrmStore`、`Principal` extractor、`require_permission!` 宏、`services::audit` 都就位。但 `Principal::from_request_parts` 里的 Bearer 分支是占位——任何 `Authorization` 头都短路返回 `Unauthorized`。

CLI（`swarmhive-cli`）目前命令全是 `todo!()`，没有任何认证机制：`swarmhive login` / `publish` / `promote` 都跑不起来。要让 add-app-release-artifact / add-storage-and-presign-upload 推进时 CLI 已经可用，必须先把"长期 Bearer token"这层加上。

依赖图位置：紧跟 `add-auth-and-rbac`，与 `add-oauth-github` / `add-mail-infrastructure` 并行；**早于** `add-app-release-artifact`——这意味着本 proposal 落地时 `app` / `channel` 实体还不存在，PAT/API Token 现在只能做 org-wide scope。

## Goals / Non-Goals

**Goals**

- 一张 `api_token` 表承载 PAT 与 API Token 两种凭证，统一 hash 存储 + 撤销机制
- Bearer 鉴权与 Session 鉴权在 `Principal` 这一层合流，下游业务 handler 不感知凭证种类
- CLI `swarmhive login` 跑通：交互式输入密码 → 拿 PAT → 写 `~/.config/swarmhive/credentials.toml` (0600)
- API Token 的 `permissions` 必须是 creator 当前权限的子集（防止越权升级）
- 撤销立即生效，无 grace period
- `token_created` / `token_revoked` / `token_used_first_time` 三种 audit 事件

**Non-Goals**

- app / channel scope（留给 add-app-release-artifact 落地后的 update-token-add-app-scope mini proposal——本阶段加列也无校验对象）
- device-code OAuth flow（CLI MVP 仅 password grant）
- token auto-rotation / 自动续期
- 按 IP / UA 限制
- 改 Session 路径（已稳定）
- Admin SPA 的 token 管理 UI（后端 endpoint + 集成测试覆盖即可；UI 留给后续 admin client 提案）

## Decisions

### D1. Token 字符串格式：`swhv_<kind>_<43char base64url>`

**选**：`swhv_pat_<43>` for PAT，`swhv_api_<43>` for API Token。43 个字符 = 32 字节随机 base64url 编码（无 padding）。

**Why**：

- 公开 grep / 日志排查友好：泄露到日志、聊天、PR 时一眼能分辨"用户个人 token"还是"CI 机器 token"
- 不暴露安全语义：`kind` 仅是 metadata，知道是 PAT 还是 API Token 对攻击者无额外信息——他要的是 secret 本身
- 对齐生态实践：GitHub `ghp_`、Gitea `gtp_`、Slack `xoxb_` 都用 kind 前缀

**Alternatives 拒绝**：

- 不带 kind 的 `swhv_<46>`：DB 查表前必须扫两表（或用 kind 列做范围过滤），增加路径复杂度；日志排查时 kind 全靠 DB 才能知道
- 内嵌 row id `swhv_pat_<id>_<secret>`：暴露主键、增加 token 长度、id 也得当 secret 比对否则没意义

### D2. 查表策略：`token_hash` 唯一索引等值查询

**选**：完整 32 字节 blake3 hash 存 `token_hash bytea`，加 `UNIQUE` 索引；请求来时 `blake3(plain).hash()` → `WHERE token_hash = $1 LIMIT 1`。

**Why**：

- blake3 是密码学 hash，等值比较不存在 timing leak（input 未知，hash 输出对比对 attacker 不暴露任何 secret bit）
- 唯一索引 = O(log n)，token 表过 10w 行也快
- 实现路径短，无需 prefix-lookup + constant-time compare 那套 GitHub-style 分段方案

**Alternatives 拒绝**：

- GitHub 风格 prefix-lookup + constant-time secret compare：在 `argon2` hash 时确有必要（计算贵），但 blake3 单次 hash 已经 sub-microsecond，引入额外列没有收益
- bcrypt/argon2 hash token：每请求做一次 KDF 是巨大浪费，token 已是 32 字节高熵随机不需要 KDF

### D3. PAT 权限：live 跟随 user；API Token：snapshot 子集

**选**：

- `api_token.permissions` 列含义：`NULL` ⇒ live（运行时按 `owner_user_id` 查 `user_role` → `role_permission` → 当前权限集）；`NOT NULL` ⇒ snapshot（创建时显式存的子集）
- 在 create 时强制：`kind='pat'` ⇒ `permissions = NULL`；`kind='api'` ⇒ `permissions = NOT NULL` 且必须 ⊆ creator 当前权限

**Why**：

- PAT 的语义是"用户在 CLI 上的化身"——用户权限收缩了，PAT 也应当立即收缩；snapshot 会让"撤角色"漏一个攻击面
- API Token 的语义是"最小权限机器凭证"——必须可显式裁剪到只够 CI 用的几个 permission（如 `release:publish` + `artifact:upload`）
- 单列双语义靠 NULL 区分，避免引入 `permission_mode enum`

**Alternatives 拒绝**：

- PAT 也走 snapshot：撤角色不影响旧 PAT，violates principle of least surprise
- 两张表分开：与上层 Bearer extractor 强耦合，所有 query / audit / list endpoint 都得双轨
- 在 PAT 上允许裁剪：让"两个概念混在一个 kind"——用户想要可裁剪就该创 API Token

### D4. CLI 登录入口：专用 `POST /api/v1/auth/cli-token`

**选**：新增 `POST /api/v1/auth/cli-token`，body `{ email, password, token_name }`，公开 endpoint，governor 与 `/auth/login` 同档（5 rps / burst 20）。返回 `{ token, name, kind: "pat", created_at }`。

**Why**：

- CLI 只为换一个 PAT，没必要维护 cookie jar、follow set-cookie、再调第二个 endpoint
- 单次 RTT：密码校验 + 创建 token 一气呵成
- 与 login 共用 argon2 verify 逻辑（DUMMY_PHC + 等时返回），未来加 2FA 时这个 endpoint 一起改

**Alternatives 拒绝**：

- 复用 `POST /api/v1/auth/login` 拿 cookie + 再调 `POST /api/v1/tokens`：CLI 平添 cookie 管理；两步 RTT；语义上 login 是浏览器路径，污染了它
- Device-code：需要 server 维护 auth_pending 状态 + 用户手动开浏览器粘贴 code，MVP 无 admin UI 引导不值得做

### D5. `last_used_at` 节流：1 分钟 SQL-内节流

**选**：每次 Bearer 命中时执行：

```sql
UPDATE api_token
SET last_used_at = now()
WHERE id = $1
  AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')
RETURNING last_used_at IS NOT NULL AS was_used_before;
```

`RETURNING ... AS was_used_before` 不返回行（WHERE 没命中）= 这次没更新 = 1 分钟内已写过；返回 `false` = 首次使用，触发 `token_used_first_time` audit。

**Why**：

- 每请求一次 UPDATE 在高 QPS 下成本不可忽视，节流把负载降到 ≤1 次/分钟/token
- 节流靠 WHERE 子句而不是应用层缓存，无 race condition、无内存状态
- `RETURNING was_used_before` 一次往返同时拿到"是不是首次使用"，省一次 SELECT

**Alternatives 拒绝**：

- 应用层 in-memory cache：多实例部署不一致；进程重启丢
- 完全不节流：高 QPS 下 `api_token` 表写放大
- 异步 channel 批量 flush：增加复杂度，crash 时丢数据

### D6. Bearer 分支与 Session 分支的优先级

**选**：`Principal::from_request_parts` 先看 `Authorization: Bearer`，命中即走 Bearer 分支（不再尝试 cookie）；无 Authorization 头才走 Session。Cookie + Bearer 同时存在以 Bearer 为准。

**Why**：

- 显式的 Bearer 应当压过隐式的 cookie——CLI 用户期望 `SWARMHIVE_TOKEN` 强制覆盖任何浏览器残留 cookie
- 单一鉴权来源避免"双重身份"歧义
- 与 GitHub / GitLab API 行为一致

**Alternatives 拒绝**：

- Cookie 优先：浏览器扩展类工具会同时携带两者，cookie 优先会让 CLI 测试时被旧 cookie 干扰
- 同时存在 → 422：合法的 curl + cookie 测试场景被拒绝，体验差

### D7. Token 表只读字段 / write-once 字段约束

- `token_hash` / `prefix` / `kind` / `owner_user_id` 一旦 insert 不再变更
- `permissions` 只在 create 时校验子集；后续不允许 PATCH（要新权限就重发 token）
- `revoked_at` 只能 NULL → 非 NULL，不能反向
- 不引入 `updated_at`（这张表的语义是 immutable + soft-delete + last_used heartbeat）

## Schema

### `api_token` entity (`crates/swarmhive-entity/src/api_token.rs`)

```rust
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "api_token")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,                          // uuid v7
    pub owner_user_id: Uuid,               // FK → user.id
    pub kind: ApiTokenKind,                // 'pat' | 'api' (Rust enum + sea_orm derived)
    pub name: String,                      // human label, e.g. "macbook-cli" or "swarmdrop-beta-ci"
    pub prefix: String,                    // 前 12 char of plain token, for UI/log display
    #[sea_orm(column_type = "Binary(32)", indexed, unique)]
    pub token_hash: Vec<u8>,               // blake3(plain) — UNIQUE index
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub permissions: Option<Vec<PermissionName>>, // NULL=live (PAT); Some(subset) for API
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,

    #[sea_orm(belongs_to, from = "owner_user_id", to = "id")]
    pub owner: Option<crate::user::Entity>,
}
```

### 索引

- `UNIQUE (token_hash)` —— 等值查表
- `INDEX (owner_user_id, kind)` —— list endpoint by-owner
- `INDEX (revoked_at)` —— 后台扫已撤销可清理（非 MVP，留 hook）

### 迁移

schema-sync 路径（与已有 entity 一致）：entity crate 顶层 `pub mod api_token;` 注册进 registry，server 启动 `db::sync_schema` 自动建表。无需 sea-orm-migration crate。

### CHECK 约束（应用层强制 + DB 兜底）

- `kind='pat' ⇒ permissions IS NULL` —— 在 create service 强制；DB 不加 CHECK（sea-orm 2 enum 在 jsonb 配合下 CHECK 写起来啰嗦，且这条不变式不会被外部直接 INSERT 触发）
- `kind='api' ⇒ permissions IS NOT NULL`

## ASCII 数据流

### Bearer 鉴权链路（每个 authenticated 请求）

```text
                 ┌─────────────────────────────────────────────────────────┐
                 │                                                         │
   request ─────▶│  Principal::from_request_parts                          │
                 │  ┌───────────────────────────────────────────────────┐  │
                 │  │  has Authorization: Bearer swhv_(pat|api)_xxx ?   │  │
                 │  └─────────┬─────────────────────────┬─────────────  ┘  │
                 │       yes  ▼                     no  ▼                  │
                 │  ┌───────────────────┐    ┌────────────────────────┐    │
                 │  │ bearer::resolve   │    │ tower-sessions cookie  │    │
                 │  │   1. parse prefix │    │   load_principal (已有)  │   │
                 │  │   2. blake3 hash  │    │                        │    │
                 │  │   3. SELECT row   │    └─────────┬──────────────┘    │
                 │  │   4. check        │              │                   │
                 │  │      revoked/exp  │              │                   │
                 │  │   5. load perms   │              │                   │
                 │  │      (live|snap)  │              │                   │
                 │  │   6. throttled    │              │                   │
                 │  │      UPDATE       │              │                   │
                 │  │      last_used_at │              │                   │
                 │  │   7. first-use?   │              │                   │
                 │  │      audit write  │              │                   │
                 │  └─────────┬─────────┘              │                   │
                 │            └────────────┬───────────┘                   │
                 │                         ▼                               │
                 │                ┌─────────────────┐                      │
                 │                │   Principal     │                      │
                 │                │ { user, scope,  │                      │
                 │                │   perms, auth } │                      │
                 │                └─────────────────┘                      │
                 └─────────────────────────────────────────────────────────┘
```

### CLI login 链路

```text
   user                CLI (swarmhive-cli)              Server                       DB
    │                       │                              │                          │
    │ swarmhive login URL   │                              │                          │
    ├──────────────────────▶│                              │                          │
    │                       │ prompt email + password      │                          │
    │◀──────────────────────┤                              │                          │
    │ user types stdin      │                              │                          │
    ├──────────────────────▶│                              │                          │
    │                       │ POST /auth/cli-token         │                          │
    │                       │   { email, password,         │                          │
    │                       │     token_name: "<host>" }   │                          │
    │                       ├─────────────────────────────▶│                          │
    │                       │                              │ argon2 verify password   │
    │                       │                              ├─────────────────────────▶│
    │                       │                              │◀─────────────────────────┤
    │                       │                              │ INSERT api_token         │
    │                       │                              │   kind='pat', perms=NULL │
    │                       │                              ├─────────────────────────▶│
    │                       │                              │ audit token_created      │
    │                       │                              ├─────────────────────────▶│
    │                       │ 200 { token: "swhv_pat_…",   │                          │
    │                       │       name, created_at }     │                          │
    │                       │◀─────────────────────────────┤                          │
    │                       │                              │                          │
    │                       │ write ~/.config/swarmhive/   │                          │
    │                       │   credentials.toml (0600)    │                          │
    │ "Logged in as foo@…"  │                              │                          │
    │◀──────────────────────┤                              │                          │
```

## Risks / Trade-offs

- [PAT live 权限：取消用户某个 role 后旧 PAT 立即收缩] → 这是**特性**不是风险；但需文档明确告知（很多用户期望 PAT 是"固定凭证"）。在 docs/13 + admin UI 提示语标注
- [`last_used_at` 节流靠 SQL 子句] → 1 分钟窗口内首次使用判断可能丢——如果 token 同时被两个 worker 命中，只有一个会 RETURNING true。**接受**：`token_used_first_time` audit 允许偶尔重复（去重不是强诉求）
- [api_token 表与 user 表强耦合，删除用户须先撤销其 token] → DB 不加 FK ON DELETE CASCADE（避免误删带走 audit 关联）；user 删除走 service 层显式撤销 token 然后 anonymize（留给 user-management proposal）
- [`permissions jsonb` 数组没有 GIN 索引] → MVP 不做 by-permission 反查，列表过滤只按 owner / kind / revoked
- [Bearer 优先于 cookie 可能影响个别测试场景] → 集成测试明确两条路径分开测；混合场景的语义在 docs/backend.md 写明
- [`swhv_pat_` / `swhv_api_` 前缀公开 kind 信息] → 是 metadata，不暴露安全语义；与生态实践一致
- [CLI password grant 把明文密码传到 server] → HTTPS 兜底；不引入额外密码 hash 层（与 `/auth/login` 同等待遇）

## Migration Plan

1. entity 添加 `api_token` → schema-sync auto-create
2. server 加 Bearer 分支（extractor.rs 重写）、`/auth/cli-token`、`/tokens` CRUD
3. CLI 加 `login` / `logout`、credentials 文件读写
4. 集成测试覆盖 5 个 acceptance 场景
5. docs/13 + docs/12 章节补全；CLAUDE.md 加 swarmhive login 示例
6. 无 rollback 需要：纯 additive；如发现严重问题可直接 revert commits（schema-sync 不会主动 drop 表，下次启动 entity 注册位置删除后表会保留但成 orphan，手动 DROP）

## Open Questions

- (待 add-app-release-artifact 时回答) app/channel scope 落到 `api_token` 时是新加两列还是引入 `api_token_scope` 关联表？倾向加列；多对多 scope 不在 MVP 视野
- (待 admin UI 实施时回答) PAT 列表是否要给"被自己撤销 vs 被 admin 撤销"加额外列？目前 `revoked_at` 没有 actor；可未来扩展 `revoked_by_user_id`
