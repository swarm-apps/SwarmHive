# cli-management Specification

## Purpose
TBD - created by archiving change add-cli-management-commands. Update Purpose after archive.
## Requirements
### Requirement: CLI SHALL manage apps

The CLI SHALL provide `apps {list, get, create, update, delete}` against the apps endpoints. `create` takes `--slug`, `--display-name`, `--platforms` (comma-separated). `update` takes `--app <slug>` and mutable fields (NOT slug). `delete` requires `--app <slug>` and `--yes`. `get` returns one app's detail. All honor the global `--output`.

#### Scenario: Create an app

- **WHEN** the user runs `swarmhive apps create --slug swarmdrop --display-name SwarmDrop --platforms tauri-desktop`
- **THEN** the CLI POSTs to `/api/v1/apps` and prints the created app
- **AND** a duplicate slug surfaces the server `conflict` error with a non-zero exit

#### Scenario: Delete requires --yes

- **WHEN** the user runs `swarmhive apps delete --app swarmdrop` without `--yes`
- **THEN** the CLI refuses and exits non-zero without calling the server
- **AND** with `--yes` it DELETEs `/api/v1/apps/swarmdrop` (and surfaces `app-has-releases` if the app still has releases)

### Requirement: CLI SHALL manage channels

The CLI SHALL provide `channels {list, create, set-default, promote, rollback}`, all `--app <slug>`-scoped. `create --name`, `set-default --name`, `promote --name --version`, `rollback --name [--to-version]`. The legacy top-level `promote` / `rollback` stub commands SHALL be removed in favor of this group.

#### Scenario: Promote a channel to a version

- **WHEN** the user runs `swarmhive channels promote --app swarmdrop --name stable --version 0.4.5`
- **THEN** the CLI POSTs to `/api/v1/apps/swarmdrop/channels/stable/promote`
- **AND** promoting a non-published version surfaces the typed conflict error

#### Scenario: Rollback without an explicit version

- **WHEN** the user runs `swarmhive channels rollback --app swarmdrop --name stable`
- **THEN** the channel reverts to the previous distinct release
- **AND** with no rollback history the CLI surfaces `nothing-to-rollback` (422) and exits non-zero

### Requirement: CLI SHALL manage releases

The CLI SHALL provide `releases {list, get, create, update, publish, yank}`, all `--app <slug>`-scoped. `create --version [--android-version-code] [--notes-file]` makes a draft (no upload). `update --version` patches mutable fields. `publish --version` publishes an existing draft. `yank --version --yes` yanks. This is distinct from `publish {tauri|android}`, which uploads artifacts.

#### Scenario: Create a draft then publish it

- **WHEN** the user runs `swarmhive releases create --app swarmdrop --version 0.4.6` then `swarmhive releases publish --app swarmdrop --version 0.4.6`
- **THEN** the first POSTs a draft release and the second POSTs `/releases/0.4.6/publish`
- **AND** publishing a release with no artifacts surfaces the server validation error

#### Scenario: Yank requires --yes

- **WHEN** the user runs `swarmhive releases yank --app swarmdrop --version 0.4.5` without `--yes`
- **THEN** the CLI refuses and exits non-zero; with `--yes` it POSTs `/releases/0.4.5/yank`

### Requirement: CLI SHALL emit machine-readable output and errors

Every command SHALL honor the global `--output {table|json}`. With `--output json`, successful results SHALL print as a JSON object/array to stdout. API errors SHALL be parsed as RFC 9457 problem+json; with `--output json` the problem SHALL be written to stderr, and all failures SHALL exit with a non-zero code. This is the stable contract a companion skill / AI parses against.

#### Scenario: JSON success on stdout

- **WHEN** the user runs `swarmhive apps create --output json ...`
- **THEN** the created app prints as a JSON object on stdout with a zero exit code

#### Scenario: JSON problem on stderr with non-zero exit

- **WHEN** a command hits an API error and `--output json` is set
- **THEN** the RFC 9457 problem+json is written to stderr
- **AND** the process exits with a non-zero code (stdout carries no partial success object)

### Requirement: CLI management SHALL be non-interactive

All management commands SHALL run without prompts: the bearer token comes from `SWARMHIVE_TOKEN` env or `credentials.toml`, and every mutation takes its inputs as flags. No management command SHALL block on interactive input (so AI / CI can drive it unattended).

#### Scenario: Runs unattended with an env token

- **GIVEN** `SWARMHIVE_TOKEN` is set in the environment
- **WHEN** any management command runs in a non-TTY context
- **THEN** it completes without prompting for credentials or confirmation (destructive ops still require the `--yes` flag, not an interactive prompt)

### Requirement: CLI SHALL set release rollout and force-update policy
The `swarmhive releases update` command SHALL expose flags to set a release's gray-rollout percentage and force-update floors (`--rollout-percent`, `--min-version`, `--android-min-version-code`), and `swarmhive releases create` SHALL expose `--android-min-version-code`, mapping each provided flag to the corresponding `UpdateReleaseRequest`/`CreateReleaseRequest` field so gray release and force update are configurable from CI/CLI, at parity with the Admin UI.

#### Scenario: Set rollout and floor via CLI
- **WHEN** an operator runs `swarmhive releases update --app a --version 1.2.0 --rollout-percent 50 --min-version 1.0.0`
- **THEN** the request sets `rollout_percent=50` and `min_version=1.0.0` on that release

#### Scenario: Omitted policy flags leave values unchanged
- **WHEN** an operator runs `releases update` without any policy flag
- **THEN** the policy fields are sent as absent (no change), preserving the stored values

#### Scenario: Clearing uses explicit sentinels
- **WHEN** an operator passes `--rollout-percent 100` or `--min-version 0.0.0`
- **THEN** gray rollout is disabled (full) or the force-update floor is removed, matching the server's single-Option sentinel semantics

### Requirement: publish 的 notes 更新条件化且后置

`swarmhive publish` 对已存在 release 的 release notes 更新(PATCH)SHALL 仅在 notes 内容实际发生变化时执行;notes 未变化时 MUST 跳过 PATCH,从而不触发 `release:update` 权限检查。notes 的 PATCH SHALL 发生在 artifact 上传之后,使「上传 artifact」这条关键路径不被「改 notes」的权限检查阻塞。CLI SHALL 提供 `--skip-notes-update` 显式跳过 notes 更新。

#### Scenario: 重发且 notes 未变跳过 PATCH
- **WHEN** 对一个已存在 release 重新 publish/补传 target,且传入的 notes 与服务端现有 notes 相同
- **THEN** CLI MUST NOT 发起 notes PATCH,即便 token 缺 `release:update` 也能正常上传 artifact

#### Scenario: notes 变化时才更新且在上传后
- **WHEN** 传入的 notes 与服务端现有 notes 不同
- **THEN** CLI 在 artifact 上传完成之后才 PATCH notes;若 PATCH 因权限失败,已上传的 artifact MUST 不被回滚

### Requirement: publish 默认上传到 draft,发布由 finalize 显式触发

`swarmhive publish` SHALL 默认只上传 artifact 并保持 release 为 draft(不发布)。CLI SHALL 提供 `release finalize` 子命令调用 server 的 finalize 端点完成发布。多 target 发布的推荐流程为:N 个 target 各自 publish(到 draft)→ 最后一次 `release finalize`。

#### Scenario: publish 默认不发布
- **WHEN** 在不带显式发布意图的情况下运行 `swarmhive publish`
- **THEN** artifact 上传成功且 release MUST 保持 draft

#### Scenario: finalize 子命令发布 release
- **WHEN** 所有 target 上传完成后运行 `swarmhive release finalize --app <slug> --version <v>`
- **THEN** CLI 调用 finalize 端点,release 变为 published

### Requirement: CLI 退出码区分永久错误与可重试错误

CLI 进程退出码 SHALL 区分故障类型:永久性错误(401/403/409/422 等权限、配置、冲突)MUST 以 `exit 2` 退出;可重试错误(408/429/5xx、网络/超时)MUST 以 `exit 1` 退出;成功为 `exit 0`。错误响应 SHALL 携带 `retryable` 信号供调用方(如 CI/action)据此决定重试或立即失败。

#### Scenario: 权限错误是永久失败
- **WHEN** 一次操作返回 403(缺权限)
- **THEN** CLI MUST 以 `exit 2` 退出并打印明确的永久错误(含 required_permission)

#### Scenario: 服务端不可用是可重试失败
- **WHEN** 一次操作遇到 5xx 或网络超时
- **THEN** CLI MUST 以 `exit 1` 退出并标注该失败可重试

### Requirement: tokens create 提供 CI 发布预设

`swarmhive tokens create` SHALL 支持 `--preset ci-publish`,一次性展开为 CI 发布所需的完整权限集,且 MUST 包含 `release:update`(本次事故缺失的权限)。用户使用预设时 MUST NOT 需要手工逐项勾选权限。

#### Scenario: ci-publish 预设含完整发布权限
- **WHEN** 运行 `swarmhive tokens create --kind api --preset ci-publish --name <n>`
- **THEN** 创建的 token 权限集 MUST 至少包含 `app:read, release:read, release:create, release:update, release:publish, release:promote, artifact:upload`

