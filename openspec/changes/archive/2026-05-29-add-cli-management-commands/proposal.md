# add-cli-management-commands

## Why

CLI 目前对 apps / releases / artifacts 只能 **list**,channel 的 `promote` / `rollback` 还是 `todo!()` 桩;真正的管理(建 / 改 / 删 app、建 / 改 / 发布 / 撤回 release、channel promote / rollback)只能去 Web Admin。但用户会让 **AI 帮忙操作**,而 AI 天然走 CLI / 脚本。要让 CLI 成为后台管理的一等公民,得把发布管理线补齐成与 Web Admin 对齐的 `<名词> <动词>` 全集,并让输出 / 错误**机器可解析**——这是后续配套 skill(包 CLI 给 AI 用)的解析契约地基。

## What Changes

- **apps**:`get`(详情)/ `create` / `update`(PATCH,slug 不可变)/ `delete --yes`(409 if 有 release)。`list` 保留。
- **channels**(新名词组,收编原 top-level `promote` / `rollback` 桩):`list` / `create` / `set-default` / `promote --version` / `rollback [--to-version]`。
- **releases**:`get`(详情)/ `create`(建 draft,不上传)/ `update`(PATCH)/ `publish`(发布已存在 draft)/ `yank --yes`。`list` 保留。
- **artifacts**:`list` 保留(本期不加 delete)。
- **AI 友好横切层**:
  - 所有命令(含写操作)honor 全局 `--output {table|json}`;`json` 时成功结果以对象 / 数组打到 **stdout**。
  - API 错误以 **RFC 9457 problem+json 打到 stderr** + **非零 exit code**(当前错误是 anyhow 人话串)。
  - 全非交互(token 走 `SWARMHIVE_TOKEN` env / credentials.toml);破坏性操作(`delete` / `yank`)强制 `--yes`。
- `client.rs` 加 `patch_json` / `delete_no_content` helper(当前只有 GET/POST)。

## Capabilities

### New Capabilities
- `cli-management`: CLI 对 apps / channels / releases 的完整管理命令(CRUD + 生命周期 + promote/rollback),以及 AI 友好的 `--output json` 成功 / problem+json 错误 / 非零 exit / `--yes` 契约。

### Modified Capabilities
（无 —— 消费 `add-app-release-artifact` 既有端点,不改其需求。）

## Impact

- **swarmhive-cli**:`commands/{apps,releases}.rs` 扩出写动词 + 新 `commands/channels.rs`;`main.rs` 命令树扩 `AppsCommand` / `ReleasesCommand` / 新 `ChannelsCommand`,移除 top-level `Promote` / `Rollback` 桩;`commands/client.rs` 加 `patch_json` / `delete_no_content` + 错误结构化(parse problem+json)+ 顶层按 `--output` 渲染错误。
- **api-types**:**零改动**(`App` / `CreateAppRequest` / `UpdateAppRequest` / `ChannelView` / `CreateChannelRequest` / `UpdateChannelRequest` / `Release` / `CreateReleaseRequest` / `UpdateReleaseRequest` / `PromoteRequest` / `RollbackRequest` 都在,Web Admin 在用)。
- **server / entity**:**零改动**(18 个 endpoint 含 `get_app` / `get_release` 单查都已存在)。
- **docs**:`docs/12-cli.md` 命令清单补全 + JSON / 错误契约段;`dev-notes/knowledge/backend.md` 的 CLI 段补管理命令。
- **测试**:`client.rs` 错误解析 / `--output json` 渲染纯函数单测;CLI 管理链路 e2e 复用 `storage_smoke` 式 testcontainers(或新 `cli_management_smoke`)。

## Non-goals

- **不做 storage / mail 管理**:留给后续 `add-cli-storage-mail-admin`(有密钥处理 / 多行模板等不同关注点)。
- **不做 users / tokens 管理**:账号 / 凭证是安全敏感的 bootstrap 面,仍只在 Web。
- **不做 MCP server**:AI 操作走「AI 友好 CLI + 后续配套 skill」(skill 是独立产物,落 `.claude/skills/`,不在本 proposal)。
- **不改既有 `publish {tauri|android}`**:上传式发布保留;`releases publish` 是「发布已存在 draft」的不同操作,两者并存(docs 区分)。
- **不加 artifact 删除 / 重传**:本期 artifacts 仍只读。

## Depends on

- `add-app-release-artifact`(已归档)—— apps / channels / releases endpoint + DTO。
- 现有 CLI 基建(`commands/client.rs` 的 `OutputFormat` / `emit` / `require_creds`、`auth::resolve`)。

## Maps to docs

- `docs/12-cli.md` —— CLI 命令设计 + 与 Web Admin 的关系。
- `docs/13-rbac.md` —— 管理命令的 verb-scoped 权限(AI 用最小权限 API Token)。
