# design

> **Rebased 2026-06-10**：与真实 ship 的 ①②③④ 对齐(原稿是落地前的预测)。改动详见各 Decision 的「⚠️ 重定基」注。

## Context

①②③④ 落齐后注册路径矩阵中只差 "自助" 一列:

| | bootstrap | 自助 | 邀请 | OAuth |
|---|---|---|---|---|
| email/password | ① | **本 proposal** | ④ | — |
| GitHub | (禁) | **本 proposal** | — | ③ |
| admin 手动 | — | — | ④ | ③ |

自助跟 "邀请" 的本质差异:admin 没有事前 vetting;所以需要 policy 控制 + 可选 verify + 可选审批。这是一个**运行时可配的策略系统**,核心决策:

1. 字段集(哪些维度可控)
2. 状态机(`user.status` 加 `PendingApproval` 后的转移)与 verify 信号(`email_verified_at`,正交)
3. UI 路由分流(pending_approval user 登进来看什么)
4. 与 ③ OAuth callback / ④ verify-email 的 hook 点

### ⚠️ 重定基要点(真实代码 vs 原稿假设)

| 原稿假设 | 真实代码 | 本稿处理 |
|---|---|---|
| `UserStatus` 含 `pending_verify`,⑤ 迁 `invited`→`pending_verify` | `{Active, Disabled, Invited}`,**无** pending_verify | `Invited`→**`Provisioned`** 改名 + 加 `PendingApproval`;一次性数据迁移(Decision 2) |
| 加 `email_verified: bool` + backfill `active→true` | 已有 `email_verified_at: Option<ts>`;Owner setup **故意 NULL** | 复用 timestamp,**不加 bool、不 backfill**(Decision 8) |
| 新建 verify-email 三端点 + 邮件模板 | ④ 已 ship 三端点 + 模板 | **扩展**现有 handler,加状态转移(Decision 3) |
| `routes/auth/*.rs`、`routes/users/approval.rs` | 扁平 `routes/`(≤15 文件) | 扁平 `routes/{register,registration_policy}.rs`,approval 扩 `users.rs`(Decision 9) |

## Goals / Non-Goals

**Goals:**

- `registration_policy` singleton 表 + admin UI 全字段可配
- OAuth 自助注册分支接入 ③ callback(支柱 A,优先)
- email 自助注册 + verify + (可选)审批闭环(支柱 B)
- pending_approval user 登录后收口到等待页(不能访问业务 page)

**Non-Goals:**

- 不引入 `pending_verify`(verify 走 `email_verified_at`);但 `Invited` 改名 `Provisioned`(语义纯净,见 Decision 2)
- 不加 `email_verified` bool、不 backfill
- 不实现 magic-link / passwordless / invite-token-bypass-approval / SCIM / OAuth-only 补设密码 / 拒绝邮件

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
    allowed_email_domains: Vec<String>,   // 存 Json,仿 app.platforms / oauth_provider.scopes
    updated_at: DateTimeUtc,
    updated_by: Uuid,
}
```

**Why singleton**:全字段强类型,sea-orm column 校验直接复用;admin form 1:1 map;跟 `storage_backend` / `oauth_provider` 单例风格一致;加字段时 schema 看得见。`id` always 1:启动期 INSERT IF NOT EXISTS,PUT 只更新 id=1。

### 2. user.status:Invited 改名 Provisioned + 加 PendingApproval

真实 `UserStatus = {Active, Disabled, Invited}`,verify 状态由正交的 `email_verified_at` 表达(不是 status)。⑤ 把 `Invited` **改名 `Provisioned`** 当 "已建档、待确认(接受邀请 / 验证邮箱)" 的统称,并加 `PendingApproval`,最终 `{Active, Disabled, Provisioned, PendingApproval}`:

```text
   invite-accept (④,/invite/accept 单设密)            admin disable
  ┌──────────────────────────────────────┐         ┌──────────────┐
  │                                       ▼         ▼              │
[Provisioned]──/auth/verify-email(消费 token)─┬─approval?─否─▶[Active]⇄[Disabled]
 ↑ invite(④)/ self-register(⑤) 共同起点      └─是─▶[PendingApproval]──approve──┘
   (邮箱未验证 = email_verified_at NULL)                  (⑤ 新增)

  正交 verify 轴(任何 status 独立发生):
    email_verified_at: NULL ──verify──▶ Some(ts)
```

**⚠️ 重定基 + 决策(2026-06-10 用户拍板:语义纯净优先)**:原稿要 rename `invited→pending_verify`(错前提)。真实只有 `Invited`。曾权衡 "保留 Invited 零迁移" vs "rename Provisioned 语义纯净",**用户选后者**——`Invited` 暗含 admin 邀请,而自助注册者没人邀请,`Provisioned` 才统得起两条流。

**Blast radius**(rename 触及,apply 时一并改):

- `entity/user.rs`:变体 `Invited`→`Provisioned` + `string_value="invited"`→`"provisioned"` + 两个 `From` 臂
- `api-types/user.rs`:`UserStatus::Invited`→`Provisioned`
- `routes/invite.rs`:`:170` set + `:252` check 的 `UserStatus::Invited`→`Provisioned`
- doc 注释:`auth/service.rs:120`、`account_token.rs:5`
- 测试:`account_token_smoke.rs` `:284`(变体)+ `:510`(wire 字符串 `"invited"`→`"provisioned"`)
- admin:`_auth/users.tsx` `:129/:156` 的 `status==="invited"`→`"provisioned"`(显示 + tag);`schema.gen.ts` 随 OpenAPI 重生成
- **不动**:`settings/mail/templates.tsx` 的 `invited_by`(邮件模板变量,与 status 无关)

**数据迁移**(见 Migration Plan):存量 `status='invited'` 行需一次性 raw `UPDATE → 'provisioned'`,且必须**在任何 sea-orm 读 user 行之前**跑(否则旧值反序列化失败 → 启动崩)。天然幂等(`WHERE status='invited'` 跑完即空),**无需 marker 表**。

### 3. self-register flow + verify-email 扩展(复用 ④,靠 status 消歧)

```text
POST /api/v1/auth/register { email, display_name, password }
   policy.allow_self_register_email=false → 410 registration_disabled
   email 占用 → 422 email_already_taken
   allowed_email_domains 非空 + domain 不匹配 → 422 email_domain_not_allowed
   password 弱 → 422 password_too_weak
   │
   INSERT user(status=Provisioned, email_verified_at=NULL) + credentials + user_role(default)
   │
   case require_email_verify:
     ├─ true:  gen EmailVerify token → 发 email_verify 邮件 → 200 { next: 'verify_email' }（不写 session）
     └─ false: case require_approval:
                 ├─ true:  status=PendingApproval, 写 session, 200 { next: 'pending_approval' }
                 └─ false: status=Active,          写 session, 200 { next: 'home' }

POST /api/v1/auth/verify-email { token }   ← 扩展 ④ 现有 handler
   ④ 现有:消费 token → email_verified_at = now()（仅 NULL→now,幂等）
   ⑤ 增量:if user.status == Provisioned:
       case require_approval:
         ├─ true:  status=PendingApproval, 写 session   （让用户能进 awaiting 页）
         └─ false: status=Active,          写 session
     else (status==Active, banner verify):  维持原行为,不转移
```

**Why 靠 status 消歧**:invite-accept 走独立的 `/invite/accept`(单设密直接 Active,不碰 verify-email);banner verify 的用户已是 Active。所以唯一以 `status=Provisioned` 进 verify-email 的就是自助注册者,无歧义。

**Why pending_verify 阶段不写 session**:邮箱未验证不可信,走链接 verify 后再写 session。**Why pending_approval 写 session**:让其登录后看到友好 "等待审批" 页,否则一直 401 困惑。

**公开 resend**:自助注册者 status=Provisioned、无 session,用不了 auth 的 `POST /users/me/verify-email/send`。⑤ 加公开 `POST /api/v1/auth/verify-email/resend { email }`(始终 200,枚举防御,timing 拉平;找到 `email_verified_at IS NULL` 用户才 invalidate+gen+发)。

### 4. OAuth callback 自助注册分支接入(改 routes/oauth.rs)

```text
③ callback "subject 不在 identity_link" 分支(routes/oauth.rs:285+):
   查 user WHERE email = ext.email
   ├─ password user 占用 → ③ 已有 302 /login?oauth_conflict（不变）
   └─ 无现存 → ⑤ hook（替换现 routes/oauth.rs:319-325 的硬 401）:
        policy.allow_self_register_oauth=false → 401 oauth_registration_disabled（不变）
        allowed_email_domains 非空 + domain 不匹配 → 302 /login?oauth_error=domain_not_allowed
        case require_approval:
          ├─ true:  INSERT user(status=PendingApproval, email_verified_at=now()) + identity_link + user_role → 写 session → 302 /awaiting-approval
          └─ false: INSERT user(status=Active,          email_verified_at=now()) + identity_link + user_role → 写 session → 302 /
        race(user.email 唯一约束 fail)→ 302 /login?oauth_error=race_conflict
```

**Why OAuth email 视为 verified**:③ 仅信任 GitHub `/user/emails` 的 verified,故 `email_verified_at=now()`,不再额外 verify。**Why oauth 开关独立于 email**:常见诉求是只开 OAuth(同事用 GitHub 自助 onboard)、不开 email。

### 5. pending_approval 路由分流

```text
admin SPA _auth guard beforeLoad:
   ensureQueryData(meQueryOptions())
   me.user.status === 'pending_approval' && location.pathname !== '/awaiting-approval'
     → throw redirect({ to: '/awaiting-approval', replace: true })
```

**Why 用 status 而非 permission 集**:permission 是细粒度能力,status 是粗粒度生命周期。pending_approval user 的 permission 集虽空(无角色),但单看 permission 无法区分 "刚注册没批" vs "Owner 故意 disable",故用 status 做主开关。`/awaiting-approval` 轮询 me query(30s,staleTime 已 30s)+ 手动刷新按钮。

### 6. 注册策略独立页 + 注册审批独立页(2026-06-10 用户拍板,推翻原"同页卡片"方案)

原方案把 Policy 做成 Settings › Authentication 底部卡片、审批做成 Users 行内按钮;**用户 review 后拍板两者都独立成页**:

```text
Settings
  ├─ 邮件        /settings/mail
  ├─ 认证        /settings/authentication   ← 只管 OAuth provider CRUD(③ 原样)
  ├─ 注册策略    /settings/registration     ← ⑤ 新页:ProForm 全字段 + mail 未配置 banner
  └─ 存储        /settings/storage

成员(父菜单)
  ├─ 成员列表    /users/list       ← directory 化;pending 行只留「去审批」入口
  └─ 注册审批    /users/approvals  ← ⑤ 新页:server 分页(GET /users/pending-approval)
                                      + 批准(角色预填可覆盖)/ 拒绝(原因)Modal
  (/users 本身是 redirect → /users/list)
```

**Why 独立页**:Policy 字段多(6 项)塞在 provider 表格下方淹没;审批是高频运营动作,值得专属入口 + 服务端分页,而不是混在全量成员列表里。认证页留一条 info Alert 链接到注册策略页。

**侧栏选中态的 same-path 坑**:子项"成员列表"最初 path 用 `/users` 与父菜单同路径 → ProLayout 以 path 为 menu key,父子撞 key,选中高亮失效(用户截图实测)。解法与 `/settings` → `/settings/mail` 同款:`/users` 变 redirect-only(`users/index.tsx`),列表挪 `/users/list`,父子路径不再重叠。

**审批职责单一**:批准/拒绝的 Modal **只在** `/users/approvals`;成员列表的 pending_approval 行只渲染「去审批」Link(避免两套 Modal 重复维护)。`GET /users/pending-approval` 返回 `UserListItem`(含 roles)以便 Modal 预填注册时绑定的默认角色——**不要**为预填打 policy 端点(操作者只保证有 `user:manage`,不一定有 `auth:manage`)。`RoleSelect` 抽 `users/-shared.tsx`(releases 的 `-shared` 先例)供列表「更改角色」与审批 Modal 共用。

### 6b. 成员管理操作:改角色 / 禁用 / 启用(2026-06-10 用户扩展,超出原 spec)

用户 review 时指出成员列表缺管理操作。新增三个 `user:manage` 端点(扩展 `routes/users.rs`)+ 列表行操作:

- `PUT /users/{id}/role { role_id }`:整体替换角色绑定(单角色 MVP,与 approve 覆盖同语义);禁选 owner。
- `POST /users/{id}/disable`:仅 Active 可禁(pending 走 reject);**置 Disabled 后立即 `revoke_user_sessions` 踢下线**。
- `POST /users/{id}/enable`:仅 Disabled 可启。
- **共同护栏 `guard_not_owner_not_self`**:不可操作 owner 用户(防降级唯一 owner 锁死系统)、不可操作自己(防自降权后无人能改回),422 typed `cannot-manage-{owner,self}`;UI 对 owner 行 / 自己行直接不渲染操作。
- 审计:`user_role_changed` / `user_disabled` / `user_enabled`(docs/13 敏感操作清单要求)。

### 7. Mail 未配置 + require_email_verify=true 的 banner

admin 端 query `mailStatus` + `registrationPolicy` 拼条件(`mail.fallback_mode && policy.allow_self_register_email && policy.require_email_verify`)→ Settings › Authentication 顶部 `Alert.warning`。**Why client 端拼**:复用现有 query,避免新 endpoint。

### 8. email_verified 信号:复用 email_verified_at,不加 bool、不 backfill

**⚠️ 重定基(原稿这里会引入 bug)**:原稿要加 `email_verified: bool` + `UPDATE ... SET email_verified=true WHERE status='active'`。但真实 `user.email_verified_at: Option<DateTimeUtc>` 已存在,且其 doc 明确 **"NULL for fresh Owner setups (verification is opt-in via the in-app banner)"**——照原稿 backfill 会把 Owner 故意的 NULL 覆盖成 now(),打破 opt-in banner = bug。

**决定**:`email_verified_at.is_some()` 即 "已验证" 信号,⑤ 不加任何字段、不 backfill、不需要 migration marker 表。self-register 创建用户时显式 `email_verified_at=NULL`(待 verify)或 `=now()`(OAuth verified),与现有语义一致。

### 9. 实现揭示的修正(2026-06-10 apply 时落定)

- **(后追加)`/awaiting-approval` 改为顶层全屏路由**:初版放 `_auth/` 下,用户实测指出待审批用户看到了 ProLayout 侧边栏壳。迁出为顶层路由(URL 不变,pathless layout 特性),自管认证 beforeLoad;`_auth` guard 的 pathname 例外条件随之删除。

- **`load_principal` 放行 `PendingApproval`**:原以为"写 session 即可看等待页",实测 `auth/service.rs::load_principal` 只放 Active → 待审批用户连 `/me` 都 401,等待页轮询直接坏。修正:放行 `Active | PendingApproval`(其 permission 集为空,所有 `require_permission!` 端点天然 403);**代价**是无权限门的 session 端点需自查——`device.rs::require_session` 显式加 Active 检查(知识库早就预言 device approve 依赖旧不变式)。**已知限制**:PendingApproval 登出后密码重登仍 401(login 的 Inactive 分支不区分),主 UX 靠注册/verify/OAuth 当场写的 session;支持重登留后续增量。
- **新增公开端点 `GET /auth/registration-options`**:/login 注册链接与 /register 提示需要"注册是否开放",但 policy 端点要 `auth:manage`,匿名页拿不到。只暴露三个布尔(email 开关/verify/approval),不下发域白名单。
- **`updated_by` 改 `Option<Uuid>`**:seed 默认行写入时尚无任何用户,非空 FK 卡 seed;NULL = 系统 seed。
- 杂项:request DTO(garde)按项目惯例 route-local 而非 api-types;handler `register` 与 `setup.rs::register` 撞 operationId → 改名 `register_account`。

## Risks / Trade-offs

- **[Owner 开自助注册又关 Mail → 流程卡死]** → banner 提示;已注册的 Invited 用户 token 无邮件触达。Mitigation:运维可 SQL 临时改 status,或公开 resend 在 mail 配好后重发;Users 页 "手动 verify" 按钮 NTH。
- **[allowed_email_domains 误配锁死 Owner]** → Owner bootstrap 不走 policy(① 固化);policy 只影响自助 + OAuth 自助。安全。
- **[pending_approval user 反复 OAuth 登 → 多余 identity_link]** → `identity_link (provider, subject)` 唯一约束兜底;同 GitHub 重复登走 "已存在" 分支。
- **[默认 role=viewer 但 Owner 改成 publisher,历史 pending_approval 用哪个]** → approve 接口允许 role_id 覆盖;UI 显示 policy 默认但可改。
- **[OAuth 自助 race:2 callback 同 GitHub email]** → `user.email` 唯一约束兜底;第二个 INSERT fail → 302 /login?oauth_error=race_conflict。
- **[allowed_email_domains 精确 vs 通配]** → 精确 lowercase(`example.com`);不支持子域通配;future NTH。
- **[~~backfill active→verified 的合理性~~]** → **已移除**:不再 backfill(见 Decision 8)。

## Migration Plan

三处 schema/数据变更(比 backfill 方案仍简单,但因 rename 含一次性数据迁移):

1. `user.status`:`Invited`→`Provisioned` 改名 + 加 `PendingApproval`。DB 列是 varchar(16),DDL 不变;**数据**需一次性 `UPDATE "user" SET status='provisioned' WHERE status='invited'`。
2. 新增 `registration_policy` 表 + 启动期 seed id=1(现有 seed.rs 模式,需 viewer role 先 seed)。

**一次性 rename 迁移的执行约束**(关键):`string_value` 从 `"invited"` 改 `"provisioned"` 后,sea-orm 读到旧 `'invited'` 行会反序列化失败 → 启动崩。故该 `UPDATE` 必须**排在任何 User entity SELECT 之前**。
**(2026-06-10 二次修订,用户拍板)** 初版把它放 `db.rs::migrate_data`(raw SQL 藏在 `sync_schema` 内)——不规范且有**真 bug**:bin 的 `sync_schema` 被 `auto_sync` gate 包住,生产(auto_sync=false)根本不跑数据迁移。重构为专门的 **`swarmhive-migration` crate**(`sea-orm-migration =2.0.0-rc.38`,与 sea-orm 同 rc;不依赖 entity 防实体漂移):`m20260610_000001_rename_invited_to_provisioned` 用 `DO $$ IF to_regclass(...)` 容忍表未建;`db::run_migrations`(=`Migrator::up()`,`seaql_migrations` 记账)在 dev 经 `sync_schema` 内联、生产经 bin else 分支**无条件执行**。回归:`db_smoke::invited_rows_are_migrated_once`。

部署路径:PR 落 main → CI 绿 → 部署 → schema-sync → **raw UPDATE invited→provisioned** → seed default policy → 默认 policy 全 false,注册行为与 ④ 落地后一致 → Owner 主动开启。

回滚:`registration_policy` 表残留无害;但 `'provisioned'`/`'pending_approval'` 行回滚到旧代码会变成未知 enum 值。Mitigation:回滚前先 `UPDATE user SET status='invited' WHERE status='provisioned'` + `UPDATE ... SET status='disabled' WHERE status='pending_approval'`。

## Open Questions

- **支柱 B(email/password 自助注册)对单组织内部工具是否真的需要?** SwarmHive 是单组织 + RBAC 的自托管工具,"陌生人填邮箱注册" 形态违和(故默认 false)。更现实是支柱 A(OAuth 自助 + 域白名单)。支柱 B 可能是 "矩阵补洞"。→ 当前保留在本 change,但 tasks 排在支柱 A 之后,可按需暂缓。
- **~~`Invited` 保留 vs rename `Provisioned`~~** → **已定:rename `Provisioned`**(2026-06-10 用户选语义纯净,接受一次性数据迁移成本);见 Decision 2。
- **/register 表单顶部按 policy 显示摘要**("注册后需审批" / "需验证邮箱")→ 是,本 proposal 落。
- **awaiting-approval 提供 "撤销注册" 按钮** → 不,Owner reject + DELETE;NTH。
- **reject 发拒绝邮件** → 不,避免泄露 admin 决策;NTH。
- **policy 改动二次确认** → 否,Switch 直接生效,audit log 兜底。
