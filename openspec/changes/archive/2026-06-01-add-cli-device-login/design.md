# design

## Context

当前 CLI 登录走 ROPC：`swarmhive login` 收集明文密码 → `POST /api/v1/auth/cli-token` → server `verify_password` → 铸永久 PAT。这把主密码交给客户端、与 OAuth-only 用户 / MFA 互斥。本设计用 RFC 8628 Device Authorization Grant 替换之，把认证步骤委托给 SwarmHive 自己的 Web `/login` 页。

跨四处边界（api-types DTO / server endpoint+entity / CLI 客户端 / admin SPA 批准页），故需数据流图。

约束：

- **device flow 是 public client**：`client_id = "swarmhive-cli"`，无 client_secret（CLI 是分发出去的二进制，存不住密钥）。
- **token 端点走 RFC 8628 wire 格式**（`400 { error }`），其余人面向端点走仓库既有 RFC 9457。
- **认证步骤零新代码**：批准页是 public 路由，未登录就引导去 `/login`，复用密码 +（未来）GitHub 按钮。
- **token 铸造零新逻辑**：复刻 `cli_token` 的临时 `Principal` + `token_service::create`。
- **schema-sync 约束**：`device_code_hash` 用 `#[sea_orm(unique)]`（高熵 blake3，全表唯一安全）；`user_code` 低熵，**不**装 partial unique index（rc.38 schema-sync bug，见 mail/account_token），活跃唯一性靠应用层生成时校验 + 重试。

## Goals / Non-Goals

**Goals：**

- `swarmhive login` 不再经手密码，认证委托浏览器。
- OAuth-only 用户能用 CLI（复用 `/login`）。
- ROPC `cli-token` 端点 + DTO 完全移除。
- device flow 端点符合 RFC 8628（grant_type URN、错误码、polling/slow_down、过期、user_code 熵）。

**Non-Goals：**

- 不实现 loopback+PKCE（留 `add-cli-loopback-login`）。
- 不改 token 存储为 keychain。
- 不发短时 token + refresh。
- 不支持多 client_id 注册。

## 数据流图

```text
                         swarmhive-cli                         SwarmHive server                    Admin SPA (浏览器，可任意设备)
                         (reqwest only)                      (routes/device.rs)                    (routes/device.tsx, public)
 swarmhive login <srv>
        │
        │ 1. POST /api/v1/auth/device/code
        │    { client_id:"swarmhive-cli", token_name:"<host>-<ts>" }
        ├───────────────────────────────────────▶ 查 bootstrap_state
        │                                          ├ needs_bootstrap → 410 device_not_available_during_bootstrap
        │                                          └ 否 → gen device_code(32B)→blake3 hash
        │                                                 gen user_code(8×base20 "WDJB-MJHT")
        │                                                 INSERT device_authorization(status=pending, expires=+15m)
        │ ◀─────────────────────────────────────── 200 { device_code, user_code,
        │                                                 verification_uri={base_url}/device,
        │                                                 verification_uri_complete=…?user_code=WDJB-MJHT,
        │                                                 expires_in:900, interval:5 }
        │
        │ 打印 user_code + verification_uri
        │ webbrowser::open(verification_uri_complete) ─────────────────────────────────▶ GET /device?user_code=WDJB-MJHT
        │                                                                                  │
        │                                                                                  ├ me 401? → "Sign in to continue"
        │                                                                                  │   Link /login?next=%2Fdevice%3Fuser_code%3DWDJB-MJHT
        │                                                                                  │   (登录：密码 或 未来 GitHub) → 回 /device?user_code=…
        │                                                                                  │
        │  ┌── 2. 每 interval 秒轮询 ──┐                                                    └ 已登录 → GET /api/v1/auth/device/lookup?user_code=
        │  ▼                          │                                                       ◀ DeviceAuthorizationView{client_name,client_id,expires_at}
        │ POST /api/v1/auth/device/token                                                     展示 "swarmhive @ macbook 想访问你的账号"
        │  { grant_type:urn:…:device_code, device_code, client_id }                          [批准]                    [拒绝]
        ├───────────────────────────▶ 查行 by blake3(device_code)                              │                          │
        │                             ├ 查不到行/已清掉    → 400 invalid_grant                  │ POST /device/approve     │ POST /device/deny
        │                             ├ expires<now      → 400 expired_token                  │ { user_code }            │ { user_code }
        │                             ├ last_polled 太频  → 400 slow_down                       │ (Session-only;           │ (Session-only;
        │                             ├ status=pending   → 400 authorization_pending  ◀────────┤  Bearer PAT → 403)       │  Bearer → 403)
        │                             ├ status=denied    → 400 access_denied          ◀──────────────────────────────────┤
        │                             ├ status=completed → 400 invalid_grant                  ▼                          ▼
        │                             └ status=approved  → 原子 claim                 status=approved,            status=denied
        │                                UPDATE…SET completed WHERE status=approved    user_id=current             auth:device_denied
        │                                rows=1 才铸；否则 → 400 invalid_grant         approved_at=now
        │                                load user → 临时 Principal                    auth:device_authorized
        │                                token_service::create(Pat, perms=None); auth:token_created
        │ ◀─────────────────────────── 200 { token:"swhv_pat_…", name, kind, created_at }     │ 204                     │ 204
        │                                                                                       └ "回到终端"               └ "已拒绝"
        │ 3. GET /api/v1/auth/me (Bearer 新 token) → email/display_name
        │    写 credentials.toml { server, email?, token }; 打印 "Logged in as <email>"
        │    (/me 失败 → 仍持久化 { server, token }、email 留空、warn，绝不丢已铸 token)
        ▼
   登录完成
```

## Decisions

### 1. 为什么 device flow 而非 loopback+PKCE

SwarmHive CLI 是 release/publish 工具，大量在**远程 build 机 / SSH / CI / 容器**上跑；server 常自托管在 LAN/VPN 内网。loopback 要求浏览器能回跳到运行 CLI 那台机的 `127.0.0.1:port`——SSH 进远程机即失效。device flow 只需出站 HTTPS，全环境可用，且 CLI 不起本地端口（无 macOS 防火墙弹窗）。这与 `gh`（最近的兄弟工具）一致。device flow 的已知钓鱼模式在「单组织自托管、用户池=自己团队」场景里风险极低，批准页明示 client/host 进一步缓解。

### 2. token 端点用 RFC 8628 wire 格式，破例不走 RFC 9457

`POST /device/token` 的轮询响应用 OAuth 标准 `400 { "error": "authorization_pending" }` 等，**不**用仓库通用的 problem+json。理由：(a) 这是「规范化」的核心诉求，标准 device-flow 客户端/库直接可用；(b) 错误码集合是 RFC 8628 固定枚举，不需要 problem+json 的 `type`/`detail` 表达力。其余三个人面向端点（lookup/approve/deny）仍走 RFC 9457。这是一处**有意的契约分叉**，在 spec 中明确。

错误码全集（RFC 8628 §3.5 / RFC 6749 §5.2）：

- `authorization_pending` —— 行存在且 `status=pending`。
- `slow_down` —— 轮询快于 `interval`。
- `access_denied` —— `status=denied`。
- `expired_token` —— 行存在但 `expires_at < now()`。
- `invalid_grant` —— **查不到行**（含未知 `device_code` 与已被 lazy 清理的行）**或**已 `completed`（单次性，二次轮询）**或**原子 claim 失败。状态机以「查行 by blake3(device_code)」起手，**查不到必须先于任何 status 分支返回 `invalid_grant`**（不能让 `completed` 兜底 not-found）。
- 请求校验：`grant_type` 非 `urn:ietf:params:oauth:grant-type:device_code` → `400 unsupported_grant_type`；`client_id` 缺失/不等于 `swarmhive-cli` → `400 invalid_request`。

防枚举：`device_code` 高熵 + blake3 精确匹配，枚举面可忽略；错误码区分（pending/denied/expired/...）只在持正确 `device_code` 时可见，对外不泄露行是否存在，与 lookup 的「未知与过期同形 404」防枚举叙述一致。

### 3. 批准页是 public 顶层路由，不放 `_auth/`

`_auth/route.tsx` 的 guard `redirect({ to:'/login', search:{ next: location.pathname } })` 只带 `pathname`、**丢 search**，会丢掉 `?user_code`。故 `/device` 做成 public 顶层路由（仿 `accept-invite.tsx` / `reset-password.tsx`），页面自管登录闸门：未登录时 `Link` 到 `/login?next=` + `encodeURIComponent(pathname+search)`，登录后完整带回 `user_code`。这同时让 device 页**自动继承** `/login` 上的所有登录方式——与 OAuth proposal 的唯一接口契约。

⚠️ **现状 login.tsx 不支持带 query 的 `next`**：[login.tsx:95-96](../../../apps/admin/src/routes/login.tsx) 现在是 `const next = search.next ?? "/"; router.navigate({ to: next, replace: true })`。TanStack Router v1 的 `to` 是 typed path、**不解析 query**，传 `/device?user_code=…` 会把 query 丢掉。本 proposal 的核心契约依赖这条回跳，故必须改造 login.tsx 成功跳转：用 `router.navigate({ href: next })`（v1 支持 href 形态），或 `const u = new URL(next, location.origin); router.navigate({ to: u.pathname, search: Object.fromEntries(u.searchParams) })`。这是一个明确的 [code] 改动，不是「复用既有能力」。

### 4. token 铸造复用 `cli_token` 套路

poll 命中 `approved` 时不引入新铸造逻辑：`load user → service::load_user_permissions → 构造临时 Principal{auth_method:Session{nil}} → token_service::create(CreateTokenRequest{kind:Pat, permissions:None, expires_at:None})`。与现 `cli_token` 完全一致，PAT 继承 owner 实时权限、可在 Tokens 页/`logout` 撤销。

**铸造前必须原子 claim**（`/device/token` 公开 + 可并发轮询，`token_service::create` 非幂等——每调一次 insert 一行 `api_token` + 写一条 `auth:token_created`）。两个并发 poll 同时读到 `approved` 会各铸一个 PAT。故铸造前先做条件更新占位：

```rust
// sea-orm：update_many + filter status=approved，靠 rows_affected==1 抢占
let claimed = device_authorization::Entity::update_many()
    .col_expr(Column::Status, Expr::value(DeviceGrantStatus::Completed))
    .filter(Column::Id.eq(row.id))
    .filter(Column::Status.eq(DeviceGrantStatus::Approved))   // CAS：只有仍是 approved 才抢到
    .exec(db).await?;
if claimed.rows_affected != 1 { return device_err(InvalidGrant); }  // 已被另一 poll 抢走
// 抢到后才 load user + token_service::create
```

抢占失败（`rows_affected==0`，已被另一 poll 置 `completed`）→ `invalid_grant`。这保证「一个 approved grant 至多铸一个 PAT、至多一条 `auth:token_created`」（spec 可断言）。Postgres READ COMMITTED 下条件 UPDATE 的行锁天然串行化两个并发 claim。

### 5. entity 唯一性与过期

- `device_code_hash`：`#[sea_orm(unique)]`（blake3(32B 随机)，碰撞概率可忽略，全表唯一安全）。
- `user_code`：低熵（8×base20 ≈ 34.5 bit），**不**装唯一索引（partial unique 触发 rc.38 schema-sync bug）。生成时查 `status=pending AND expires_at>now` 是否已有同 `user_code`，命中则重生成（最多 N 次）。
- 过期：不存 `expired` 状态，由 `expires_at < now()` 在 poll/lookup 时派生（避免后台 sweep 改状态）。`/device/code` 入口顺手 lazy 清理旧行，但**带 1h grace**：`DELETE WHERE expires_at < now() - INTERVAL '1 hour'`。这样刚过期的行仍在表里、轮询返 `expired_token`（RFC 8628 语义）；只有早已过期的行被物理删，其轮询落 `invalid_grant`（not-found 分支）。若用 `DELETE WHERE expires_at < now()` 无 grace，过期码会立刻返 `invalid_grant` 而非 `expired_token`——两者对客户端都是「terminal，重跑 login」，但 `expired_token` 语义更准。
- `before_save` 钩子填 `created_at`；`expires_at` handler 显式 `Set(created + 15min)`（注意 sea-orm before_save caveat：单条 `insert` 路径才触发钩子，本处用单条 insert 故 `created_at` 自动）。

### 6. bootstrap window 排除

user 表空时 `/device/code` 返 `410 device_not_available_during_bootstrap`（typed problem+json，复用 `ApiError::Typed`）。理由：没有任何已存在用户能进批准页（批准页 → /login → `__root` 把空 DB 全路径跳 `/setup`），允许发码只会让 CLI 静默轮询到超时。即时报错引导用户先在 Web 完成 Owner setup。与 OAuth 的 bootstrap 排除对称。

### 7. 限流与防滥用

⚠️ **governor 不是全局的**：[lib.rs](../../../crates/swarmhive-server/src/lib.rs) 里 `GovernorLayer`（per-IP 5rps/burst20，`SmartIpKeyExtractor`）只 `.layer()` 在 `sensitive` 子路由（auth+setup+password_reset）上，顶层 `api_router`（tokens/mail/apps/...）无 governor。`openapi_router()` 的 sensitive 子路由不挂 layer（仅 codegen）。所以「两处 merge」并不对称：`build_router` 的 sensitive 受 governor、`openapi_router` 的不受。

挂载选择：**整个 `routes::device::router()` 挂进 `sensitive` 子路由**（code + token 都继承 per-IP 5rps/burst20 governor）。不拆成「code 进 governor、token 不进」——单个 OpenApiRouter 无法只 layer 一部分，拆需两个 router，复杂度不值。

- governor 作**粗粒度 DoS 兜底**，协议内 `slow_down`（per-row）才是轮询的主限速器。两者职责分层。
- **轮询数学**：interval=5s → 单 CLI 0.2 rps，远低于 5 rps sustained；20 个 CLI 共享一个 NAT IP ≈ 4 rps，仍在 5rps+burst20 预算内。正常轮询**不会**触 governor，故「429 非 slow_down、标准客户端不退避」的担忧在 MVP 实际不发生。极端 NAT 扇出真触 429 时，缓解是调大 burst 或把 token 端点单拆出 governor——记录但非 MVP。
- `slow_down` 状态机（避免边界抖动活锁）：首次轮询 `last_polled_at=null` → **不** slow_down，按 status 返回（通常 `authorization_pending`）并写 `last_polled_at=now`；之后每次先比较 `now - last_polled_at < interval_secs` → `slow_down`。**被拒的 slow_down 那次不刷新 `last_polled_at`**（只在正常受理 pending/approved 时更新），否则边界抖动客户端会反复被判 slow_down。合规客户端收到 slow_down 后 `interval += 5` 退避到 10s，自然不再触发 5s 窗口。

### 8. approve/deny/lookup 仅接受 Session，拒 Bearer

`Principal` extractor（[extractor.rs](../../../crates/swarmhive-server/src/auth/extractor.rs)）里 `Authorization: Bearer` **优先于** cookie session。若 approve/deny 只写 `require auth`（裸 `Principal`），一个低权限 PAT 持有者就能脱离浏览器 `POST /device/approve` 给自己批一个继承 owner 实时权限的设备 grant——绕过「人在浏览器里确认」这道钓鱼缓解。故 approve/deny（lookup 同理）**只接受 Session 来源的 Principal**：`match principal.auth_method { Session{..} => ok, Pat|ApiToken => 403 }`（或专门的 Session-only extractor）。这把「批准必须是交互式浏览器会话」钉成契约。

## Entity

```rust
// crates/swarmhive-entity/src/device_authorization.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]          // 对齐 string_value，避免 PascalCase 422
pub enum DeviceGrantStatus {
    #[sea_orm(string_value = "pending")]   Pending,
    #[sea_orm(string_value = "approved")]  Approved,
    #[sea_orm(string_value = "denied")]    Denied,
    #[sea_orm(string_value = "completed")] Completed,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "device_authorization")]
pub struct Model {
    #[sea_orm(primary_key)] pub id: Uuid,           // v7
    #[sea_orm(unique)]      pub device_code_hash: String,  // blake3 hex
    #[sea_orm(indexed)]     pub user_code: String,         // "WDJB-MJHT"
    pub client_id: String,                          // "swarmhive-cli"
    pub client_name: Option<String>,                // "swarmhive @ macbook.local"
    pub token_name: String,                         // 待铸 PAT 名
    pub scope: Option<String>,                      // 保留；null = 全权 PAT
    pub status: DeviceGrantStatus,
    pub user_id: Option<Uuid>,                      // 批准时填
    pub interval_secs: i32,                         // 默认 5
    pub last_polled_at: Option<DateTimeUtc>,
    pub approved_at: Option<DateTimeUtc>,
    pub expires_at: DateTimeUtc,                    // created + 15min
    pub created_at: DateTimeUtc,                    // before_save 自动
}
impl ActiveModelBehavior for ActiveModel {}
```

索引：`device_code_hash` 唯一（poll 查找）；`user_code` 普通索引（lookup/approve/deny 查找）。无 partial unique index。

## api-types DTO（新 `crates/swarmhive-api-types/src/device.rs`）

```rust
pub struct DeviceCodeRequest  { pub client_id: String, #[serde(default)] pub scope: Option<String>, #[serde(default)] pub token_name: Option<String> }
pub struct DeviceCodeResponse { pub device_code: String, pub user_code: String, pub verification_uri: String, pub verification_uri_complete: String, pub expires_in: i64, pub interval: i64 }
pub struct DeviceTokenRequest { pub grant_type: String, pub device_code: String, pub client_id: String }  // grant_type = "urn:ietf:params:oauth:grant-type:device_code"
pub struct DeviceTokenResponse { pub token: String, pub name: String, pub kind: ApiTokenKind, pub created_at: DateTime<Utc> }
#[serde(rename_all = "snake_case")]
pub enum DeviceTokenError { AuthorizationPending, SlowDown, AccessDenied, ExpiredToken, InvalidGrant }
pub struct DeviceTokenErrorResponse { pub error: DeviceTokenError }   // RFC 8628 token-端点专用
pub struct DeviceVerifyRequest { pub user_code: String }             // approve/deny body
pub struct DeviceAuthorizationView { pub user_code: String, pub client_id: String, pub client_name: Option<String>, pub created_at: DateTime<Utc>, pub expires_at: DateTime<Utc> }
```

删 `CliTokenRequest` / `CliTokenResponse`。`DeviceTokenResponse` 复用现 `CliTokenResponse` 的形（`token/name/kind/created_at`），故 CLI 解析端改动最小。

## Risks / Trade-offs

- **[device flow 钓鱼]**——攻击者诱导受害者在真 IdP 输入攻击者的 user_code。Mitigation：批准页明示 `client_name`（内嵌 host，如 `swarmhive @ macbook.local`）+「仅在你刚运行过 `swarmhive login` 时批准」文案；approve/deny 只接受 Session（拒 Bearer PAT 自批，见 Decision 8）；单组织自托管场景受害面=自己团队，远小于公共 IdP。（注：MVP 的 `DeviceAuthorizationView` 只回 `client_name`，**不**回 host/IP 独立字段——host 已内嵌在 `client_name`；若日后要显示来源 IP，需加 `requester_ip` 列并从 `RequestCtx::from_headers` 在 `/device/code` 时落库。）
- **[approved grant 被并发铸两次]**——`/device/token` 公开可并发，`token_service::create` 非幂等。Mitigation：铸造前条件 UPDATE 原子 claim（Decision 4），`rows_affected==1` 才铸，否则 `invalid_grant`。
- **[Bearer PAT 自批]**——见 Decision 8，approve/deny/lookup 只接受 Session-derived Principal。
- **[/me 失败孤儿化已铸 PAT]**——token 已铸+`completed` 后再 `GET /me` 取 email，若 /me 失败（网络抖/503/解析错），CLI 不能因拿不到 email 就 bail——那会留下一个已铸但本地无记录的永久 PAT（须手动撤销），且用户重试会再铸一个。Mitigation：**token 获取即成功边界**——/me 失败时 CLI 仍持久化 `{ server, token }`（email 留空/占位），warn 不 bail；`Credentials.email` 改 `Option<String>` 或写空串、下次 /me 成功回填。
- **[user_code 低熵碰撞]**——8×base20≈34.5bit。Mitigation：仅在活跃（pending+未过期）集合内要求唯一，生成时校验重试；15min TTL + lazy 清理使活跃集合极小。
- **[token 端点契约分叉]**——`/device/token` 不走 RFC 9457，前端/工具若假设全站 problem+json 会困惑。Mitigation：spec 显式标注；该端点仅 CLI 调用，admin SPA 不碰。
- **[与 registration-policy ⑤ 的 pending_approval 联动]**——⑤ 给 `_auth` guard 加「`user.status===pending_approval` → /awaiting-approval」拦截，但 `/device` 是 public 路由、approve/deny 只校验 Session（不校验 `user.status`）。后果：经 ⑤ 自助注册产生、处 `pending_approval` 的用户虽被挡在业务页外，仍能登录 `/device` 批准设备并铸 PAT。Mitigation：⑤ 落地时，approve/deny 同样校验 approver 的 `user.status`（仅 `active` 可批准，否则 403），与 `_auth` guard 对称。本 proposal 先在 spec 留该约束位，实际收口随 ⑤；本 proposal 单独落地时（无 ⑤、无 pending_approval 状态）不受影响。
- **[base_url 配错]**——`verification_uri` 取 `ServerConfig.base_url`，配错会让用户打开错误地址。Mitigation：与 invite/reset 链接同源，部署时本就要配对；CLI 同时打印 `user_code` 让用户可手动到 `/device` 输入。
- **[移除 cli-token 的破坏性]**——已脚本化依赖 `/auth/cli-token` 的用户会破。Mitigation：CI 场景本应用 `SWARMHIVE_TOKEN`（scoped API Token），proposal Non-goals 已说明；CHANGELOG / docs 标破坏性变更。
- **[dev 模式 base_url 跨端口]**——dev 下 SPA `:5173`、server `:3030`，`base_url` 须指向 `:5173`（SPA origin）否则 `/device` 打不开。Mitigation：dev 配置 `base_url=http://localhost:5173`（invite/reset 已是此约定）。

## Migration Plan

无 DB 破坏（仅加表）。部署路径：

1. 本 proposal 后 schema 自动加 `device_authorization` 表。
2. 旧 CLI 二进制调 `/auth/cli-token` → 404；用户升级 CLI 后 `swarmhive login` 走 device flow。
3. 已存的 PAT（旧 cli-token 铸的）继续有效（同 `api_token` 表，鉴权链路不变）。

回滚：revert + 重启；`device_authorization` 表残留无害。

## Open Questions

- **CLI 无浏览器环境（纯 headless 且无 GUI）**——`webbrowser::open` 失败时 CLI 仍打印 `verification_uri` + `user_code` 让用户手动在另一设备打开。已覆盖，无需额外分支。
- **是否给 device flow 加独立 audit `auth:device_code_requested`**——不加（过噪）；安全敏感事件由 `auth:token_created` + `auth:device_authorized` 覆盖。
- **token_name 冲突**——多设备同名 PAT 允许（`api_token.name` 非唯一）；host+ts 已足够辨识。
