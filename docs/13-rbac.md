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
