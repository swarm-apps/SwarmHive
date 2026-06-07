## 1. 前置 / 依赖

- [x] 1.1 [code] 在 workspace 根 `Cargo.toml` 的 `[workspace.dependencies]` pin `dialoguer`,并在 `crates/swarmhive-cli/Cargo.toml` 加 `dialoguer.workspace = true`(确认与 `indicatif` 共享 `console`,`cargo tree` 无重复 console major)。
- [x] 1.2 [code] 核对 `api-types` 已有 `UpdateReleaseRequest.release_notes` 且 server `PATCH /api/v1/apps/:slug/releases/:version` 路由可用(`releases update` 已用),作为 notes 注入前置;若缺则在 design 记风险并最小补(不扩面)。

## 2. `swarmhive init` 双模式(capability: cli-project-init)

- [x] 2.1 [code] 定义 init 全字段 flag:`--server`/`--app`/`--platform <tauri|android>`(可重复)/`--tauri-conf`/`--android-apk`/`--force`/`--yes`,并透传全局 `--output`;flag 永远覆盖 prompt/默认。
- [x] 2.2 [code] 模式选择 + 取值:TTY 且无 `--yes` → 交互(`dialoguer` `Input`/`MultiSelect` 只问缺失字段,平台按 `src-tauri/`·`android/`·`*.gradle*` 探测预勾);`--yes` 或非 TTY → 零 prompt、flag + 探测默认,缺必填 `app.slug` 走 typed 错误 + 非零退出(`--output json` 时 problem+json → stderr)。用 `std::io::IsTerminal` 判定。
- [x] 2.3 [code] 手写 `swarmhive.toml` 字符串模板(嵌套 `[app]`/`[app.tauri]`/`[app.android]`;`artifacts` 注释示例块,两模式都不 prompt);`--force` 才覆盖既有文件;写后自解析校验(`ProjectConfig::load`)。
- [x] 2.4 [code] `main.rs` 接 `Command::Init` 派发(移除 `todo!`);成功 `--output json` 输出单对象 `{ path, app, server?, platforms[], created }`。
- [x] 2.5 [test] init 测试:① flag-only(非交互)生成可被 `ProjectConfig` 解析;② 非 TTY 无 `--app` → typed 错误 + 非零;③ `--force` 覆盖守卫;④ `--output json` 为单对象;交互路径把模板渲染抽成纯函数测(避开 dialoguer I/O)。

## 3. publish `--notes-file` / `--notes`(changelog 注入)

- [x] 3.1 [code] `CommonArgs` 加 `--notes-file <path>` 与 `--notes <text>`(file 优先);新建 release 时塞进 `CreateReleaseRequest.release_notes`。
- [x] 3.2 [code] 既有 release(ensure 返回 409/already-exists)且给了 notes 时,补一次 `PATCH` release 更新 `release_notes`(复用 releases update 端点)。
- [x] 3.3 [test] notes 注入两路径(新建一步写入 / 既有 PATCH 更新),断言发布后 `release_notes` 非空。

## 4. publish `--dry-run`

- [x] 4.1 [code] `CommonArgs` 加 `--dry-run`;在 `plan_artifacts` 后、`require_creds`/`post_ensure`/presign 前短路返回,打印发布计划(release/产物/sha256/签名/目标 channel),不发任何 HTTP、不读 creds。
- [x] 4.2 [test] dry-run 不产生网络调用(无 presign/upload/complete)、免鉴权;`--dry-run --output json` 输出 `{ dry_run: true, ... }`。

## 5. `--output json` + 进度条 TTY/JSON 守卫

- [x] 5.1 [code] `main.rs` 把全局 `OutputFormat` 透传进 `publish`/`verify` 派发(当前未传)。
- [x] 5.2 [code] publish 成功 JSON 负载(app/version/status/published/channel?/artifacts[]/endpoints);verify 成功 JSON 负载(app?/version?/version_code?/artifacts[]/ok);`table` 维持现状。失败路径不动(已走 `render_error`)。
- [x] 5.3 [code] 进度条守卫:`--output json` 或 `!stderr().is_terminal()` 时用 `ProgressDrawTarget::hidden()` 构造(`upload_put` 签名不动,改在 `publish.rs::progress_bar` 处)。
- [x] 5.4 [test] `--output json` 时 stdout 为单个合法 JSON、无进度条/人类文案混入;管道(非 TTY)运行 stderr 无进度条乱码。

## 6. 交互框架统一(rpassword → dialoguer)

- [x] 6.1 [code] `client.rs::resolve_secret` 的 TTY 提示分支从 `rpassword::prompt_password` 换成 `dialoguer::Password`(保留外层 `std::io::stdin().is_terminal()` 预判 → 无 TTY 仍返回 `None`;`dialoguer::Error` map 进 `anyhow`)。
- [x] 6.2 [code] 从 root `Cargo.toml` 与 `crates/swarmhive-cli/Cargo.toml` 移除 `rpassword`;确认全仓无残留引用。
- [x] 6.3 [test] 回归 `storage create` / `mail providers create` 的密钥录入:`--secret-stdin`/env/明文 flag 三路仍生效、TTY 提示走 dialoguer、非 TTY 返回 `None`(`update` 省略=保留)。

## 7. docs / memory 同步

- [x] 7.1 [docs] 更新 `docs/12-cli.md`:`init`(双模式)、`publish --dry-run`、`publish/verify --output json`、`publish --notes-file/--notes` 段。
- [x] 7.2 [docs] 更新 `docs/06-cicd.md` changelog 注入说明;`.github/actions/publish/action.yml` 可选增 `notes-file` 透传 input(若加,同步 action 文档)。
- [x] 7.3 [docs] 修 `CLAUDE.md` 过时的 "init/verify/publish/promote/rollback 都是 todo!() stub"(实际只剩 init,本 change 后全实现)。
- [x] 7.4 [docs] 更新 `dev-notes/knowledge/`(CLI 章节如有)+ `memory/project-cli-surface.md`:init 双模式落地、notes-file/dry-run/json 补齐、交互统一到 dialoguer(移除 rpassword);纠正旧的 `default_channel`/扁平结构描述。
- [x] 7.5 [docs] `grep -rn "default_channel\|todo!.*init" openspec/ docs/ memory/` 扫残留旧描述并清理。

## 8. 验收门禁

- [x] 8.1 [test] `cargo fmt --all` + `cargo clippy -p swarmhive-cli --all-targets -- -D warnings` + `cargo test -p swarmhive-cli` 全绿;`grep -rn 'todo!\|unimplemented!\|rpassword' crates/swarmhive-cli/src/` 为空。
- [x] 8.2 [test] 手动 smoke:临时目录跑 `swarmhive init`(交互 + `--yes` 两路)生成可解析配置;`publish tauri --dry-run --output json` 输出单 JSON 且零上传;`storage create` 在 TTY 下 dialoguer 录密钥可用。
- [x] 8.3 [code] `cargo tree -p swarmhive-cli | grep -E 'sea-orm|rpassword'` 无输出(crate 边界 + rpassword 移除回归);`openspec validate add-cli-publish-polish --strict` 通过。
