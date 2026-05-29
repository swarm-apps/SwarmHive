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

