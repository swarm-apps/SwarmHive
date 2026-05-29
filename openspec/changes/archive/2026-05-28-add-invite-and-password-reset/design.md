# design

## Context

①② 落地后，新成员入口和忘记密码自救两条链路依然是空白。本 proposal 把"邀请 + 重置"作为 ②（mail）的第一个真实消费者落地。两条链路共享同一套"一次性 token"机制，所以决策的核心是 **token 模型**（单表 vs 多表、明文 vs hash、过期 vs 单次性 vs 双重）。

约束：

- **不存明文 token**：token 在邮件链接中可见，但 DB 必须存 hash（防 DB dump 后 token 被滥用）
- **复用 garde/zod 密码强度**：① 已落
- **复用 mail trait**：② 已落 `Mailer::send(MailEnvelope)`
- **密码重置后让旧 session 失效**：基础安全要求
- **`/forgot-password` 防 email 枚举**：始终返 200，不论 email 是否存在
- **邀请链接 72h 过期 / 重置链接 1h 过期**：业界主流参数

## Goals / Non-Goals

**Goals:**

- Owner / admin 能从 Users 页邀请新成员（最小 Users 页 + 邀请 drawer）
- 被邀人 + 任何已激活 user 都能走 web 端完整 self-service 流程
- 邀请 / 重置 / 验证 token 安全（hash 存 + 一次性 + 过期）
- 密码重置后所有旧 session 失效（防 stolen session）
- **Owner setup 填错 email 的事故路径闭环**：banner 持续引导 verify + reset 硬阻塞未验证邮箱
- email_verify endpoint 完整落地（⑤ 自助注册直接复用，无须再做）

**Non-Goals:**

- 不实现自助注册（/register UI 留 ⑤）
- 不实现修改密码（profile 改）
- 不实现修改 email（profile 改）
- 不实现批量邀请
- 不实现邀请撤销专用 endpoint（DELETE user 即可）
- 不实现 OTP 形态 verify（统一 URL token）

## Decisions

### 1. 单表 account_token（vs 多表 invite_token / reset_token / verify_token）

```rust
pub enum TokenPurpose { Invite, PasswordReset, EmailVerify }

pub struct Model {
    id: Uuid,
    purpose: TokenPurpose,
    user_id: Option<Uuid>,       // invite 时 null（user 已创但 token 是 invitee 凭证）
    token_hash: String,          // argon2(plaintext)
    payload: Option<Value>,      // invite: { role_id }；其他 None
    expires_at: DateTimeUtc,
    consumed_at: Option<DateTimeUtc>,
    created_at: DateTimeUtc,
    created_by: Option<Uuid>,    // invite: 发邀请的 admin
}
```

**Why 单表**：

- 三类 token 数据形态 95% 相同（id / hash / expires / consumed / user_id），抽 enum 字段比三张表干净
- 索引复用：`(user_id, purpose) WHERE consumed_at IS NULL` 满足"一个 user 同 purpose 只能有一个 active token"
- future 加新 purpose（如 2FA recovery token）无需新表 + migration
- garbage collection 简单（一个 cron job 清三类）

**Trade-off**：payload 是 jsonb，invite 的 role_id 校验得在应用层做（schema 表达不出）。trade-off 可接受。

### 2. token_hash 用 argon2 vs HMAC

- argon2（推荐）：跟密码 hash 同形态，DB 即使被脱裤 token 无法暴力还原；缺点 verify 慢（~100ms / token）
- HMAC：常数时间校验、快、足够安全；缺点 server 端要存 hmac key（在 SecretKey 模块里，已就绪）

**选 argon2**：本场景 token verify 量极小（每次 reset / accept 一次），100ms 完全可接受；少一个 key 管理点。argon2 参数沿用 OWASP 2024。

### 3. token 编码 + 长度

- plaintext 32 字节随机 → base64-url-no-pad（43 字符）
- URL 形态：`<base_url>/accept-invite?token=Eh3...`
- 校验流程：
  1. 接到 plaintext → 查 account_token WHERE consumed_at IS NULL AND expires_at > now()
  2. 但 token_hash 是 argon2 无法 SELECT WHERE → 怎么找？

**两阶段查询设计**：

```text
account_token 表加 token_lookup 字段：sha256(plaintext) 前 16 字节 base64（22 字符）
索引 (purpose, token_lookup)
校验流程：
  1. lookup = sha256(plaintext)[..16]
  2. SELECT token_hash WHERE purpose=? AND token_lookup=? AND consumed_at IS NULL AND expires_at > now()
  3. argon2_verify(plaintext, token_hash) → ok / fail
```

**Why lookup hash + argon2 hash 双层**：

- lookup 解决"无法 WHERE argon2_hash"的问题（argon2 含 salt 不能直接比对）
- sha256 16 字节 lookup 抗碰撞够（2^64 碰撞概率极低，且 attacker 拿到 lookup 也无法还原 plaintext）
- argon2 提供"真校验"防 lookup 表泄露后被暴力

### 4. 密码重置后清 session

```text
POST /api/v1/auth/reset-password { token, password }
  ├─ 验 token + 设新密码 + mark consumed
  ├─ DELETE FROM session WHERE user_id = ?
  ├─ INSERT new session for current request
  └─ 写 audit log password_reset_completed
```

**Why 清所有 session**：reset 场景假设"账号可能被入侵"，旧 session 必须失效（即使 attacker 已在浏览器拿了 session cookie 也立刻无效）。

**Why 不影响 PAT/Bearer token**：PAT 是显式 device credential，user 自己有责任管理；reset 不动 token 避免 CI/CD 集成中断。后续可加"reset 同时撤销 PAT"开关。

### 5. /forgot-password 防 email 枚举

```text
POST /api/v1/auth/forgot-password { email }
  │
  ▼
查 user WHERE email
  ├─ 找到 → invalidate active reset token → gen new token → 发邮件 → 200 generic 文案
  └─ 找不到 → 200 generic 文案（不发任何邮件）
```

**timing**：找到/找不到分支耗时差异巨大（argon2 hash gen ~100ms + mail send ~50ms vs 一次 SELECT ~1ms）。**Mitigation**：找不到分支 sleep 到 150ms 拉平（粗暴但有效）；admin SPA 始终显示"如果该邮箱可用，重置邮件已发送"。

### 6. 邀请流的 user 时序

```text
POST /api/v1/users/invite { email, role_id }
  │
  ▼
校验 email 未占用 → 422 if taken
  │
  ▼
TX:
  INSERT user (status=pending_verify, email, display_name=email_local_part)
  INSERT user_role (user_id, role_id)
  INSERT account_token (purpose=Invite, user_id=new_user.id, payload={role_id}, ...)
  │
  ▼
发 user_invite 邮件 with invite_url containing plaintext token
  │
  ▼
返 200 { user_id, expires_at }
```

**Why 邀请时立刻 INSERT user**：

- Owner 在 Users 页看到 pending_verify 行能跟进状态
- 一次性 transaction 避免"邀请发了但 user 没建"的边角态

**Why pending_verify 不能登录**：login handler 检查 `user.status == 'active'`；pending_verify 返 401 `account_not_activated` 引导走 /accept-invite。

### 7. UI: 4 个新页 + Users 页最小版

```
/forgot-password                 公开，无 guard
/reset-password?token=...        公开，token 必须 valid
/accept-invite?token=...         公开，token 必须 valid
/_auth/users (本 proposal 最小版) Users 列表 + 邀请按钮
```

Users 页本来计划留给后续 `add-users-page-ui` proposal，但邀请按钮天然落 Users 页，本 proposal 做"最小版"（列表 + 邀请 drawer + resend），后续 page proposal 扩 detail / edit / delete 等。

### 8. URL token 与 search params

token 在 URL search params（`?token=...`）而非 path（`/accept-invite/Eh3...`），原因：

- search params 默认不进 referer header（浏览器主流不传）
- 不污染 router file-based naming（路由就是 `/accept-invite`，不需要 `/$token.tsx`）
- 跟 GitLab / GitHub 实践一致

**风险**：token 进浏览器 history。Mitigation：accept / reset 成功后立即 `navigate({ to: '/', replace: true })` 让 token URL 不进 back stack；不做完美防护（self-host 私人浏览器场景风险低）。

### 9. Owner email 自验证流（探索拍板新增）

**问题**：① 的 setup 不验证 owner email 可达性；如果 owner 在 setup 时 typo email，后续 reset 邮件发到错地址永远收不到 → 唯一救援是 DB 直改 user.email。本 proposal 加 reset 流程，反而让这个事故路径"更可见"（用户会假设 reset 能救自己，实际救不了）。

**方案**（探索过 A/B/C 三选，详见 `dev-notes/explore-summaries/2026-05-27-account-onboarding.md` 补充段落 / 本 proposal 探索讨论记录）：选 B 方案 + 硬阻塞 + 持续 banner + ConsoleMailer 时强制引导先配 SMTP，理由：

- **不阻塞 setup 体验**（A 方案保留）：binary 跑起来仍能立刻完成 owner 创建 + auto-login，无须 SMTP 先就绪；与 `add-mail-infrastructure` 的 ConsoleMailer fallback 形成完整的"零依赖启动"语义
- **闭环救援路径**（A 方案缺失）：reset 流硬阻塞未验证邮箱后，"reset 邮件发到错地址" 这条事故路径不再可能发生；owner 唯一被锁死的场景是"邮件配错 + 忘密码"复合事故，需 DB 直改救援 —— 这是相对窄的角落
- **owner 有强动机 verify**（C 方案过于宽松）：持续 banner 不可 dismiss + reset 硬阻塞 → owner 即使懒，第一次想用 "忘记密码" 时就被强制走完 verify
- **ConsoleMailer 时引导先配 SMTP**：banner 检查 `mailStatus.fallback_mode`，fallback 模式下 banner 文案与 action 切换为"请先配置 SMTP"；verify-email/send endpoint 也以 422 `mail_not_configured` 拒绝。这是顺序约束（SMTP → verify → reset），但每一步都有清晰引导

**实现要点**：

- `user.email_verified_at: Option<DateTimeUtc>` 与 `user.status` 正交：status 管账号生命周期（active / pending_verify / disabled），verified_at 管邮箱真实性。一个 status=active 但 verified_at=NULL 的 owner 仍能用所有功能，只是不能 reset
- Invitee accept invite 完成时自动设 `email_verified_at=now()`：点 invite 链接本身已证明邮箱可达，无须再走一遍 verify 流程；reset 链路对 invitee 立即可用
- `EmailVerify` token 复用同一个 `account_token` 表 + 同一套 verify/consume 工具函数；spec 高度复用 invite/reset 模式
- email verify token 有效期 24h（取中：reset 1h 太短给 owner 处理时间窄；invite 72h 太长给 verify 增加 attacker 窗口；24h 平衡）
- verify-email/send 60s 重发限速：通过 query 现有 `(user_id, EmailVerify) WHERE consumed_at IS NULL` token 的 created_at 判断；过近 → 429。原因是 verify 是 owner 主动触发的低频操作，60s 节流足够防 spam
- verify-email POST 不要求 session：点邮件链接的 user 可能在另一台设备 / 另一个浏览器；token 本身已是凭证。这与 accept-invite 一致

## Risks / Trade-offs

- **[token_lookup sha256 碰撞]** → 2^64 碰撞空间，实际无威胁；但若发生 → argon2 verify 兜底失败返 404 not_found。可接受。
- **[邀请 email 字段 typo]** → 邀请发到错邮箱 → 任何人能注册成该 role；只能靠 Owner 谨慎填邮箱。Mitigation：admin SPA drawer 加"再次确认 email"二次输入框（同 email 才能 submit）。
- **[reset 期间 user 不在线，attacker 抢先 reset]** → 1h 窗口 + email 必须能收 = 假设 email 账户没被攻陷。Mitigation：发 reset 邮件同时发 security_alert 邮件到 user（"有人请求重置你的密码，如非本人请联系 admin"）；NTH，本 proposal 留 ⑤ 一起做（需要 security_alert template 完善）。
- **[admin invite 不可撤销]** → DELETE user 是事实上的撤销（CASCADE delete user_role + account_token）。但已激活 user 误删风险大。Mitigation：仅 pending_verify status 可"撤销"（DELETE）；active user 改 status=disabled。本 proposal 实现 pending_verify DELETE。
- **[密码重置同时撤销 PAT 与否的选择争议]** → 不撤销（reset 场景跟"账号被入侵"是两个层级）；future 加 user 自主"revoke all my PATs" 按钮 NTH。
- **[`/forgot-password` sleep 150ms 拉平 timing 在 high-load 下不准]** → 简单 sleep 模型在负载高时实际时间会偏大；可接受 trade-off（攻击者拿到的信号噪声很大）。
- **[invite link 在公共邮件 inbox 被预览爬虫触发]** → Gmail / Outlook 等 email preview bot 会预访问 URL 触发 GET。本 proposal `accept-invite/info` GET 是只读（不 consume token），安全；POST consume token 才标 consumed。verify-email/info 同设计。
- **[未验证 owner 被锁死]** → owner 邮箱填错 + ConsoleMailer fallback + 忘密码三重事故 → 完全锁死。Mitigation：(a) banner 持续可见促 owner 优先 verify；(b) `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL` env 可在重启时通过环境覆盖（虽不能直接救账号但能确认填错的 email）；(c) 文档 [docs/13-rbac.md](../../../docs/13-rbac.md) 写明 DB 直改 user.email 是合法的最后救援手段，附 SQL 模板。
- **[verify token 在邮件链接被第三方截获]** → 等同 reset token 的攻击面：拿到 token 的人能 verify 任意人的邮箱（标 verified_at），但**不能因此登录或改密码**（verify endpoint 只标位）。最坏后果是受害者 reset 流被"提前解锁"，但 reset 仍要发邮件到 verified 邮箱本身 —— attacker 拿不到内容。Mitigation：24h 过期 + token 一次性；可接受。

## Migration Plan

无破坏。部署路径：

1. schema-sync 创建 `account_token` 表 + `user.status` enum 加 'pending_verify'
2. mail template 写实（覆盖 ② seed 的占位内容）
3. /login "忘记密码" 链接可用

回滚：revert + 重启；token 表残留无害（仍标 pending_verify 的 user 走 admin 手动激活）。

## Open Questions

- **invite 的 default role 是否限制（不能 invite owner）** → 是；endpoint 校验 role.name != 'owner'，返 422 cannot_invite_owner（Owner bootstrap 严格只 ① 一条路径）。本 proposal 落地。
- **是否在 /forgot-password 加 rate limit per email** → 是；tower-governor 已对 /auth 子路由限流，本 proposal 不另加 per-email key；NTH。
- **password reset 期间是否允许同时多个 active token** → 否，新发 invalidate 旧 token（`(user_id, purpose) WHERE consumed_at IS NULL` 索引强制）。
- **invite token 过期 72h 是否可 admin 配** → 否，hardcode；future 加 admin Settings > Security 配置项 NTH。
- **是否在 admin Users 页加"待激活 (pending_verify)" 状态筛选** → 是，本 proposal Users 页最小版含 status filter（active / disabled / pending_verify）。
