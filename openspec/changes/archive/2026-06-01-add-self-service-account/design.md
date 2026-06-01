# Design — add-self-service-account

## 后端数据流

```text
                         PATCH /api/v1/users/me { display_name }
                         ┌──────────────────────────────────────────┐
 SPA /profile  ───────▶  │ routes/account.rs::update_me               │
                         │  Principal(Active) → load user(id)         │
                         │  trim + 校验 1..=100 → user.display_name   │
                         │  user.save(db)  (before_save 填 updated_at)│
                         └────────────────┬───────────────────────────┘
                                          ▼  Json(api::User::from(&user))

                         PUT /api/v1/users/me/password { current_password?, new_password }
                         ┌──────────────────────────────────────────────────────────┐
 SPA /profile  ───────▶  │ routes/account.rs::change_password                          │
                         │  Principal(Active) → uid                                    │
                         │  cred = user_credentials::find_by_id(uid)                   │
                         │   ├─ Some(c): current_password 必填                          │
                         │   │     password::verify(current, c.argon2_hash)            │
                         │   │       false → 422 current_password_incorrect            │
                         │   └─ None  (OAuth-only): 跳过 current(=「设置密码」)          │
                         │  validate_strong_password(new) ? → 422 password_too_weak    │
                         │  ── TX ──────────────────────────────────────────────────  │
                         │   service::upsert_credentials(txn, uid, hash(new))          │  ← 从 password_reset 提升
                         │   service::revoke_user_sessions(txn, uid)  (删全部 session) │  ← 从 password_reset 提升
                         │  ──────────                                                 │
                         │  service::establish_session(&session, uid)  (重发当前 session) │
                         │  audit_log: actor=uid, action="auth:password_changed"       │
                         └────────────────┬───────────────────────────────────────────┘
                                          ▼  204 No Content
```

**鉴权**:两个 endpoint 只要求 `Principal` 是 Active 用户(extractor 已拒非 Active),**不**需要任何 `*:manage`——这是 self-service,作用域恒为「调用者自己」。允许 Session 与 Bearer(PAT 所有者即本人);改密码的真正闸门是 `current_password`(已有密码时)或「已认证为该用户」(OAuth-only 设密)。

**为什么改密后踢其它 session 而非全踢**:复刻 `password_reset` 语义——当前操作设备保持登录(刚 `establish_session` 重发),其它设备/标签下次请求 401 回登录。这是 NIST 800-63B 推荐的「密码变更使旧凭证失效」。`upsert_credentials` + `revoke_user_sessions` 由 reset 路径与本路径共用,故从 `password_reset.rs` 私有 helper 提升到 `auth/service.rs`(对齐 `establish_session` / `verify_password` 同住一处)。

**为什么挂 `sensitive_routes`**:`change_password` 校验 `current_password`,可被在线暴力,需 governor 限流;`update_me` 一起挂无害。两处路由清单(`sensitive_routes` + `openapi_router`)是单一来源,新 router merge 一次即同时进运行时与 codegen。

## 前端信息架构(IA)：before → after

```text
BEFORE                                   AFTER (方向 A)
─────────────────────────────            ─────────────────────────────
头像下拉                                  头像下拉
  └─ 个人资料 → /profile                    └─ 个人资料 → /profile        ← 个人账户唯一入口
        (仅 OAuth 登录方式)                      ├─ 账户信息(邮箱/显示名✎/验证)
                                                 ├─ 安全(改/设密码)
设置菜单(所有人可见)                            └─ 登录方式(OAuth 绑定/解绑)
  ├─ 账户 → /settings/account ← 删
  │     (只读 邮箱/显示名/验证)            设置菜单(仅 canManageSettings 可见)
  ├─ 邮件   (mail:manage)                   ├─ 邮件   (mail:manage)
  ├─ 认证   (auth:manage)                   ├─ 认证   (auth:manage)
  ├─ 存储   (storage:manage)                ├─ 存储   (storage:manage)
  └─ 遥测   (disabled)                      └─ 遥测   (disabled)
                                          /settings → redirect 首个 enabled
                                            manage 模块(无权限 → /profile)
```

**关键决策**:个人级数据归头像下拉,组织/部署级配置归「设置」+ manage 门控。删掉「账户」后,「设置」菜单不再需要 everyone 例外——普通用户登录后侧栏不出现「设置」,符合「无权配置就不展示配置入口」。

**`/profile` 页布局**:`PageContainer` + `tabList`(`账户信息` / `安全` / `登录方式`),复用 settings/mail 的 tabList 范式;每个 tab 一个 Card,内容条件渲染(切 tab remount,ProForm initialValues 每次从最新 `me` 生效)。显示名编辑用 `ProForm` 内联,改密码用 `ProForm`(current 条件显示)。

**「已有密码」信号 → 新增 `MeResponse.has_password`（apply 期决策）**:「安全」tab 必须区分「改密码(要 current)」与「设密码(OAuth-only,免 current)」。`me` 原本不含 credential 存在性,前端无从判断。最终在 **`MeResponse`(`/auth/me`)新增 `has_password: bool`**(后端 `user_credentials` 的 `count > 0`,不读 argon2 hash),而**不**塞进纯 `User` DTO——这正是本仓「待真有 UI 分支需求再加」的兑现点(见 oauth change 决策)。`change_password` 端点仍独立兜底(无 credential 即视为设密),与前端信号双保险。

## 测试

`tests/account_smoke.rs`(testcontainers Postgres,in-process `build_router`):

1. bootstrap owner(有密码)→ PATCH display_name → GET me 反映新值。
2. 改密码:错误 current → 422 `current_password_incorrect`;正确 current + 弱新密 → 422 `password_too_weak`;正确 current + 强新密 → 204。
3. 改密码后:旧 session cookie 失效(用第二个 client 持旧 cookie 打 `/me` → 401),当前 client 仍 200。
4. OAuth-only 用户(直接插 user + identity_link、无 credential)→ PUT password 不带 current → 204 → 之后能用新密 `/login`。
