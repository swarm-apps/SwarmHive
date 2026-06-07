## Why

`swarmhive` CLI 的本地/CI 发布链路与 docs/12-cli.md 的硬性 UX 契约有几处缺口,且唯一剩余的 stub `swarmhive init`(`main.rs` 仍是 `todo!`)让"开发者本地起步"断档——它还应像其它管理命令一样可被 AI/skill 非交互驱动,目前完全缺位:

- `publish {tauri|android}` **不注入 changelog**——`CreateReleaseRequest.release_notes` 写死 `None`、无 `--notes-file`,而 docs/06-cicd.md 把"自动注入 changelog"列为 CI/CD 一等目标。
- `publish` 缺 `--dry-run`(docs/12-cli.md 要求"所有发布命令支持 dry-run"),直接用 publish 无法预检。
- `publish`/`verify` 不输出 `--output json`——尽管 cli-management 的 "emit machine-readable output" 已要求 "Every command SHALL honor `--output`",这两个发布命令未达标,破坏 CI/AI 解析契约。
- 进度条无 TTY 守卫:无 TTY(CI)或 `--output json` 时会污染 stderr / 混入 JSON。

## What Changes

- 实现 `swarmhive init`,**双模式**(同一套字段):① 交互(TTY,dialoguer)对缺失字段 prompt;② **命令式/非交互**——全字段可由 flag 指定(`--server`/`--app`/`--platform tauri|android`(可重复)/`--tauri-conf`/`--android-apk`/`--force`/`--yes`/`--output json`),`--yes` 或非 TTY 时**绝不 prompt**、纯靠 flag + 探测默认生成,缺必填 `app.slug` 则 typed 报错 + 非零退出;flag 永远覆盖 prompt/默认。生成嵌套 `[app]`/`[app.tauri]`/`[app.android]`,`artifacts` 出注释示例块;已存在不覆盖(除非 `--force`)。**AI/skill/CI 可无人值守初始化**,与既有管理命令的非交互契约一致。
- `publish {tauri|android}` 新增 `--notes-file <path>`(可选 `--notes <text>`):把 release notes / changelog 注入 release——新建走 create、已存在走 release update,**复用既有 server 端点,零 server 改动**。
- `publish {tauri|android}` 新增 `--dry-run`:presign 前预检(定位产物、算 sha256、查同名 `.sig`、打印发布计划)后返回,不发起任何上传。
- `publish {tauri|android}` 与 `verify {tauri|android}` 落实 `--output json`:成功时输出结构化 JSON(失败已走 RFC 9457 problem+json + 非零退出)。
- 进度条 TTY 守卫:无 TTY 或 `--output json` 时禁用进度条。
- **统一交互 prompt 到 dialoguer**:把 CLI 仅存的手写交互——`client.rs::resolve_secret` 读密钥的 TTY 分支(`rpassword::prompt_password`)——改用 `dialoguer::Password`,并**移除 `rpassword` 依赖**,让 dialoguer 成为 CLI 唯一交互框架。secret 三路输入契约(`--secret-stdin` > env > 明文 flag > TTY 提示)**不变**,仅换提示实现。

## Capabilities

### New Capabilities

- `cli-project-init`: `swarmhive init` 生成 `swarmhive.toml`(项目本地发布起步),双模式——交互式(TTY)+ 命令式/非交互(`--yes` 或非 TTY,AI/skill/CI 可驱动);与既有 `cli-device-login`(login)同属"允许交互的 CLI 命令",边界上独立于非交互的 `cli-management`。

### Modified Capabilities

- `storage-and-presign-upload`: "CLI SHALL verify and publish artifacts via presign + complete" 要求新增 `--notes-file`/`--notes`(changelog 注入)、`--dry-run`(publish 预检)、`publish`/`verify` 的 `--output json` 成功负载,以及进度条的 TTY/JSON 守卫。

## Non-goals

- 零 server / entity / api-types / schema 改动(release notes 复用既有 release create/update 端点;presign/complete 不变)。
- `init` 纯本地、不联网(不拉 `/apps` 列表);不做多 ABI 自动识别(fat APK `abi=None` 已默认支持)。
- 不改 GitHub Action 逻辑(已用 `dry-run→verify` verb 切换);至多同步 `action.yml` 文档。
- secret 三路输入**契约不变**——只换提示框架(`rpassword`→`dialoguer`),不动 `--secret-stdin`/env/flag 优先级与"省略=保留"语义;**不改 `storage-cli-admin`/`mail-cli-admin` 的 spec**(纯实现层重构)。
- 不引 `inquire`/`cliclack`;`dialoguer`(共享 `console`)是唯一新增交互依赖。
- 不加 telemetry/events CLI 命令(归 Admin)。

## Impact

- 主要在 `crates/swarmhive-cli`(`main.rs` 派发 + `commands/{project,publish,verify,client}.rs`);**零** server / entity / api-types / schema 改动——release notes 走既有 `releases create`/`update` 端点,presign/complete 不变。
- `dialoguer`(console-rs,与已有 `indicatif` 共享 `console`,边际依赖低)成为 CLI **唯一交互框架**:`init` 富交互 + `resolve_secret` 的密钥 TTY 提示;**移除 `rpassword`**(root + cli `Cargo.toml`)。`std::io::IsTerminal` 做 TTY 判定(无 TTY 不阻塞)。需在 workspace 根 pin `dialoguer`。
- `resolve_secret` 重构波及 storage/mail 的密钥交互输入路径(`storage create/update`、`mail providers create/update`),**行为保持一致**(三路 + TTY 提示),仅提示框架变更——需回归这两条命令的交互读取。
- docs 同步:docs/12-cli.md(`init` / `--dry-run` / `--output json` 段)、docs/06-cicd.md(changelog 注入);顺手修 `CLAUDE.md` 过时的 "init/verify/publish/promote/rollback 都是 todo!() stub" 描述(实际只剩 `init`)。
- 依据:docs/12-cli.md(CLI UX 硬性要求)、docs/06-cicd.md(自动注入 changelog)。
