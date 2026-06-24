# cli-project-init Specification

## Purpose
TBD - created by archiving change add-cli-publish-polish. Update Purpose after archive.
## Requirements
### Requirement: CLI SHALL scaffold swarmhive.toml via init

The `swarmhive init` command SHALL scaffold a `swarmhive.toml` in the current directory, local-only (no network calls), in two modes sharing one field set. Every field SHALL be available as a flag — `--server`, `--app`, `--platform <tauri|android>` (repeatable), `--tauri-conf`, `--android-apk`, `--force`, `--yes`, and the global `--output {table|json}` — and flags SHALL always take precedence over prompts and defaults. In a TTY without `--yes`, it SHALL prompt (via `dialoguer`) only for fields not supplied as flags: `server` (default = the logged-in credentials' server), `app.slug` (default = the current directory name), the target platforms (a multi-select pre-checked from on-disk detection), and the chosen platforms' `tauri.conf` / `android.apk` paths. With `--yes` or in a non-TTY context it SHALL NOT prompt and SHALL derive every field from flags plus detection defaults — so an AI / skill / CI can drive it unattended (matching the non-interactive management-command contract) — failing with a typed error and non-zero exit only when the required `app.slug` cannot be resolved. The `app.tauri.artifacts` list SHALL be emitted as a commented example block and SHALL NOT be prompted. It SHALL refuse to overwrite an existing `swarmhive.toml` unless `--force` is given, the generated file SHALL be parseable by the CLI's own config loader, and with `--output json` a successful run SHALL print a single JSON object to stdout while failures emit RFC 9457 problem+json to stderr.

#### Scenario: Interactive scaffold in a TTY

- **GIVEN** a project directory containing `src-tauri/`
- **WHEN** the user runs `swarmhive init` in a TTY without `--yes`
- **THEN** the CLI prompts (dialoguer) only for fields not given as flags — server, app slug, and platforms (tauri pre-checked from detection)
- **AND** it writes a `swarmhive.toml` that loads cleanly via the CLI's config loader

#### Scenario: Non-interactive flag-driven scaffold for AI / CI

- **GIVEN** a non-TTY context (or `--yes` passed in a TTY)
- **WHEN** an AI / skill runs `swarmhive init --app swarmdrop --platform tauri --yes --output json`
- **THEN** the CLI writes `swarmhive.toml` from the flags plus detection defaults without ever prompting
- **AND** it prints a single JSON object describing the result and exits zero

#### Scenario: Non-interactive without an app slug fails clearly

- **WHEN** `swarmhive init --yes` runs with no `--app` and no inferable directory name
- **THEN** the CLI does not prompt
- **AND** it exits non-zero with a typed error (RFC 9457 problem+json under `--output json`)

#### Scenario: Refuses to overwrite without --force

- **GIVEN** a `swarmhive.toml` already exists in the directory
- **WHEN** the user runs `swarmhive init`
- **THEN** the CLI refuses and exits non-zero
- **AND** it overwrites only when `--force` is passed

### Requirement: init 可一步打通 CI token 与 workflow 样板

`swarmhive init` SHALL 支持 `--setup-ci-token`,在生成 `swarmhive.toml` 之后,引导创建一个带 `ci-publish` 预设权限的 CI token,并打印把它写入 GitHub secret 的命令(`gh secret set SWARMHIVE_TOKEN`),同时生成一份可直接 copy-paste 的 release.yml 样板。在 `--json`(非交互/AI/CI)模式下,该命令 MUST 以结构化字段输出建议命令、secret 名与样板路径,不得有交互提示。

#### Scenario: setup-ci-token 打通接入第一步
- **WHEN** 运行 `swarmhive init --setup-ci-token`
- **THEN** 生成 `swarmhive.toml`、创建含 `release:update` 的 CI token、打印 `gh secret set SWARMHIVE_TOKEN ...`、并产出一份可用的 release.yml 样板

#### Scenario: json 模式无交互且字段完整
- **WHEN** 以 `swarmhive init --setup-ci-token --json` 运行(非 TTY)
- **THEN** 输出 MUST 为单个 JSON 对象,包含建议的 token 创建命令、`SWARMHIVE_TOKEN` secret 名与建议的 workflow 路径,且 MUST 不阻塞等待输入

