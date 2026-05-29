# SwarmHive 的 CLI 鉴权：从 PAT 长什么样讲起，再对比业界八种主流方案

> 写在前面：每个写过 CLI 的人都会在某个早晨被同一个问题怼住——"用户在终端里怎么证明自己是自己？" 听上去是道送分题，真要做对涉及 11 件事：token 格式、过期、撤销、节流、节流后的审计、本地存哪、文件权限、env 优先级、CI 怎么用、私有 CA 怎么过、离线登出怎么办。
>
> 这篇把 SwarmHive 的 CLI 鉴权链路从客户端到服务端逐层拆开，重点放在 **PAT (Personal Access Token)** 的产生 / 校验 / 撤销 / 节流四件套，然后把它放到业界 8 种主流方案里横向对比，告诉你**为什么 SwarmHive 选这种**、什么场景下**应该选别的**。
>
> 读完应该能：
>
> 1. 看懂 `swhv_pat_<43>` 这种长串是怎么来的、为什么 DB 里存 blake3 hash 而不是明文
> 2. 理解 PAT vs API Token 在 "permissions=NULL vs Some(subset)" 这一条上的语义鸿沟
> 3. 知道 GitHub / AWS / Google Cloud / Docker / Cargo / npm / kubectl / Stripe CLI 各自的鉴权姿势和缺陷
> 4. 给自己手上的项目挑出最合适的鉴权方案，避开常见雷区

---

## 0. 一次 `swarmhive login` 的全景

先建立物理直觉。开发者在终端敲下 `swarmhive login https://updates.example.com`，五秒后看到 `Logged in to ...`，中间走了哪几步？

```mermaid
flowchart LR
    USER([User]) -->|email + password| CLI[swarmhive CLI<br/>main.rs]
    CLI -->|prompt rpassword<br/>无回显| TTY[TTY]
    CLI -->|POST /api/v1/auth/cli-token<br/>JSON: email + password + token_name| SRV[Server<br/>routes/auth.rs]
    SRV -->|argon2id verify| DB[(Postgres user)]
    SRV -->|load_user_permissions| DB
    SRV -->|token::mint:<br/>32B OsRng → base64url<br/>blake3 hex hash| MINT[mint]
    MINT -->|INSERT api_token<br/>token_hash 唯一索引| DB
    MINT -->|audit auth:token_created| DB
    SRV -->|200 OK: plaintext PAT<br/>★ 仅此一次| CLI
    CLI -->|toml::to_string_pretty<br/>+ chmod 0600| FS[~/.config/swarmhive/<br/>credentials.toml]
    CLI -->|Logged in to ...| USER
```

简单总结：

- **客户端**只做三件事：收凭据 → 单次 RTT 换 PAT → 落盘。
- **服务端**做四件事：验密码 → 生成随机 token → hash 后入库 → 把明文**唯一**那一次返回。
- 明文 PAT **不存 DB**，落地后 CLI 也只是把它放进一个 `0600` 的 toml 文件里。

为什么不用 session cookie？为什么用 token 不用 OAuth？为什么是 32 字节而不是 64？这些选择背后都有具体取舍，下文逐个拆。

---

## 1. PAT 是什么？格式、生命周期、它和 Session 的区别

### 1.1 PAT vs Session vs API Token：三种凭据各管一块

SwarmHive 的鉴权域里有三种长期 / 中期凭据，都在 [`auth/principal.rs`](../../crates/swarmhive-server/src/auth/principal.rs) 的 `AuthMethod` 枚举里统一表达：

| 凭据 | 谁在用 | 存在哪 | 撤销方式 | 权限模型 |
|---|---|---|---|---|
| **Session cookie** | Admin SPA 浏览器 | HttpOnly cookie + Postgres `session` 表 | 删 session 行 / cookie 过期 | live（每请求拉权限） |
| **PAT** | CLI 个人开发者 | `~/.config/swarmhive/credentials.toml` (`0600`) | 设 `revoked_at` | **live** — 跟随 owner 当前角色 |
| **API Token** | CI/CD 流水线 | env `SWARMHIVE_TOKEN` | 设 `revoked_at` | **snapshot** — 与 creator 解耦 |

三种凭据共用同一个 `Principal` 抽象，但来源不同。让 `bearer::resolve` 这一处统一校验，handler 不感知具体凭据类型。

> 🧠 **为什么不上 JWT?**
>
> SwarmHive 是单 binary monolith，JWT 的 stateless 优势（无中心化校验、跨服务传播）此处全部失效。代价反而上头：
> - **撤销难**：要么写黑名单（破坏 stateless）、要么短 TTL + refresh（CLI 不好用）
> - **scope 重发**：用户角色变了要重发新 JWT，CLI 没法主动配合
> - **公钥分发**：自托管场景每加一个 verifier 都要把公钥分发过去
>
> 长期 token + blake3 hash 入 DB，撤销立即生效、scope 实时同步、没有公私钥管理 —— 这是 self-host 单 binary 场景的甜区。

### 1.2 PAT 的字节结构

[`auth/token.rs:1-23`](../../crates/swarmhive-server/src/auth/token.rs#L1-L23) 把格式钉死成：

```text
swhv_pat_<43 char base64url-no-pad>
└┬─┘ └┬┘ └────────────────┬─────────────────┘
 │    │                    └── 32 字节 OsRng → URL_SAFE_NO_PAD 编码 = 43 字符
 │    └── kind tag (pat | api)
 └── 项目品牌前缀 (固定)

总长 52 字符。前 12 字符 (`swhv_pat_AbC`) 作为公开 prefix 显示。
```

`token::mint` 的核心 12 行：

```rust
pub fn mint(kind: ApiTokenKind) -> (String, String, String) {
    let mut bytes = [0u8; PAYLOAD_BYTES];   // 32
    OsRng.fill_bytes(&mut bytes);            // OS CSPRNG
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let plain = format!("{}{}", kind_prefix(kind), payload);
    debug_assert_eq!(plain.len(), TOTAL_LEN);
    let prefix = plain[..DISPLAY_PREFIX_LEN].to_string();           // 12
    let hash = blake3::hash(plain.as_bytes()).to_hex().to_string(); // 64 hex
    (plain, prefix, hash)
}
```

四个数字背后都有取舍：

| 参数 | 选定 | 替代方案 / 为什么不选 |
|---|---|---|
| 随机 bytes | **32** (256 bit) | 16 bit (128) 防暴力够，但 32 给未来留余量；64 (512) 没必要、空间浪费 |
| 编码 | **base64url-no-pad** | 标准 base64 含 `+/` 路径不安全；hex 太长 (64 chars vs 43) |
| 显式 kind | **`pat` / `api` 嵌入前缀** | 不嵌入 → 日志泄漏后无法快速分类；嵌入便于 `grep -E 'swhv_(pat\|api)_'` |
| DB 列 | **`prefix VARCHAR(12)`** | 不存 → admin/CLI 列表无法辨识 token；存全字符 → 等于明文落库 |

### 1.3 入库的是 hash，不是明文

```rust
let hash = blake3::hash(plain.as_bytes()).to_hex().to_string();
```

[`api_token.rs:70-71`](../../crates/swarmhive-entity/src/api_token.rs#L70-L71)：

```rust
/// `blake3` hex of the plaintext token (64 chars). Unique index.
#[sea_orm(unique)]
pub token_hash: String,
```

为什么用 **blake3** 而不是 argon2 / bcrypt / sha256？

- **blake3 vs sha256**：blake3 更快、抗碰撞同级别、依赖更轻；本场景**不需要 KDF 慢化**（输入熵已是 256 bit，暴力跑慢化函数没意义）
- **blake3 vs argon2**：argon2 是给**低熵密码**用的、故意慢；token 是 256 bit 高熵，跑 argon2 就是每个请求白白烧 100 ms CPU
- **存 hex string 而不是 bytea**：长度翻倍（64 char vs 32 byte）但 grep / SQL 查询友好；现代列式压缩对 ASCII hex 友好；详见 [`backend.md`](../knowledge/backend.md) "不要做" 段

校验路径在 [`bearer.rs:39-51`](../../crates/swarmhive-server/src/auth/bearer.rs#L39-L51)：

```rust
let plain = header_value.strip_prefix("Bearer ")...;
let (parsed_kind, hash_hex) = token::parse(plain).ok_or(ApiError::Unauthorized)?;

let row = api_token::Entity::find()
    .filter(api_token::Column::TokenHash.eq(hash_hex))    // ← O(1) 唯一索引
    .one(db).await?
    .ok_or(ApiError::Unauthorized)?;
```

`token_hash` 上有 `#[sea_orm(unique)]` → Postgres 自动建唯一 B-tree → 查找 O(1)。

### 1.4 完整的 5 道校验关

[`bearer.rs:34-104`](../../crates/swarmhive-server/src/auth/bearer.rs#L34-L104) 是整个 PAT 的"门禁"：

```mermaid
flowchart TB
    A[收到 Bearer header] --> B{strip_prefix 'Bearer '}
    B -->|✗| F1[401]
    B -->|✓| C{parse swhv_*_43}
    C -->|✗| F1
    C -->|✓| D[blake3 hash plaintext]
    D --> E[SELECT api_token<br/>WHERE token_hash = ?]
    E -->|未找到| F1
    E -->|找到| G{kind == parsed_kind?}
    G -->|✗| F1
    G -->|✓| H{revoked_at IS NULL?}
    H -->|✗| F1
    H -->|✓| I{expires_at > now?}
    I -->|✗| F1
    I -->|✓| J{owner.status == Active?}
    J -->|✗| F1
    J -->|✓| K{kind?}
    K -->|pat| L[load_user_permissions LIVE]
    K -->|api| M[decode snapshot from JSON]
    L --> N[Principal 构造]
    M --> N
    N --> O[heartbeat last_used_at<br/>throttled 1/min]
```

5 道关一道接一道，**任何一道失败都是 401**——刻意不暴露失败原因（不告诉攻击者 token 存在但过期 vs token 根本不存在），减少枚举攻击面。

### 1.5 PAT vs API Token 在权限上是分叉的

最容易踩雷的设计选择：**PAT 走 live 权限，API Token 走 snapshot**。

```rust
let (permissions, auth_method) = match row.kind {
    api_token::ApiTokenKind::Pat => {
        let perms = service::load_user_permissions(db, owner.id).await?;
        (perms, AuthMethod::Pat { token_id: row.id })
    }
    api_token::ApiTokenKind::Api => {
        let perms = decode_snapshot_permissions(row.permissions.as_ref());
        (perms, AuthMethod::ApiToken { token_id: row.id, scope: Scope::None })
    }
};
```

效果：

| 场景 | PAT | API Token |
|---|---|---|
| Owner 给我加了 `release:publish` 角色 | **立即生效** | 不影响 |
| Owner 撤销了我的 `artifact:upload` | **立即收缩** | 不影响 |
| 我自己被 disable | **token 失效** | **token 失效**（同一道关） |
| Token 本身 revoke | 失效 | 失效 |

API Token 的 `permissions = Some(...)` 是创建时的**快照**，与 creator 解耦——CI 流水线最需要这个语义：**不会因为创建 token 的人被改了角色，CI 就突然全挂**。代价是创建时必须明确指定 subset，且不会随 creator 升级。

PAT 反过来：你是谁，你的 PAT 就是谁；你换角色，PAT 换角色。CLI 个人使用最自然——你不会想"我升职了但 CLI 还在用旧权限"。

#### 创建时的硬约束

[`services/token.rs:177-216`](../../crates/swarmhive-server/src/services/token.rs#L177-L216) 在 service 层强制：

```rust
fn validate_permissions(req: &CreateTokenRequest, creator_perms: &HashSet<PermissionName>) -> Result<(), ApiError> {
    match req.kind {
        ApiKindWire::Pat => {
            if req.permissions.is_some() {
                return Err(ApiError::Validation {
                    detail: "PAT must not carry an explicit permissions list ...".into(),
                });
            }
        }
        ApiKindWire::Api => {
            let Some(perms) = &req.permissions else {
                return Err(ApiError::Validation { ... });
            };
            let overbroad: Vec<&'static str> = perms.iter()
                .filter(|p| !creator_perms.contains(p))
                .map(|p| p.as_str()).collect();
            if !overbroad.is_empty() {
                return Err(ApiError::Validation {
                    detail: format!("permissions exceed creator's grant: {}", overbroad.join(", ")),
                });
            }
        }
    }
    Ok(())
}
```

**防权限提升**：API Token 的 `permissions ⊆ creator.permissions`，超额直接 422 + 列出超额项。这一道关让 "developer 创建一个 release-manager 权限的 token 转交给 CI" 变成不可能——除非 developer 自己先被升级成 release-manager。

---

## 2. `last_used_at` 节流：一条 SQL 解决竞态

PAT 鉴权每命中一次理论上都该更新 `last_used_at`，用户在 admin 看自己 token "上次使用 3 分钟前"。但每请求都 UPDATE 一次：

- 高 QPS 下写放大严重
- 多实例下没 cache 一致性
- 容易踩到 Postgres 行锁

SwarmHive 用一条 SQL 同时解决"节流 + first-use 审计 + 多实例一致" ([`bearer.rs:119-165`](../../crates/swarmhive-server/src/auth/bearer.rs#L119-L165))：

```rust
let stmt = Statement::from_sql_and_values(
    DatabaseBackend::Postgres,
    r#"
    UPDATE api_token
    SET last_used_at = NOW()
    WHERE id = $1
      AND (last_used_at IS NULL OR last_used_at < NOW() - INTERVAL '1 minute')
    "#,
    [row.id.into()],
);
let result = db.execute_raw(stmt).await?;
let first_use = was_null && result.rows_affected() > 0;
```

把节流条件写进 `WHERE` 子句：

- ≤1 次/分钟：超过窗口 0 行受影响，单 round-trip 直接返回
- 不需要 app-level 锁 / cache / 队列
- 多实例并发：Postgres 行锁串行化 WHERE 计算，永远只有一个赢家
- `rows_affected()` 返回 1 → 这次是真写了

**最妙的副作用**：`was_null && rows_affected > 0` 正好是"NULL → Some 转换"的判据——也就是"这个 token 首次被使用"。立刻写一条 `auth:token_used_first_time` 审计：

```rust
if first_use {
    audit::write(db, AuditEntry {
        actor_type: audit_log::ActorType::Token,
        action: "auth:token_used_first_time".into(),
        // ...
    }).await?;
}
```

这是 SwarmHive 安全模型的一个关键指标：**token 被发出后第一次"被实际拿来用"是什么时候、从什么 IP**。Owner 收到这条审计 alert，能马上判断"这是我刚才在 CI 设的那个 token，还是被人偷走了？"

---

## 3. 客户端落盘：`~/.config/swarmhive/credentials.toml`

服务端把明文返回后，CLI 立刻落盘。[`credentials.rs:23-71`](../../crates/swarmhive-cli/src/credentials.rs#L23-L71) 干三件事：

### 3.1 跨平台路径用 `directories::ProjectDirs`

```rust
pub fn path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "swarmhive", "swarmhive")
        .context("could not determine user config directory")?;
    Ok(dirs.config_dir().join("credentials.toml"))
}
```

各平台落到：

| 平台 | 路径 |
|---|---|
| Linux | `~/.config/swarmhive/credentials.toml` |
| macOS | `~/Library/Application Support/dev.swarmhive.swarmhive/credentials.toml` |
| Windows | `%APPDATA%\swarmhive\swarmhive\config\credentials.toml` |

直接 `dirs.config_dir()` 比手写 `if cfg!(target_os = ...)` 优雅得多——XDG 规范、Apple SDK 风格、Windows 标准 ProgramData 都被一道封了。

### 3.2 强制 `0600` (Unix)

```rust
#[cfg(unix)]
fn set_owner_read_write_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        tracing::warn!(?err, path = %path.display(), "failed to chmod credentials file to 0600");
    }
}
```

这一刀阻止两件事：

- **同机多用户**互相偷凭据（共享开发服务器最常见）
- **进程 dump / coredump** 被其他用户读取

Windows 路径放弃了——文件系统 ACL 模型完全不同，且 Windows 用户 profile 默认就有强 ACL。SwarmHive 选择 `tracing::debug!` 记一下，不强行折腾 ACL API。

### 3.3 TOML 格式（不是 JSON）

```toml
server = "http://localhost:3030"
email = "alice@example.com"
token = "swhv_pat_AbC..."
```

为什么 TOML？

- 人眼**直接可读可编辑**——出问题用户能自己看
- 多注释友好（JSON 不允许注释）
- Rust 生态原生支持 (`toml` crate 在 workspace 已有)
- 文件极小（< 200 字节），不在乎序列化效率

不放 JSON 还有一层小优势：避免和 `package.json` / `tsconfig.json` 这种典型项目文件混淆，开发者一眼就知道这是配置不是 JS 工程产物。

---

## 4. 完整链路时序图

把所有部分串起来——`swarmhive login` 之后下一条 `swarmhive publish` 的鉴权路径：

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as CLI
    participant FS as credentials.toml
    participant SRV as Server
    participant DB as Postgres

    Note over U,DB: ─── 阶段 1: swarmhive login ───
    U->>CLI: swarmhive login http://srv --email alice@x
    CLI->>U: Password: (rpassword 无回显)
    CLI->>CLI: default_token_name = "host-1234567"
    CLI->>SRV: POST /api/v1/auth/cli-token<br/>{email, password, token_name}
    SRV->>DB: verify_password (argon2id)
    SRV->>DB: load_user_permissions(uid)
    SRV->>SRV: token::mint(Pat)<br/>= (plain, prefix, hash)
    SRV->>DB: INSERT api_token<br/>(kind=pat, hash=blake3, prefix, perms=NULL)
    SRV->>DB: audit auth:token_created
    SRV-->>CLI: {token: "swhv_pat_xxx", name, created_at}
    CLI->>FS: toml + chmod 0600
    CLI-->>U: Logged in. Path: ~/.config/swarmhive/credentials.toml

    Note over U,DB: ─── 阶段 2: 后续命令鉴权 ───
    U->>CLI: swarmhive publish tauri
    CLI->>CLI: auth::resolve(config_server)
    Note over CLI: env SWARMHIVE_TOKEN<br/>else credentials.toml
    CLI->>SRV: Authorization: Bearer swhv_pat_xxx
    SRV->>SRV: bearer::resolve
    Note over SRV: parse → blake3 → 5 道门禁
    SRV->>DB: SELECT api_token WHERE token_hash=?<br/>(O(1) 唯一索引)
    SRV->>DB: SELECT user WHERE id=owner_id
    SRV->>DB: load_user_permissions LIVE (PAT)
    SRV->>DB: UPDATE last_used_at<br/>(WHERE 节流 ≤1/min)
    alt 首次使用
        SRV->>DB: audit auth:token_used_first_time
    end
    SRV->>SRV: 构造 Principal
    SRV-->>CLI: 200 + business response

    Note over U,DB: ─── 阶段 3: swarmhive logout ───
    U->>CLI: swarmhive logout
    CLI->>FS: load credentials
    CLI->>SRV: GET /api/v1/tokens (with Bearer)
    SRV-->>CLI: [token list]
    CLI->>CLI: find prefix == token[..12]
    CLI->>SRV: DELETE /api/v1/tokens/{id}
    SRV->>DB: UPDATE revoked_at = now()
    SRV->>DB: audit auth:token_revoked
    SRV-->>CLI: 200
    CLI->>FS: delete credentials.toml
    CLI-->>U: Token revoked
```

---

## 5. 业界主流 8 种 CLI 鉴权方案对比

光看 SwarmHive 自己的设计还不够。下面横向对比 8 种最常见的 CLI 鉴权方案，看看每家在哪条曲线上做的取舍。

### 5.1 GitHub CLI (`gh auth login`) —— OAuth Device Flow

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as gh CLI
    participant GH as GitHub
    participant BR as Browser

    CLI->>GH: POST /login/device/code
    GH-->>CLI: { device_code, user_code, verification_uri }
    CLI->>U: "Visit github.com/login/device, enter ABCD-EFGH"
    U->>BR: 打开 URL, 输入 code
    BR->>GH: 登录 + 授权
    loop 每 5s
        CLI->>GH: POST /login/oauth/access_token<br/>(grant_type=device_code, device_code)
        GH-->>CLI: pending / authorization_pending
    end
    GH-->>CLI: { access_token, token_type, scope }
    CLI->>CLI: 存 ~/.config/gh/hosts.yml<br/>或 OS keyring
```

**优点**：

- 用户**无需把密码输进 CLI**——浏览器走标准 OAuth 登录，支持 2FA / SSO / passkey
- token 自动带 OAuth scope（`repo`、`workflow`、`gist`...）
- 撤销在 GitHub 网页 Settings 里集中管理

**缺点**：

- 需要**浏览器**——纯 SSH 跳板机 / Docker 容器场景需要复制 URL 到本机浏览器
- 实现复杂：要轮询 / 处理 `slow_down` / 处理 `authorization_pending`
- 自托管不友好：需要自己实现完整 OAuth Provider

**SwarmHive 为什么没用**：self-host MVP 阶段引入 OAuth 是大工程，且单 admin 用户场景下浏览器跳转反而麻烦。OAuth 留给后续 `add-oauth-github` proposal。

### 5.2 AWS CLI (`aws configure`) —— Access Key + Secret

```toml
# ~/.aws/credentials
[default]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

# ~/.aws/config
[default]
region = us-east-1
output = json
```

**优点**：

- 极简：两个字符串，零状态
- 长期有效，CI 友好
- SigV4 签名机制，请求体也被签——防重放
- 多 profile 一文件搞定

**缺点**：

- **明文落盘**——任何能读 `~/.aws/credentials` 的进程都能拿到你的 AWS 账号控制权
- access key 一旦泄漏只能手动 rotate
- 后来 AWS 自己也意识到不好，推 **SSO** / **IAM Identity Center** / **role assume** 等更短期的方案

**SwarmHive 借鉴**：单文件 + multi-profile 模式很优雅。SwarmHive credentials.toml 目前只支持单 profile，未来 `[profile.prod]` 这种语法很容易扩展。

### 5.3 Google Cloud CLI (`gcloud auth login`) —— OAuth + Refresh Token

```mermaid
sequenceDiagram
    participant CLI as gcloud
    participant BR as Browser
    participant G as Google OAuth

    CLI->>BR: 启动浏览器 + 监听 localhost:8085
    BR->>G: OAuth Consent Screen
    G->>BR: redirect → localhost:8085/?code=xxx
    BR->>CLI: code 回流
    CLI->>G: POST /token<br/>(grant_type=authorization_code, code, ...)
    G-->>CLI: { access_token, refresh_token, expires_in }
    CLI->>CLI: 存 ~/.config/gcloud/application_default_credentials.json

    Note over CLI,G: ─── 每次调用 API ───
    CLI->>G: 用 access_token
    alt access_token 过期
        CLI->>G: POST /token (grant_type=refresh_token)
        G-->>CLI: 新的 access_token
    end
```

**优点**：

- 短期 access_token (60min) + 长期 refresh_token
- access_token 即使泄漏窗口期也只有 1 小时
- 与 Google 账号、IAM、Workload Identity Federation 无缝
- 服务端可以**整体 revoke refresh_token**

**缺点**：

- **CLI 本身要长期持有 refresh_token**——一旦泄漏几乎等于完整账号
- 实现复杂：要管 token 缓存、过期、并发 refresh、429 退避
- 离线极不友好（refresh 也要联网）

**SwarmHive 没用**：MVP 阶段加 refresh 逻辑收益小于成本——PAT 一个长期 token 已经够用。未来如果支持企业 SSO，refresh 逻辑可能落到 OAuth provider 上。

### 5.4 npm (`npm login`) —— Bearer Token in `.npmrc`

```ini
# ~/.npmrc
//registry.npmjs.org/:_authToken=npm_AbCdEf...
@myorg:registry=https://npm.example.com/
//npm.example.com/:_authToken=internal_token_xxx
```

**优点**：

- **per-registry 独立 token**——多 registry 场景天然支持
- 同 `.npmrc` 还能放 scope 配置
- token 格式带前缀（`npm_xxx`、`npmrc_legacy_xxx`），泄漏 grep 友好

**缺点**：

- INI 格式没人喜欢
- 文件权限**不强制**，默认普通用户可读
- 长期 token 没有过期机制

**SwarmHive 对照**：npm 的 token 前缀策略和 SwarmHive 几乎一模一样（`swhv_pat_` vs `npm_`），都是 GitHub 在 2021 年的"secret scanning friendly token" 风潮里学的。

### 5.5 Docker CLI (`docker login`) —— Credentials Helper

```json
// ~/.docker/config.json
{
  "auths": {
    "https://index.docker.io/v1/": {}
  },
  "credsStore": "osxkeychain"
}
```

**优点**：

- **凭据不落盘**——`credsStore` 委托 OS keyring（macOS Keychain、Windows Credential Manager、Linux secretservice）
- helper protocol 极简：stdin 写 server URL，stdout 拿 `{Username, Secret}` JSON
- 支持企业 / 私有 helper（如 `docker-credential-ecr-login` 走 AWS SDK 即时签）

**缺点**：

- 仍然 fallback 到 `auths` 字段的 base64 明文（"base64 不是加密"，但很多用户不知道）
- helper 协议跨平台兼容性还行但有边角 case
- 配置文件本身没格式校验，错改一行 docker 整个挂

**SwarmHive 借鉴**：OS keyring 集成是个未来方向。目前 MVP 直接落盘 + `0600` 已经超过 docker 默认行为。后续可以加 `credsStore = "keyring"` 这种可选模式。

### 5.6 Cargo (`cargo login`) —— Plain Token

```toml
# ~/.cargo/credentials.toml
[registry]
token = "cio..."

[registries.my-registry]
token = "..."
```

**优点**：

- 简单到不能再简单
- multi-registry 一文件
- 文件存的就是明文 bearer，HTTP header 直接 `Authorization: Bearer <token>`

**缺点**：

- 明文落盘
- 没有 token 区分前缀（早期 crates.io 用 `cio` 前缀，不强制）
- 无过期 / scope

**SwarmHive 对照**：Cargo 的极简风格和 SwarmHive 客户端实现几乎一致——一个 toml，一个 token，go。差别只在前缀强度和服务端 hash 策略。

### 5.7 kubectl —— ExecCredential / Service Account Token

```yaml
# ~/.kube/config
users:
- name: cluster-admin
  user:
    exec:
      apiVersion: client.authentication.k8s.io/v1beta1
      command: aws
      args: [eks, get-token, --cluster-name, my-cluster]
```

**优点**：

- 把鉴权委托给**外部进程**——cloud provider / IDP 都可以接入
- token 短期有效（IAM 默认 15min）
- 支持复杂场景：x509 client cert、bearer token、exec、OIDC、impersonation

**缺点**：

- 配置极复杂
- exec plugin 容易出问题（PATH 不对、plugin 版本不匹配）
- 各 cloud 的 exec plugin 都自己一套

**SwarmHive 不适合**：kubectl 这种"鉴权委托"模式适合多源凭据（个人 / CI / cloud SDK），SwarmHive MVP 单一 PAT 源不需要这层抽象。

### 5.8 Stripe CLI (`stripe login`) —— Device Pairing

```mermaid
sequenceDiagram
    participant CLI as stripe
    participant SRV as Stripe API
    participant BR as Browser

    CLI->>SRV: POST /v1/cli/login_links
    SRV-->>CLI: { url, pairing_code }
    CLI->>BR: open url + display pairing_code
    BR->>SRV: 登录, 确认 pairing_code 匹配
    loop 轮询
        CLI->>SRV: GET /v1/cli/login_links/{id}
        SRV-->>CLI: pending / paired
    end
    SRV-->>CLI: { api_key, mode: test/live }
    CLI->>CLI: 存 ~/.config/stripe/config.toml
```

**优点**：

- 类似 OAuth device flow，但**专为 CLI 设计**
- pairing code 是显式确认——防止中间人偷链接
- 一次登录可以发不同 scope token (test/live)

**缺点**：

- 仍然需要浏览器
- 自托管复杂度高
- pairing UX 需要打磨（用户经常困惑要不要确认 code）

### 5.9 总览对比表

| 维度 | SwarmHive PAT | gh OAuth | aws keys | gcloud OAuth | npm | docker | cargo | kubectl | stripe |
|---|---|---|---|---|---|---|---|---|---|
| 浏览器需求 | ✗ | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | ✓ |
| Token 类型 | 长期 PAT | OAuth access | 长期 access key | access + refresh | 长期 token | 多 helper | 长期 token | 多种 | API key |
| 落盘形式 | toml `0600` | yaml + keyring | ini | json | ini | json + keyring | toml | yaml + exec | toml |
| 服务端存储 | blake3 hash | hash | hash | hash | hash | hash | hash | varies | hash |
| 撤销立即生效 | ✓ (设 revoked_at) | ✓ | ✓ | ✓ refresh | ✓ | ✓ | ✓ | varies | ✓ |
| 过期 | 可选 | 短期 | 永不 | 短期 + refresh | 永不 | 永不 | 永不 | 短期 | 永不 |
| Scope 控制 | verb-scoped | OAuth scope | IAM policy | IAM | publish/read | repo perm | repo | RBAC | livemode/scope |
| Live 权限 | ✓ (PAT) | ✗ | live | live | ✗ | ✗ | ✗ | live | ✗ |
| CI 友好 (env) | ✓ | ✓ | ✓ | ✓ + ADC | ✓ | ✓ | ✓ | ✓ | ✓ |
| Self-host 友好 | ✓ | △ 需 OAuth provider | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ |
| 实现复杂度 | 低 | 高 | 极低 | 高 | 低 | 中 | 极低 | 高 | 高 |

### 5.10 该用哪种？决策树

```mermaid
flowchart TD
    A[要给 CLI 加鉴权] --> B{自托管 / 内部工具?}
    B -->|是| C{用户量?}
    C -->|< 100| D[SwarmHive / Cargo 风格<br/>简单 PAT toml]
    C -->|> 100, 多组织| E[OAuth + IDP 集成<br/>gh 风格]
    B -->|否, SaaS| F{对接广度?}
    F -->|开发者工具 GitHub/Stripe| G[OAuth device flow<br/>+ 短期 token]
    F -->|infra/cloud| H[access key + signature<br/>aws / gcloud 风格]
    F -->|registry/package| I[per-registry token<br/>npm / cargo 风格]

    D --> J{是否需要 CI?}
    J -->|是| K[加 API Token snapshot 模式<br/>SwarmHive 双轨]
    J -->|否| L[纯 PAT 够用]
```

**SwarmHive 走右上角 → 中间右 → 加 CI**：

- self-host + 用户量小 → 简单 PAT
- CI 有 → 在同表加 `kind=api` 双轨，snapshot 权限独立

这是 GitHub 早期、GitLab、Sentry、Linear、Vercel 等绝大多数中型 SaaS 走过的路径。简单可靠，未来再加 OAuth 也不会推翻——OAuth 只是多一种"换出 PAT 的方式"。

---

## 6. SwarmHive 选择背后的具体取舍清单

这一节把上面所有对比浓缩成一张"为什么"清单，方便你抄进自己项目的设计文档。

### 6.1 为什么是 PAT 不是 OAuth

| 评估维度 | PAT | OAuth |
|---|---|---|
| 自托管复杂度 | 低 — 一张表 | 高 — provider + 公私钥 + redirect |
| 浏览器需求 | 无 | 需要 |
| CLI 实现 | 200 行 | 1000+ 行 (含 device flow / token refresh / 错误处理) |
| 用户量 < 100 | 完全够用 | 杀鸡用牛刀 |
| 集成 IDP (SSO) | 需要补 OAuth provider | 天然 |

**结论**：MVP 阶段 PAT 完胜。未来若有企业 SSO 需求，在 PAT 之上加 OAuth 不冲突。

### 6.2 为什么是 blake3 不是 argon2

| 哈希函数 | 速度 | 适用对象 |
|---|---|---|
| argon2id | 慢（百毫秒级，有意为之） | 低熵密码 |
| sha256 | 快 | 高熵 token，但依赖较重 |
| **blake3** | **极快**（GB/s 级） | **高熵 token**，依赖轻 |

**结论**：256 bit 高熵输入不需要慢化函数；blake3 速度 + 轻量取胜。

### 6.3 为什么是 32 字节 + base64url 而不是 16 字节 + hex

- 32 字节 (256 bit) 是 NIST 推荐的"long-term key" 强度
- base64url 比 hex 短 25%（43 vs 64 字符）—— CLI 用户复制粘贴友好
- `URL_SAFE_NO_PAD` 避免 URL 转义问题 + 长度固定（43 字符）

### 6.4 为什么前缀里嵌 kind (`swhv_pat_` / `swhv_api_`)

GitHub 2021 年推 "secret scanning friendly token" 之后业界共识：

- **泄漏后快速分类**：GitHub Actions 看到 `swhv_pat_` 可以提示用户"这是 PAT，立即去 https://swarmhive.dev/settings/tokens 撤销"
- **grep 友好**：`grep -rE 'swhv_(pat|api)_' .` 一键扫整个项目
- **kind 验证**：[`bearer.rs:53-57`](../../crates/swarmhive-server/src/auth/bearer.rs#L53-L57) 校验 `row.kind == parsed_kind`，防止哈希碰撞或 DB 错位

### 6.5 为什么 PAT live 权限、API Token snapshot

- **PAT 跟人**：你升职了 PAT 自动升级；你被开了 PAT 立刻失效
- **API Token 跟脚本**：CI 配置一次别天天改；creator 角色变动不应该让 CI 突然挂

这是 Sentry / Linear / Vercel 等公司"两轨长期 token"的标准做法。

### 6.6 为什么 last_used_at 用 SQL WHERE 节流

| 替代方案 | 缺点 |
|---|---|
| App-level cache (LRU) | 多实例不一致；重启丢；内存压力 |
| 单独 throttle 表 | 多一张表 + 多一次 query |
| 队列异步写 | 增加运维复杂度；可能丢消息 |
| **SQL WHERE 条件** | **单 round-trip，无 race，无外部依赖** |

加 bonus："NULL → Some" 转换自然导出 "first-use" 事件——一条 SQL 同时解决三个问题。

### 6.7 为什么撤销是 best-effort

[`commands/logout.rs:25-32`](../../crates/swarmhive-cli/src/commands/logout.rs#L25-L32):

```rust
match revoke_remote(&creds).await {
    Ok(true) => println!("Token revoked on {}", creds.server),
    Ok(false) => println!("Token not found on {} (already revoked or rotated)", creds.server),
    Err(err) => eprintln!(
        "warning: failed to revoke token on {}: {err:#}\nRemoving local file regardless.",
        creds.server
    ),
}
let removed = Credentials::delete()?;
```

如果**先删本地、再撤远端**：远端失败时本地已删，用户**无法重试撤销**（没 token 了）。

如果**严格要求远端先成功**：远端宕机时用户**永远登不出**（本地 token 还在但用户认为已 logout）。

折中：**远端尽力撤、本地必删**。最坏情况：远端 token 还活着但本地 token 文件已没了——再 login 一次即可，最多服务端多挂一个未使用的 token，下次 `swarmhive token list` 用户能看到。

---

## 7. 可以抄回去用的几个核心 idea

总结一下 SwarmHive PAT 链路里**可以直接复用到任何自托管 CLI 项目**的部分：

### 7.1 Token 字节模式：`<brand>_<kind>_<base64url-32B>`

```text
swhv_pat_AbCdEf...   ← 52 字符，前 12 字符做公开 prefix
```

借鉴 GitHub 2021 secret-scanning 风潮。比"裸 UUID"或"用户名:密码"都好。

### 7.2 hash 落库，明文仅返回一次

```rust
let hash = blake3::hash(plain.as_bytes()).to_hex().to_string();
INSERT INTO api_token (token_hash, prefix, ...) VALUES ($1, $2, ...)
RETURNING ...

// 响应里只在创建时带 plaintext, 之后任何 GET 都不返回
```

DB 被 dump 也无法还原 token。

### 7.3 双轨 token：PAT live + API snapshot

```rust
match kind {
    Pat => load_user_permissions_live(),
    Api => decode_snapshot(row.permissions),
}
```

满足"CLI 用户跟人"和"CI 脚本跟代码"两类不同需求。

### 7.4 `last_used_at` 写进 WHERE 子句

```sql
UPDATE api_token
SET last_used_at = NOW()
WHERE id = $1
  AND (last_used_at IS NULL OR last_used_at < NOW() - INTERVAL '1 minute')
```

单 SQL 解决 throttle + first-use 双重需求。

### 7.5 env 优先于配置文件

```rust
let token = env::var("SWARMHIVE_TOKEN").or_else(|| file_token);
```

CI 注入 env 不需要任何"登录"步骤。

### 7.6 logout best-effort 远端 + 严格本地

远端撤销失败不阻塞本地清理，让"下次 login 从干净状态开始"优先级压过"严格一致"。

### 7.7 前 12 字符做 `prefix` 列

```text
swhv_pat_AbC  ← 公开
[hash]        ← 内部
```

允许 admin/CLI 列出"我有哪些 token"而不暴露 secret，且 logout 可以通过 prefix 反查 id。

---

## 结语

CLI 鉴权听上去简单，真要做对——格式、过期、撤销、节流、节流后审计、本地存哪、文件权限、env 优先级、CI 怎么用、私有 CA 怎么过、离线登出怎么办——每一个都是一道独立题。

SwarmHive 的选择不是"最酷"的，但是"最可拆"的：

- 单表两 kind（PAT + API Token）满足 95% 自托管场景
- blake3 + base64url + 双前缀让安全和体验都过得去
- SQL 一行解决 throttle/audit
- env > file 让 CI 零摩擦

如果你正在为自己的工具做这件事，建议路径：

1. **MVP**：抄 SwarmHive 模式，单 PAT 表 + toml 落盘
2. **加 CI**：表内加 `kind=api`，snapshot 权限
3. **大用户量 / SSO**：在 PAT 之外加 OAuth provider，PAT 不下线
4. **企业**：加 keyring 集成 + 短期 token + refresh

每一步都不推翻前面，每一步都对得起当时的需求。

代码入口（顺着读最容易理清）：

- [`crates/swarmhive-cli/src/commands/login.rs`](../../crates/swarmhive-cli/src/commands/login.rs) — 客户端登录
- [`crates/swarmhive-cli/src/credentials.rs`](../../crates/swarmhive-cli/src/credentials.rs) — 本地落盘 + 0600
- [`crates/swarmhive-cli/src/auth.rs`](../../crates/swarmhive-cli/src/auth.rs) — env > file 优先级
- [`crates/swarmhive-cli/src/commands/logout.rs`](../../crates/swarmhive-cli/src/commands/logout.rs) — 远端 best-effort + 本地清理
- [`crates/swarmhive-server/src/auth/token.rs`](../../crates/swarmhive-server/src/auth/token.rs) — mint / parse / hash
- [`crates/swarmhive-server/src/auth/bearer.rs`](../../crates/swarmhive-server/src/auth/bearer.rs) — 5 道门禁 + last_used_at 节流
- [`crates/swarmhive-server/src/services/token.rs`](../../crates/swarmhive-server/src/services/token.rs) — create/revoke/list + 权限校验
- [`crates/swarmhive-server/src/routes/auth.rs`](../../crates/swarmhive-server/src/routes/auth.rs) — `/auth/cli-token` endpoint
- [`crates/swarmhive-entity/src/api_token.rs`](../../crates/swarmhive-entity/src/api_token.rs) — 表结构 + 双 kind 不变式

进一步阅读：

- [dev-notes/knowledge/backend.md](../knowledge/backend.md) "鉴权" 段 — 项目内权威设计记录
- [dev-notes/blog/2026-05-29-oss-release-artifact-pipeline.md](2026-05-29-oss-release-artifact-pipeline.md) — 上传链路如何用 PAT 鉴权
- [docs/13-rbac.md](../../docs/13-rbac.md) — verb-scoped 权限矩阵

如果你打算在自己的 CLI 里复刻这套，最容易踩雷的两件事我直接挑明：

1. **不要把明文 token 落进 DB**——一次泄漏所有 token 都得 rotate
2. **不要每个请求都同步写 `last_used_at`**——高 QPS 下写放大严重；SQL WHERE 节流是最省事的方案

祝你的 CLI 鉴权不再让用户填问卷。
