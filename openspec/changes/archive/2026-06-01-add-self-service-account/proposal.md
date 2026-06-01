# add-self-service-account

## Why

当前「当前登录用户自己的账户」被劈成两个页面、两个入口:

- `/profile`(头像下拉进入)只管 OAuth 登录方式(绑定/解绑 GitHub)。
- `/settings/account`(设置菜单第一项,对所有人可见)只读展示邮箱/显示名/验证状态。

二者都是个人级数据,却拆散在两处;而且正因为「账户」要对所有人可见,「设置」菜单不得不对**毫无 `*:manage` 权限的普通用户**也显示(`_auth/route.tsx` 的 "Account is everyone's own profile" 妥协)。同时后端**没有**任何 self-service 修改 endpoint——用户改不了自己的显示名,也改不了密码(OAuth-only 用户更无法设置密码以便解绑 GitHub)。

本 change 把个人账户统一到 `/profile`,让「设置」回归纯组织/部署级配置(manage 门控),并补齐 self-service 编辑(显示名 + 密码)。

## What

**后端(新 endpoint 集合,新 vertical-slice `routes/account.rs`,挂 `sensitive_routes` 受 governor 限流):**

- `PATCH /api/v1/users/me { display_name }` — 任意 Active principal,改自己的显示名(trim 后 1..=100 字符),返回更新后的 `User`。
- `PUT /api/v1/users/me/password { current_password?, new_password }` — 任意 Active principal:
  - 已有 `user_credentials` 行 → `current_password` 必填且 argon2 校验,错误 `422 current_password_incorrect`;
  - 无 credential(OAuth-only)→ 视为「设置密码」,忽略 `current_password`;
  - `new_password` 走 `validate_strong_password`(弱口令 `422 password_too_weak`);
  - 写库后**踢掉该用户其它所有 session**,保留当前 session;写 `audit_log`;返回 `204`。
- 把 `password_reset.rs` 的私有 `upsert_credentials` / `revoke_user_sessions` 提升到 `auth/service.rs`(满足「≥2 route 文件复用」规则)。

**前端(方向 A:并进 `/profile`):**

- `/profile` 吸收账户信息(邮箱/显示名/验证状态 + 重发验证)+ 可编辑显示名 + 改/设密码表单 + 既有 OAuth 登录方式。
- 删除 `_auth/settings/account.tsx`;「设置」子菜单去掉「账户」项 + 去掉 everyone 例外,整体 `canManageSettings` 门控。
- `_auth/settings/index.tsx` 重定向改到首个 enabled 的 manage 模块(无 manage 权限则回 `/profile`)。

## Acceptance

- `cargo test -p swarmhive-server account_smoke` 全绿:改名持久化、错误 current 拒绝、OAuth-only 设密成功、弱口令拒绝、改密后其它 session 失效。
- `pnpm openapi` 无 drift、`tsc -b` + `pnpm admin:build` + `pnpm lint` 全绿。
- 手动:头像下拉「个人资料」改名后刷新仍在;改密码后其它设备下次请求被踢登录;普通用户(无 manage 权限)登录后**看不到**「设置」菜单,个人账户走头像下拉。

## Non-goals

- **改邮箱**(需重新验证流程,单独 change)。
- 头像上传 / 删除账号 / 注销。
- 管理员改他人资料或密码(那是 `user:manage` 范畴;本 change 严格 self)。
- 2FA / passkey / 「我的会话」列表与逐个撤销 UI(未来)。

## Depends on

- `add-oauth-github-and-provider-config`(identity links + `user_credentials` 1:1 + OAuth-only 用户)。
- `add-login-and-owner-bootstrap-ui`(`validate_strong_password` 强度规则)。
- `add-invite-and-password-reset`(被提升的 `upsert_credentials` / `revoke_user_sessions`)。

## Maps to docs

- `docs/13-rbac.md` — 新增「Self-service account」段(个人 vs 组织设置的可达性分层)。
- `docs/08-admin-and-analytics.md` — Profile 页职责。
