# design

## Context

`add-auth-and-rbac` 当年用 setup_token 模型把 Owner bootstrap 收口：server 首次启动 stdout 打印一次性 token，sysadmin 拿 token POST `/api/v1/setup` 创建 Owner。安全模型严：暴露窗口几乎为零（只有看 stdout 的人能拿）。

但作为一个 self-host 发布分发系统，目标用户的"部署即用"期望更接近 Coolify / Plausible / Vaultwarden web 模式 —— docker run 完直接打开 web 完成初始化，不必再开终端。setup_token 流程在这种 UX 下是隐性门槛。

同时 admin SPA 的 `/login` 是占位（Card + Alert "尚未实现"），整个登录链路在 web 端实际是"不可用"。两个 gap 必须一起补，否则部署完没有任何 web-only 路径能动起来。

约束：

- **单组织 + 完整 RBAC**（[docs/13-rbac.md](../../../docs/13-rbac.md)）—— bootstrap 出来的 Owner 一定是 org 唯一 Owner
- **AntD 6 + Pro Components 是唯一 UI**（admin-frontend-foundation 已锁定）
- **i18n-ready zh-CN MVP**（Lingui v6，新 page 必须 `<Trans>` 全包裹）
- **self-host 主旨** —— 不接外部服务（CAPTCHA / HIBP / Sentry）
- **OpenAPI drift gate** —— server endpoint 改动必须同步 regen admin `schema.gen.ts`
- **不引入新 auth lib** —— 沿用自写 Principal extractor + argon2 同步路径

## Goals / Non-Goals

**Goals:**

- Web 端 `/setup` + `/login` 是 admin SPA 内开箱可用的入口，无 ssh / 无终端
- Bootstrap 模型符合 Coolify 主流 UX，同时通过 ENV 锁定提供"轻防护"层
- 账号级失败锁定（5/30min 软锁）作为 baseline 安全补丁，跟现有 tower-governor 请求级限流并存
- 后续 ②③④⑤ 都能基于本 proposal 的 `/login` 容器 + bootstrap window 守门做扩展

**Non-Goals:**

- 不实现忘记密码 / 重置密码（依赖 mail，留 ④）
- 不实现 OAuth 按钮 / 入口（依赖 oauth_provider 表 + admin 配置 UI，留 ③）
- 不实现 email 验证 / pending_verify / pending_approval（依赖 mail + policy，留 ⑤）
- 不实现 MFA / 2FA / HIBP
- 不重写 archived auth proposal 的 spec —— 用 ADDED Requirements 覆盖即可

## Decisions

### 1. Bootstrap 模型：Coolify 式 + 可选 ENV 锁定

```text
Server 启动
   │
   ▼
读 ENV SWARMHIVE_BOOTSTRAP_OWNER_EMAIL（Option<String>）
   │
   ▼
查 user 表 count
   │
   ├─ count > 0 → 永久 bootstrap_complete = true
   │
   └─ count = 0 → bootstrap_complete = false

GET /api/v1/setup/info
   { needs_bootstrap: !bootstrap_complete,
     locked_email: SWARMHIVE_BOOTSTRAP_OWNER_EMAIL 当 needs_bootstrap=true 时 }

POST /api/v1/setup { email, display_name, password }
   ├─ bootstrap_complete → 410 problem+json type=bootstrap_already_complete
   ├─ locked_email.is_some() && req.email != locked_email
   │     → 422 problem+json type=bootstrap_email_mismatch
   ├─ password 未过强度校验 → 422 problem+json type=password_too_weak
   ├─ INSERT user + user_credentials + 默认 owner role → 写 session cookie → 200
   └─ INSERT 后 bootstrap_complete 自动转 true（下次请求即 410）
```

**为什么删 setup_token 不保留双轨**：双轨意味着 `/setup` UI 要先判断"当前 server 是 token 模式还是 open 模式"，分支多、文档复杂、决策疲劳。Coolify 实践证明 ENV 锁定 + 文档警告对 99% 自部署场景已够。

**为什么用 user 表 count 而不是单独的 bootstrap_status 表**：user 表是 source of truth；引单独表会产生"user 已创建但 bootstrap_complete=false"的不一致态。

**为什么 locked_email 字段而非更复杂的 owner_token**：ENV 注入 email 比 token 更"轻"，docker compose / helm chart 配 ENV 是标准实践；token 还要解决"怎么传给前端不被中间人截"的问题。

### 2. 账号级软锁：user_login_attempts 表

```rust
#[sea_orm::model]
pub struct Model {
    #[sea_orm(primary_key)]
    user_id: Uuid,
    failed_count: i32,           // 累计失败
    last_failed_at: DateTimeUtc,
    locked_until: Option<DateTimeUtc>, // 锁定截止
    updated_at: DateTimeUtc,
}
```

```text
POST /api/v1/auth/login { email, password }
   │
   ▼
SELECT user WHERE email = ?
   │
   ├─ Not found → 200ms argon2 dummy verify（timing 等长）→ 401 invalid_credentials
   │
   └─ Found → SELECT user_login_attempts WHERE user_id
        │
        ▼
        locked_until > now() → 410 account_locked_until { locked_until: <ts> }
        │
        ▼
        argon2 verify password
           │
           ├─ OK → DELETE user_login_attempts → 写 session → 200
           │
           └─ Fail → UPSERT failed_count += 1, last_failed_at = now()
                      ├─ failed_count >= 5 → set locked_until = now() + 30min
                      └─ 401 invalid_credentials
```

**为什么 per-user 锁而非 per-IP**：per-IP 锁会被 NAT / 反代误伤；per-user 锁直接对应攻击模型（password spraying / brute force per account）。请求级限流（tower-governor，已就绪）兜 IP 维度。

**为什么 30min 锁而非递增 backoff**：实现简单 + 用户预期一致（"等半小时再试"是直觉）；递增锁需要额外字段（attempt_window_start 等）增加 schema 复杂度。

**为什么单表而非合到 user 表**：login_attempts 是高频写（每次失败 update），user 是低频写（仅 profile 改动）；分表减少 user 表的写锁竞争 + 让 user 表的 audit log diff 干净。

### 3. 密码强度：garde + 内置弱口令字典

```rust
#[derive(garde::Validate)]
struct SetupRequest {
    email: String,
    display_name: String,
    #[garde(custom(validate_strong_password))]
    password: String,
}

fn validate_strong_password(pwd: &str, _: &()) -> garde::Result {
    if pwd.len() < 12 { return Err(garde::Error::new("password_too_short")); }
    let classes = count_classes(pwd); // upper / lower / digit / special
    if classes < 3 { return Err(garde::Error::new("password_not_diverse")); }
    if WEAK_PWDS.contains(pwd) { return Err(garde::Error::new("password_in_weak_list")); }
    Ok(())
}
```

**为什么 garde 不引 zxcvbn**：zxcvbn-rs ~500KB bundle，对 server 也是无谓负担；NIST 800-63B 推荐的"长度 + 多样性 + 弱口令字典"已经覆盖 95% 风险。

**为什么 admin SPA 也写一份 zod**：前端即时反馈 UX；后端 garde 是 source of truth（即使前端绕过也拦得住）。两套规则保持一致（文档化 + 单测）。

**弱口令字典哪里来**：embedded `top-100.txt`（"password", "123456", "qwerty" 等），来源 SecLists；server crate `include_bytes!` 嵌入，零运行时下载。

### 4. Admin SPA 路由分流

```text
__root.tsx beforeLoad:
  - 调 setupInfoQueryOptions()（缓存 + staleTime 60s）
  - 若 needs_bootstrap:
      - 当前 path != '/setup' → throw redirect({ to: '/setup', replace: true })
  - 若 !needs_bootstrap:
      - 当前 path === '/setup' → throw redirect({ to: '/login', replace: true })

routes/setup.tsx:
  - 顶层路由（无 _auth guard）
  - beforeLoad 双重确认 needs_bootstrap=true（防 race）
  - ProForm: email (locked_email 时 disabled) + display_name + password + confirm

routes/login.tsx:
  - 顶层路由（无 _auth guard）
  - ProForm: email + password + 记住我 + "忘记密码"占位（disabled）
  - submit: fetchClient POST → 解 problem+json → 按 type 分支 UI
  - search params: { next?: string } 登录成功 → navigate to next ?? '/'

routes/_auth.*.tsx:
  - 保持 admin-frontend-foundation 已写的 beforeLoad（meQueryOptions）
  - 401 → redirect /login（不变）
```

**为什么 setup/login 用 `/setup`、`/login` 顶层而非 `/auth/setup`、`/auth/login`**：顶层 path 更短，跟 Coolify / Plausible / Outline 一致；admin SPA 不存在 multi-section auth namespace 需求。

### 5. ENV 配置约定

`SWARMHIVE_BOOTSTRAP_OWNER_EMAIL`：

- 空（默认）→ 完全开放，文档警告"部署后立即访问 web"
- 设值 → `/setup` email 字段固化为该值（disabled + 预填），mismatch 返 422
- 已 bootstrap 后该 ENV 失效（不影响已存在 user）

```toml
# config/dev.toml 注释示意
[bootstrap]
# 留空则任意 email 可成为 Owner（部署后第一个访问 web 的人）
# 设值则该 email 锁定为唯一可成为 Owner 的邮箱（推荐公网部署设此值）
# 也可通过 ENV 覆盖：SWARMHIVE_BOOTSTRAP_OWNER_EMAIL=me@example.com
# owner_email = ""
```

## Risks / Trade-offs

- **[公网部署裸 Coolify 模式的 owner 抢注风险]** → ENV `BOOTSTRAP_OWNER_EMAIL` + 部署文档强调首启即访问 web。MVP 接受这个 trade-off（同 Coolify / Plausible）。Mitigation：docs/13 明确警告 + Phase 5 可加更严防护（如 token 模式 config 切换）但本 proposal 不做。
- **[删除 setup_token 实体的 migration 兼容性]** → 项目用 sea-orm `Schema::create_table_from_entity` 模式无独立 migration crate；启动期 drop table 在已有 user 的 DB 中无副作用（只是清掉 dev 残留 token）。Mitigation：启动期 `if table_exists drop` 守门，不阻塞已 bootstrap 的部署。
- **[per-user 锁导致 timing leak]** → 攻击者可通过"锁定 vs 未锁定"区分 user 是否存在。Mitigation：未找到 user 时仍走 200ms argon2 dummy verify（已有 timing-equalising path）；锁定逻辑只在找到 user 后才触发，与"invalid credentials"返回时间近似等长。
- **[locked_until 倒计时 UI 跟 server 时钟漂移]** → 直接显示绝对时间戳，让浏览器本地化（"5 minutes ago" / "until 14:30"）；不做精确倒计时（避免漂移误差）。
- **[密码强度字典误伤]** → top-100 字典聚焦最常见弱口令，不会拦截"中等强度"密码；admin SPA 字段下显示具体失败原因（长度 / 多样性 / 字典命中），让用户能调整。
- **[ENV 锁定的 typo 让 Owner 永远进不去]** → mismatch 时 422 提示 "expected email: me@x.com"（暴露 locked_email），让运维一眼能修。安全 trade-off 可接受（locked_email 不是 secret）。
- **[bootstrap window 期间被陌生 IP 抢成 Owner]** → 已在 Risk 1 接受。Mitigation 同。
- **[admin SPA 主 query (meQueryOptions) 在 bootstrap 状态下会先打 401]** → root beforeLoad 先调 setupInfoQueryOptions（无需登录），needs_bootstrap=true 直接 redirect /setup，不打 me。Mitigation：在 beforeLoad 严格按"先 setup info → 后 me"顺序。

## Migration Plan

无 DB schema 破坏（删 setup_token 表 + 加 user_login_attempts 表，均无现有数据依赖）。

部署路径：

1. PR 落 main → CI 跑 rust + node + e2e 全绿
2. 部署到 dev/staging：sea-orm schema sync 自动 drop setup_token + create user_login_attempts
3. 已有 Owner 用户继续可登录（user 表未动）；登录后下一次失败累加 attempts（新表无历史，从 0 起）
4. 新部署：访问 web → `/setup` → 创建 Owner

回滚：revert commit + 重启即可；user_login_attempts 表残留无害（无 user 引用就是孤儿）。

## Open Questions

- **是否需要在 `/login` 提供 "Resend setup email" 链接给运维找回 Owner 入口** → 不需要：bootstrap 完成即固化，Owner 忘密码走 ④ 的 `/forgot-password`（依赖 mail）。
- **`SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` 是否要支持多 email 列表** → 不需要：单 Owner 模型，多个候选反而引入"谁先抢到"的混淆。
- **倒计时 UI 是否本地化 zh-CN "5 分钟后再试"** → 用 dayjs `from()` + Lingui plural，本 proposal 落地时定。
- **是否在 `/setup` 完成后弹"已成为 Owner，建议立即开启 OAuth / 设置 mail / 邀请成员"的引导卡片** → 留给 ⑤ 一起设计（onboarding tour 是 cross-cutting concern）。
