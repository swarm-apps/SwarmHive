# add-login-and-owner-bootstrap-ui

## Why

`add-auth-and-rbac` 已经把 server 侧的 `/api/v1/auth/login` + `/api/v1/setup` + session + Principal extractor + argon2 hash 全部落齐，但 admin SPA 的 `/login` 仍是占位 Card + Alert "尚未实现"，`/setup` 完全没有 web UI —— 当前 Owner bootstrap 必须 ssh 上去看 stdout 抓 setup_token 再 POST。

这一层缺位会卡住整条用户旅程：

- 部署后第一时间想点 admin 看效果 → 没有可用 login → 必须开终端看 server 日志 → 反 PaaS UX
- 后续每个新成员入口（邀请 / OAuth / 自助注册）都要回到 `/login` 收尾，login 不真做完，后面所有路径都断
- bootstrap 的 setup_token 模型本身也跟 Coolify 式"裸 web 注册即 root"的主流 self-host UX 不一致

本 proposal 把 admin SPA 的 `/login` + `/setup` 一次做完，同时把 server 的 bootstrap 模型从 "setup_token + stdout" 改成 "Coolify 式 + 可选 ENV 锁定"，作为后续所有账号能力 proposal（mail / oauth / invite / self-register）的"前置入口"。

## What Changes

### Server 侧

- **删 setup_token 模型**：
  - 删 `setup_token` 实体 + 启动期生成 / stdout 打印逻辑
  - 删 `GET /api/v1/setup/info` 中 token 相关字段
  - `POST /api/v1/setup` 请求体去掉 `token` 字段；仅接受 `{ email, display_name, password }`
- **加 bootstrap window 守门**：
  - `POST /api/v1/setup` 只在 user 表为空时允许（user 表非空 → 410 Gone + problem+json `"bootstrap_already_complete"`）
  - `GET /api/v1/setup/info` 返回 `{ needs_bootstrap: bool, locked_email: Option<String> }`
- **可选 ENV 锁定**：
  - 启动期读 `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL`（可空）
  - 设置时 `/setup/info` 返回 `locked_email: Some(value)`；`/setup` 请求 email 字段不匹配 → 422 problem+json
- **加账号级失败锁定（baseline）**：
  - 新增 `user_login_attempts` 实体（`user_id` PK, `failed_count`, `last_failed_at`, `locked_until`）
  - `/api/v1/auth/login` 失败 → 累加 `failed_count`；`failed_count >= 5` → 设 `locked_until = now() + 30min`
  - 锁定期间登录 → 410 + problem+json `"account_locked_until"`，body 含 `locked_until` timestamp（供 UI 倒计时）
  - 登录成功 → 清空 `failed_count` + `locked_until`
- **密码强度（baseline，garde 后端校验）**：
  - `/setup` 的 password 字段：≥12 字符 + 至少 3 类（大写 / 小写 / 数字 / 特殊符）+ 不在内置弱口令字典（top-100）
  - `/auth/login` 不强校验（向后兼容已存账号；强校验只在 set/change/reset 路径）

### Admin SPA 侧

- **`/login` 真实实现**（替换占位 Card）：
  - ProForm + 邮箱 + 密码 + "记住我" + "忘记密码" 链接（暂禁用，等 ④ 落地）
  - submit → `fetchClient` POST `/api/v1/auth/login` → 成功 navigate to `search.next ?? '/'`
  - 错误：解析 problem+json → 按 type 分支：`account_locked_until` 显示倒计时 / 其它统一 `invalid_credentials` 提示
  - 全文案 `<Trans>` 包裹（Lingui v6）
- **`/setup` 新建路由**：
  - mount 在 SPA 顶层（无 auth guard）
  - load 时调 `GET /api/v1/setup/info`，`needs_bootstrap: false` → redirect `/login`
  - ProForm + email + display_name + password + confirm；`locked_email` 时 email 字段 disabled + 预填
  - submit → POST `/api/v1/setup` → 成功后 server 已写 session cookie → navigate `/`
- **bootstrap-aware router 分流**：
  - root `beforeLoad` 调 `/api/v1/setup/info`（缓存）
  - `needs_bootstrap: true` + 访问任意 path → redirect `/setup`
  - `needs_bootstrap: false` + 访问 `/setup` → redirect `/login`
- **i18n 文案补**：补 `/login` + `/setup` 的 zh-CN PO 条目（约 20 条）
- **Vitest 单测**：登录表单 happy path + 锁定 problem+json 渲染倒计时 + `/setup` locked_email disabled 渲染

## Capabilities

### New Capabilities

- `login-and-bootstrap`：Admin SPA 的登录 + Owner bootstrap 引导 + 账号级软锁的可观测行为契约。

### Modified Capabilities

- 隐式废弃 archived `add-auth-and-rbac` 中 setup_token 相关 Requirements —— 不修改 archive，但本 proposal specs 中 ADDED Requirements 包含"bootstrap MUST NOT require token"明确覆盖。

## Impact

- **Code**：server 端 ~5 文件改动（删 setup_token 实体 + bootstrap window guard + login_attempts 表 + login handler 加锁逻辑）；admin SPA ~4 文件新增（`routes/login.tsx` 重写、`routes/setup.tsx` 新建、`lib/api/setup.ts` queryOptions、`__root.tsx` beforeLoad 分流）
- **DB**：删 `setup_token` 表 + 加 `user_login_attempts` 表（注意：项目用 sea-orm `Schema::create_table_from_entity` 自动同步，无独立 migration crate）
- **API**：`POST /api/v1/setup` 请求体破坏性变更（去 token 字段）；`GET /api/v1/setup/info` 响应字段变更（去 token 相关，加 `locked_email`）；`POST /api/v1/auth/login` 响应错误类型新增 `account_locked_until`
- **OpenAPI**：schema 漂移 → CI drift gate 会要求 admin 端 regen `schema.gen.ts`（已有机制）
- **Deps**：admin 无新依赖；server 引入 `garde`（如未已用）做密码强度校验
- **不影响**：CLI / PAT / Token / Storage / RBAC

## Non-goals

- **不实现**：忘记密码 / 重置密码 UI（合到 ④ `add-invite-and-password-reset`）
- **不实现**：OAuth 按钮 / GitHub 登录入口（合到 ③ `add-oauth-github-and-provider-config`，本 proposal 仅留 `/login` 容器位置）
- **不实现**：邮箱验证 / pending_verify / pending_approval 状态（合到 ⑤ `add-registration-policy-and-self-register`）
- **不实现**：MFA / 2FA、HIBP 密码检查、CAPTCHA（self-host 主旨 + NTH）
- **不引入** `axum-login` / `axum-valid` / `argon2-async`（沿用 archived auth proposal 的自写 extractor + 同步 argon2）
- **不修改** archived `add-auth-and-rbac` 的 spec（在本 proposal specs 里以 ADDED Requirements 覆盖）

## Depends on

- `add-auth-and-rbac`（archived）—— 提供 session、Principal、argon2、`/api/v1/auth/login` 基础
- `add-pat-and-api-token`（archived）—— 提供 Bearer / CLI token 兼存模型，本 proposal 不动 PAT 流程
- `add-admin-frontend-foundation`（archived）—— 提供 Provider 链、`/login` 占位、auth guard、Lingui、$api client、错误链

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) Admin 技术栈段（bootstrap UX 句一行）
- [docs/13-rbac.md](../../../docs/13-rbac.md) Bootstrap Owner 段（更新为 Coolify 模式描述）
- [dev-notes/knowledge/backend.md](../../../dev-notes/knowledge/backend.md) auth 段（补 bootstrap window + login_attempts）
- [dev-notes/knowledge/admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) 补 `/setup` 引导段
- [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md) 决策源

## Acceptance

- 空 DB 启 server + admin SPA → 访问 `http://localhost:5173` → 自动跳 `/setup` → 填表单 → 提交 → 跳 `/`（已登录 Owner）
- 已 setup 的 DB → 访问 `/setup` → 跳 `/login`
- 设 `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL=me@x.com` → `/setup` email 字段固化为 me@x.com 且 disabled；改值提交 → 422 problem+json
- 连续 5 次错密码 → 第 6 次返 410 + problem+json `account_locked_until`；UI 显示倒计时
- 正确密码登录后 `user_login_attempts.failed_count` 清零
- `/setup` 提交弱密码（"password123" 等）→ 422 problem+json，UI 字段下显示 i18n 化错误提示
- `pnpm lint` / `cargo clippy` / `cargo test --workspace` / `pnpm --filter @swarmhive/admin test` 全绿
- OpenAPI drift gate 通过（admin 已 regen `schema.gen.ts` 并 commit）
