# design

## Context

①②③④ 落齐后注册路径矩阵中只差"自助"一列：

| | bootstrap | 自助 | 邀请 | OAuth |
|---|---|---|---|---|
| email/password | ① | **本 proposal** | ④ | — |
| GitHub | (禁) | **本 proposal** | — | ③ |
| admin 手动 | — | — | ④ | ③ |

自助跟"邀请"的本质差异：admin 没有事前 vetting；所以需要 policy 控制 + 可选 verify + 可选审批。这是一个 **运行时可配的策略系统**，所以核心决策是：

1. 字段集（哪些维度可控）
2. 状态机（user.status 多了 pending_verify / pending_approval 后的所有转移）
3. UI 路由分流（pending_approval user 登进来看什么）
4. 与 ③ OAuth callback 的 hook 点

## Goals / Non-Goals

**Goals:**

- registration_policy singleton 表 + admin UI 全字段可配
- email 自助注册 + verify + （可选）审批闭环
- OAuth 自助注册分支接入 ③ callback
- pending_approval user 登录后被收口到等待页（不能访问业务 page）
- email_verified 字段 + 老用户 backfill

**Non-Goals:**

- 不实现 magic-link / passwordless login
- 不实现 invite token 跳过 approval 的混合（即邀请 + 自助混合）
- 不实现拒绝邀请邮件通知
- 不实现 SCIM / Workforce sync
- 不实现 OAuth-only user 补设密码

## Decisions

### 1. singleton 表 vs settings KV

```rust
pub struct Model {
    id: i32,              // always 1
    allow_self_register_email: bool,
    allow_self_register_oauth: bool,
    require_email_verify: bool,
    self_register_default_role_id: Uuid,  // FK role
    self_register_require_approval: bool,
    allowed_email_domains: Vec<String>,
    updated_at: DateTimeUtc,
    updated_by: Uuid,
}
```

**Why singleton**：

- 全字段强类型，sea-orm column 类型校验直接复用
- admin UI form 直接 1:1 map 字段
- 跟项目 storage_backend 风格一致
- 加字段时 schema migration 看得见（vs KV 表的"加 row 不要 migration"是个伪优势，UI 端仍要更新枚举映射）

**id always 1**：启动期 INSERT IF NOT EXISTS id=1 with defaults；DB 实际只一行；PUT 也只更新 id=1。

### 2. user.status 完整状态机

```text
                       ┌────────────────────┐
                       │ pending_verify     │ ④ invite 或 ⑤ self-register w/ verify
                       └─────────┬──────────┘
                                 │ verify-email POST
                                 ▼
                  ┌──────────────────────────┐
                  │ pending_approval         │ ⑤ if policy.require_approval
                  └─────────┬────────────────┘
                            │ admin approve
                            ▼
                  ┌──────────────────────────┐         admin disable
                  │ active                   │  ◄────────────────────
                  └─────────┬────────────────┘                 │
                            │ admin disable                     │
                            ▼                                   │
                  ┌──────────────────────────┐                  │
                  │ disabled                 │ ─────────────────┘
                  └──────────────────────────┘    admin re-enable
```

**`invited` 是 ④ 落地后的同义词 pending_verify** —— ④ 用 pending_verify；archived auth-rbac 的 'invited' 在本 proposal 后移除掉（避免双义）。tasks 中明确 migration 路径：archived 'invited' 全 UPDATE 为 'pending_verify'。

### 3. self-register flow 三态分支

```text
POST /api/v1/auth/register { email, display_name, password }
   │
   ▼
policy.allow_self_register_email=false → 410 registration_disabled
   │
   ▼
email 占用 → 422 email_already_taken
   │
   ▼
allowed_email_domains 非空 + domain 不匹配 → 422 email_domain_not_allowed
   │
   ▼
password 弱 → 422 password_too_weak
   │
   ▼
case policy.require_email_verify:
   ├─ true:
   │   INSERT user(status=pending_verify, email, email_verified=false) + credentials
   │   gen EmailVerify token → 发 email_verify 邮件
   │   返 200 { next: 'verify_email' }
   │
   └─ false:
       INSERT user(status=?, email, email_verified=true) + credentials + user_role(default_role)
       case policy.require_approval:
           ├─ true:  status=pending_approval, 返 200 { next: 'pending_approval' }（不写 session）
           └─ false: status=active, 写 session, 返 200 { next: 'home' }

POST /api/v1/auth/verify-email { token }
   │
   ▼
verify token → user.email_verified=true
   │
   ▼
case policy.require_approval:
   ├─ true:  status=pending_approval, INSERT user_role(default_role), 写 session（让用户能进 awaiting）
   └─ false: status=active, INSERT user_role(default_role), 写 session
```

**Why pending_approval 用户依然写 session**：让他们登录后能看到"等待审批"友好页面，否则一直 401 困惑。

**Why pending_verify 不写 session**：还没 verify email 不能信任，让用户走链接完成 verify 后再写 session。

### 4. OAuth callback 自助注册分支接入

```text
③ callback "subject not in identity_link"分支：
   │
   ▼
查 user WHERE email = ext.email
   │
   ├─ password user 占用 → ③ 已有 oauth_email_conflict 处理（redirect /login）
   │
   └─ 无现存 → 本 proposal hook：
        │
        ▼
        policy.allow_self_register_oauth=false → 401 oauth_registration_disabled
        │
        ▼
        allowed_email_domains 非空 + domain 不匹配 → 401 + redirect /login?oauth_error=domain_not_allowed
        │
        ▼
        case policy.require_approval:
           ├─ true:
           │   INSERT user(status=pending_approval, email_verified=true) + identity_link + user_role
           │   写 session → 302 /awaiting-approval
           │
           └─ false:
               INSERT user(status=active, email_verified=true) + identity_link + user_role
               写 session → 302 /
```

**Why OAuth email 视为 verified**：GitHub /user/emails 返回 verified 字段，本 proposal 仅信任 verified email（同 ③ 决策）。email_verified=true 不再额外 verify。

**Why allow_self_register_oauth 独立于 allow_self_register_email**：用户场景不同 —— 有人只想开 OAuth（用 GitHub 注册组织成员），不想开 email 自助。

### 5. pending_approval 路由分流

```text
admin SPA _auth.tsx beforeLoad：
   │
   ▼
ensureQueryData(meQueryOptions())
   │
   ▼
me.user.status === 'pending_approval' && location.pathname !== '/awaiting-approval'
   → throw redirect({ to: '/awaiting-approval', replace: true })
```

**Why 用 status 字段而不是 permission 集**：permission 是细粒度（哪些 capability 能用），status 是粗粒度（账号生命周期阶段）。pending_approval user 的 permissions 集是空（无角色），但单看 permissions 无法区分"刚注册没批" vs "Owner 故意 disable"，所以用 status 做主开关。

**`/awaiting-approval` 页轮询 me query**：5s 间隔 + manual refresh 按钮；Owner approve 后 user 浏览器 5s 内跳 `/`。

### 6. Settings > Authentication 页结构

```text
Settings > Authentication
  ├─ OAuth Providers 卡片（③ 已落 ProTable）
  └─ Registration Policy 卡片（本 proposal 新增 ProForm）
       allow_self_register_email   [Switch]
       require_email_verify        [Switch, disabled if above=false]
       allow_self_register_oauth   [Switch]
       self_register_default_role  [Select role]
       self_register_require_      [Switch]
         approval
       allowed_email_domains       [Select tags, free input]
       updated_at / updated_by 显示 readonly
       [保存] 按钮
```

**Why 同页而非独立 /settings/registration**：concept 上"Authentication" 涵盖 provider + 谁能注册；菜单条目少更清爽；用 ProForm in Card 视觉分块。

### 7. Mail 未配置 + require_email_verify=true 的 banner

启动期 check：
- registration_policy.allow_self_register_email=true && require_email_verify=true && Mailer instance is ConsoleMailer
- → admin SPA query mailStatus + registrationPolicy 拼接条件 → /settings/authentication 顶部 Alert.warning

**Why client 端拼条件而非 server 端单独 endpoint**：UI 端 query 复用，避免新 endpoint。

### 8. user.email_verified 字段 + backfill

archived auth-rbac 的 user 表无 email_verified 字段；本 proposal 加 + 迁移：

```sql
ALTER TABLE "user" ADD COLUMN email_verified BOOL NOT NULL DEFAULT false;
UPDATE "user" SET email_verified = true WHERE status = 'active'; -- 老 active 用户视为已 verify
```

sea-orm schema-sync 在加字段后跑一次性 backfill（server 启动期 migration 钩子 + 标记位防重复跑）。

**Why default false + 老用户 backfill true**：default false 让新 INSERT 必须显式设；老用户经过 Owner 信任流程进入，视为 verified 合理。

## Risks / Trade-offs

- **[Owner 开了自助注册又关 Mail → 流程卡死]** → banner + 提示，但 user 已经注册的 pending_verify token 永远无邮件触达。Mitigation：admin Users 页加"手动 verify"按钮（permission-gated user:manage）跳过邮件路径强制 verify；NTH，本 proposal 不实现，留运维通过 SQL `UPDATE user SET status=pending_approval WHERE id=...` 临时处理。
- **[allowed_email_domains 误配锁死 Owner]** → Owner bootstrap 不走 policy（① 已固化）；Policy 只影响自助注册 + OAuth 自助。安全。
- **[pending_approval user 通过 OAuth 反复点登 → 创建多余 identity_link]** → identity_link 表已有 `(user_id, kind)` 唯一索引；同 GitHub 重复登 callback 直接走"已存在"分支，不重复 INSERT。
- **[默认 role 是 viewer 但 Owner 改成 publisher 后历史 pending_approval user approve 时不知道用哪个]** → approve 接口允许 role_id 参数覆盖；UI 显示 policy 默认值但允许 Owner 改。
- **[OAuth 自助注册 race condition：同时 2 个 callback 用同一 GitHub email]** → DB 唯一约束 `user.email` 兜底；第二个 INSERT fail → callback 转 409 + redirect /login?oauth_error=race_conflict（罕见，不优化）。
- **[awaiting-approval 5s polling 加重 server 负载]** → me query staleTime 已 30s（admin-foundation 默认），polling 设 30s 间隔；同时 owner approve 后 audit log 触发 server-side notify 是 NTH，本 proposal 走简单轮询。
- **[allowed_email_domains 正则 vs 精确匹配]** → 精确匹配 lowercase（`@example.com`）；不支持子域 / 通配符（如 `@*.example.com`）；future NTH。
- **[email_verify token 过期 24h 是否可配]** → 不可配，hardcode；future NTH。
- **[backfill 已有 active user 视为 verified 的合理性]** → archived 流程都有人工 vetting（Owner 邀请或 bootstrap），可信。Mitigation：document 此假设。

## Migration Plan

破坏性：`user.status` enum 加 'pending_approval'；'invited' 全 UPDATE 为 'pending_verify'。

部署路径：

1. PR 落 main → CI 全绿
2. 部署 → schema-sync ALTER user 加字段 + enum 扩展 + UPDATE 老 'invited' → 'pending_verify' + INSERT default registration_policy
3. 已有 active user 全部 email_verified=true 后端默认值（迁移期 backfill）
4. 默认 policy 全 false → 注册行为跟 ④ 落地后完全一致（仅 invite + bootstrap）
5. Owner 主动开启自助注册

回滚：revert 难（schema 含已写入数据，无法 drop column 不损失）。Mitigation：本 proposal apply 时确保 backfill 幂等；回滚需文档化"先 UPDATE 'pending_approval' user 为 disabled，再 drop column"流程。

## Open Questions

- **是否在 /register 表单顶部显示当前 policy 摘要**（"注册后需管理员审批" 提示）→ 是，本 proposal 落地。
- **OAuth 自助注册创建的 user 是否记录"通过 OAuth 注册"来源标记** → 不显式记，看 identity_link 是否存在就能推断；NTH。
- **awaiting-approval 页是否提供"撤销注册"按钮** → 不，让 Owner 直接 reject + DELETE；user 主动撤销 NTH。
- **reject 是否发拒绝邮件** → 不，避免泄露 admin 决策；user 自己看不到任何反馈（NTH）。
- **policy 改动是否需要 Owner 二次确认**（敏感设置改完 confirm）→ 否，UI Switch 直接生效；audit log 兜底 review。
