# add-cli-release-policy

## Why

`UpdateReleaseRequest`(api-types)与 server update handler 早已支持灰度 / 强更策略列(`rollout_percent`/`min_version`/`android_min_version_code`),Admin UI 也刚补齐编辑(`add-release-policy-edit-ui`)。但 **CLI `releases update`/`create` 把这三个字段硬编码成 `None`/`Default`**(`commands/releases.rs` 注释明写「CLI 暂不暴露…本 change 不动 CLI」)——**CLI 是一等发布入口**,灰度发布与强制更新却只能走 UI/API,CI 流水线无法纯 CLI 调灰度。补齐 CLI parity。

## What

- **`main.rs`**:
  - `ReleasesCommand::Update` 加 `--min-version <semver>` / `--rollout-percent <1-100>` / `--android-min-version-code <int>`(都 `Option`)。
  - `ReleasesCommand::Create` 加 `--android-min-version-code <int>`(`CreateReleaseRequest` 已有该列;`min_version`/`rollout` 是 update-only,create 不设,与 UI 一致)。
  - dispatch 透传新参数。
- **`commands/releases.rs`**:`update`/`create` 函数签名加对应参数,直接填进请求 body(CLI 的清空语义比 UI 简单——**flag 直接映射 `Option<field>`:省略=不改,传值=设;清灰度传 `--rollout-percent 100`、清强更下限传 `--min-version 0.0.0`**,由 `#[arg]` help 文案〔英文〕说明,无需 compare-to-initial)。
- **`ReleaseRow` 表格**:加 `rollout` / `min ver` 列,让 `releases list`/`get` 的 table 输出也能看到当前策略(`--output json` 本就含全字段)。

## Acceptance

- `cargo build -p swarmhive-cli`;`clippy --workspace --all-targets -- -D warnings`;`fmt --check`。
- `swarmhive releases update --help` / `create --help` 显示新 flag;`releases get --output json` 含 `rollout_percent`/`min_version`/`android_min_version_code`。
- CLI 单测(若加纯函数)+ `cargo test -p swarmhive-cli`。
- `cargo tree -p swarmhive-cli | grep -E "sea-orm|swarmhive-entity"` 仍空(CLI 不漏 entity)。

## Non-goals

- **零 server / 零 api-types 改动**——端点与 DTO 已支持全部字段。
- **不**在 CLI 引入 UI 那套 compare-to-initial 清空逻辑;CLI 是显式命令式接口,sentinel(`0.0.0`/`100`)由用户在 flag 里显式给,help 文案说明即可。
- **不**做 `publish {tauri|android}` 上传式发布时设策略(那是 draft→上传链路;策略走 `releases update` 在发布前后单独调)。

## Depends on

`add-cli-management-commands`(`releases` 命令宿主,已归档)+ `add-update-check-tauri`/`add-update-check-rn-android`(策略列 + update handler,已归档)+ `add-release-policy-edit-ui`(UI parity 对照,已归档)。

## Maps to docs

- `docs/12-cli.md`(releases 命令)
- `docs/08-admin-and-analytics.md`(Policies——CLI 与 UI 双入口)
