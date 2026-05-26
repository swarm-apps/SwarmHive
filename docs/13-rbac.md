# RBAC 权限模型

## 设计结论

SwarmHive MVP 做 **单组织 + 完整 RBAC**，不做真正多租户。

原因：SwarmHive 涉及高风险操作，包括上传安装包、发布 stable、强制更新、rollback、配置 S3/RustFS 密钥、管理 CI Token、查看下载与埋点数据。权限边界应从第一版就建立。

## 多租户边界

MVP 只有一个默认组织：

```text
Default Organization
  └─ Apps
      └─ Releases
          └─ Artifacts
```

但数据库核心表预留 `org_id`，未来可以演进为多组织 / managed cloud。

第一阶段不做：

- 多组织切换。
- 组织级计费。
- 组织间强隔离策略。
- 配额管理。

## 角色

### Owner

系统所有者。

能力：

- 管理用户。
- 管理角色。
- 管理存储配置。
- 管理 API Token。
- 管理所有应用、版本、策略。
- 查看所有统计和埋点。

### Admin

应用和发布后台管理员。

能力：

- 管理应用。
- 管理版本。
- 管理更新策略。
- 创建 release。
- 查看统计。

限制：

- 不能管理 Owner。
- 不能修改系统级敏感配置，除非显式授予。

### Release Manager

发布负责人。

能力：

- 发布版本。
- promote。
- rollback。
- yank。
- 上传 artifact。

### Developer

开发者。

能力：

- 上传 draft / beta 产物。
- 查看自己有权限的应用和版本。

限制：

- 不能发布 stable。
- 不能强制更新。
- 不能管理 storage/token。

### Viewer

只读角色。

能力：

- 查看应用。
- 查看版本。
- 查看下载量。
- 查看埋点漏斗。

## Permission 列表

角色只是 permission 集合。服务端鉴权应按 permission 判断。

### System

- `system:manage`
- `user:manage`
- `role:manage`
- `token:manage`
- `storage:manage`

### App

- `app:create`
- `app:read`
- `app:update`
- `app:delete`

### Release

- `release:create`
- `release:read`
- `release:update`
- `release:publish`
- `release:promote`
- `release:rollback`
- `release:yank`

### Artifact

- `artifact:upload`
- `artifact:read`
- `artifact:delete`

### Analytics

- `analytics:read`
- `telemetry:read`

## Scope

权限支持作用域。

MVP 支持：

- org-level role。
- app-level role。
- token app scope。
- token channel scope。

示例：

```text
User A: org Owner
User B: app swarmdrop Developer
User C: app swarmnote-rn Release Manager
```

## Identity Providers

用户身份在 MVP 即支持多种来源，模型上通过 `User` + `IdentityLink` 拆分：

```text
User (id, email, display_name, status, created_at)
  └─▶ IdentityLink (user_id, provider, subject, metadata)
        provider ∈ { "password", "github", ...(future: google, gitlab, oidc) }
        subject  = provider 侧的稳定 ID（password: email；github: numeric user id）
```

MVP 首批：

- **password**：email + argon2id 密码 hash。
- **github**：通过 OAuth2 拿到 GitHub user → 写入 `IdentityLink(provider="github", subject=<github-id>)`；若 email 已存在则提示用户登录后绑定，避免账号分裂。

未来可加 Google / GitLab / 内部 OIDC，只需新增 provider 适配器。

## Bootstrap setup token

首次启动 server（`user` 表为空）时，server 自动颁发一次性 setup token 并打印到 **stdout**（带 ASCII 框，便于 `docker logs | grep` 提取）。运维流程：

```text
1. server 启动
   ╔════════════════════════════════════════════╗
   ║  SwarmHive first-run setup                 ║
   ║  Open the Admin SPA and POST /api/v1/setup ║
   ║  with this token:                          ║
   ║      <43-char base64url token>             ║
   ║  Token is one-shot and expires in 1 hour.  ║
   ╚════════════════════════════════════════════╝

2. 运维拿 token 访问 /setup（或调 POST /api/v1/setup）
3. 提交 { token, email, display_name, password } —— 创建 Owner + auto-login
4. 二次使用 token → 410 Gone（"setup token has already been used"）
5. 若 1 小时未消费 → 410 Gone（"setup token has expired"）
6. 若 user 表后续被外部清空 → 下次 server 启动会自动颁发新 token
```

**为什么是 stdout 而不是 env / file**：

- env：要 deployer 提前生成、注入，安全敏感（被 leak 风险高）
- file：容器/裸机部署差异大，写哪里、怎么读、权限怎么定都是麻烦事
- stdout：所有部署形态都能看（`docker logs`、systemd journal、PM2 logs），一次性、不持久化，与 Vaultwarden / Authelia 等 self-hosted 同行经验对齐

**token 安全属性**：

- 32 字节随机（OsRng）→ base64url-no-pad，明文 43 字符
- DB 只存 `blake3` hash；明文仅在 stdout 出现一次
- 1 小时 TTL；消费时翻 `used_at`，二次使用直接 410
- `/api/v1/setup` endpoint 走 `tower-governor` 限流（5 rps / burst 20，per-IP）

**相关文件**：`crates/swarmhive-server/src/auth/service.rs` (`issue_setup_token` / `register_owner`)、`crates/swarmhive-server/src/bin/server.rs` (`maybe_issue_setup_token` / `print_setup_banner`)。

## 三类凭证

SwarmHive 同时支持三种登录形态，统一在 server 端汇流成 `Principal { user, scope, permissions, auth_method }`：

| 凭证 | 用途 | 载体 | 撤销方式 |
| --- | --- | --- | --- |
| **Session cookie** | Admin SPA 浏览器登录 | HttpOnly + SameSite=Lax cookie，session 行存 Postgres | 登出 / 后台踢人 |
| **Personal Access Token (PAT)** | CLI 本地 `swarmhive login` 后的长期凭证 | `~/.config/swarmhive/credentials.toml`（0600），或 `SWARMHIVE_TOKEN` 环境变量覆盖 | 用户自己删 / Admin 撤销 |
| **API Token (scoped)** | CI/CD、机器对机器 | env：`SWARMHIVE_TOKEN` | Admin 撤销 / 自动过期 |

三者的 token 字符串都以高熵随机串生成，DB 只存 `blake3` hash，明文仅在创建时显示一次。**不引入 JWT**：单 binary monolith 下 stateless 验证没有收益，撤销 / scope 重发的复杂度大于价值。

## API Token

API Token 不等同用户角色，必须支持 scope。

示例：

```text
token name = swarmdrop-beta-ci
app = swarmdrop
channel = beta
permissions = artifact:upload, release:create, release:publish
expires_at = 2026-12-31
```

CI Token 推荐最小权限：

- beta 构建：`artifact:upload`, `release:create`, `release:publish`，scope 到 beta。
- stable promote：`release:promote`，单独 token 或人工审批后使用。

PAT 与 API Token 共用同一张表与同一份鉴权基建，区别只在 scope 默认值：PAT 默认 = 用户在 RBAC 下的全部权限；API Token = 创建者在 Admin UI 上勾选的子集。

## 敏感操作

以下权限需要特别保护：

- `storage:manage`：能配置 S3 / RustFS / OSS 密钥。
- `token:manage`：能创建 CI Token。
- `release:publish`：发布版本。
- `release:promote`：提升 channel。
- `release:rollback`：回滚 channel。
- `release:yank`：撤回版本。
- `analytics:read` / `telemetry:read`：可能涉及用户环境信息。

## 审计日志

关键操作必须写入 audit log：

- 登录成功 / 失败。
- 创建 / 删除用户。
- 修改角色。
- 创建 / 撤销 token。
- 修改 storage 配置。
- 发布 release。
- promote / rollback / yank。
- 修改强制更新策略。

审计字段：

- actor_type：user / token。
- actor_id。
- org_id。
- app_id。
- action。
- resource_type。
- resource_id。
- ip。
- user_agent。
- created_at。

## Admin UI 行为

- 无权限的按钮隐藏或禁用。
- 执行敏感操作前二次确认。
- 强制更新、rollback、yank 要显示影响范围。
- storage secret 不回显明文。
- token 只在创建时显示一次。

## 路线

MVP：

- 单组织。
- 多用户。
- 角色与权限。
- app-level role。
- scoped API Token。
- 审计日志。

后续：

- 多组织。
- 组织切换。
- 配额。
- managed cloud 隔离。
- 更细粒度数据权限。
