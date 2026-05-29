# add-tokens-page-ui

## Why

CI/CD 与脚本要用 scoped API Token 调 CLI(`SWARMHIVE_TOKEN`),个人设备的 PAT 也需要可见可撤销。但 Admin **完全没有 token 管理页** —— 目前只能靠 `swarmhive login`(CLI)产生自己的 PAT,无法在网页里为 CI 铸造"只含指定权限"的 API Token,也无法查看 / 撤销已有 token。本提案补上这个一等运维入口。

## What Changes

- **新增顶层「令牌」页 `/tokens`**(与 应用 / 版本 同级):
  - **列表**:当前用户自己的 token —— name、prefix(`swhv_pat_AbC…`)、kind(PAT / API)、API token 的权限集、`last_used_at`、`expires_at`、状态(活跃 / 已撤销 / 已过期)。
  - **创建**(需 `token:manage`):选 kind —— PAT(继承本人实时权限)/ API(勾选**权限子集**,选项 = 自己当前的权限,`⊆ 自己`);填 name;可选 `expires_at`。明文 token **仅一次性**展示(Modal + 复制),关闭后不可再得。
  - **撤销**:Popconfirm → `DELETE /tokens/:id`(幂等)。
- 点亮顶层菜单「令牌」项 + 路由 guard 继承 `_auth`。

## Capabilities

### New Capabilities
- `tokens-page-ui`: Admin 的 token 管理页 —— 列出本人 token、创建 PAT / 带权限子集的 API Token(一次性展示明文)、撤销;权限门控(创建需 `token:manage`)。

### Modified Capabilities
（无 —— 消费 `add-pat-and-api-token` 既有端点,不改其需求。）

## Impact

- **admin**:新增 `routes/_auth/tokens.tsx` + `lib/api/tokens.ts`(types + query options + create/revoke helper + 状态/标签纯函数);`routes/_auth/route.tsx` 顶层菜单加「令牌」;`usePermissions` 门控创建。新依赖:无(`expires_at` 用既有 dayjs + AntD DatePicker)。
- **server / api-types / entity**:**零改动**(端点 `POST/GET /api/v1/tokens`、`DELETE /api/v1/tokens/:id` + DTO 已由 `add-pat-and-api-token` 提供)。
- **docs**:`dev-notes/knowledge/admin-spa.md` 加 token 页范式(一次性明文展示、权限子集多选 = me.permissions、状态推导)。
- **测试**:状态推导 / 权限选项 纯函数 Vitest;整页渲染 + e2e 沿用 foundation harness gap 暂缓。

## Non-goals

- **不做"管理他人 token"**:v1 只管自己的(后端 `GET ?owner=` + token:manage 列他人留后续)。理由:生成的 token 权限必然 `⊆ 自己`,跨用户治理是另一层 RBAC 运维需求,非首版必需。
- **不做 per-app / per-channel scoped token**:当前后端 `CreateTokenRequest` 只有扁平 `permissions` 子集,无 app/channel scope 字段;真要细到资源级是后端先扩 DTO 的事。
- **不改 token 后端 / 鉴权链路**:bearer resolve、`/auth/cli-token`、权限子集校验都不动。
- **不在页面内嵌 CLI 引导文档**:只给明文 + 复制 + 用法提示一行,完整 CLI 用法在 docs/12。

## Depends on

- `add-pat-and-api-token`(已归档)—— tokens 端点 + `ApiToken` / `CreateTokenRequest` / `CreateTokenResponse` DTO + 权限子集校验。
- `add-admin-frontend-foundation`(已归档)—— Provider 链 / auth guard / typed client / `usePermissions`。

## Maps to docs

- `docs/13-rbac.md` —— scoped API Token + PAT + 权限矩阵。
- `docs/12-cli.md` —— CI 用 `SWARMHIVE_TOKEN`(env)的发布路径。
- `docs/08-admin-and-analytics.md` —— Admin 运维入口。
