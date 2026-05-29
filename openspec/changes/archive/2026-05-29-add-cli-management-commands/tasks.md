# tasks — add-cli-management-commands

CLI-only,消费 `add-app-release-artifact` 既有端点。零后端 / api-types 改动。`[code]`/`[test]`/`[docs]`。

## 1. client.rs:HTTP helper + 错误结构化

- [x] 1.1 [code] 提炼 `ApiError`(`thiserror`:status / type / title / detail / extra)——非 2xx 时解析 RFC 9457 problem+json(复用 login.rs 现有手解 `detail` 逻辑)。
- [x] 1.2 [code] 加 `patch_json<B,T>` / `delete_no_content`(沿用 `build_client` + bearer 注入 + `ApiError` 解析);现有 `get_json` / `post_json` 改用统一错误解析。
- [x] 1.3 [test] 单测:problem+json → `ApiError` 解析(有 detail / 缺 detail / 非 JSON body 回落)。

## 2. main.rs:命令树

- [x] 2.1 [code] 扩 `AppsCommand`:`Get` / `Create` / `Update` / `Delete{--yes}`;扩 `ReleasesCommand`:`Get` / `Create` / `Update` / `Publish` / `Yank{--yes}`。
- [x] 2.2 [code] 新 `ChannelsCommand`:`List` / `Create` / `SetDefault` / `Promote` / `Rollback`;`Command` 加 `Channels { command }`。
- [x] 2.3 [code] 移除 top-level `Command::Promote` / `Command::Rollback` 桩 + 其 `todo!()` match 臂。
- [x] 2.4 [code] main 顶层错误渲染:`run()` 返回 `Result<_, ApiError>`,按 `cli.output` —— json → stderr problem+json + `process::exit(非零)`;table → 人话 + 非零。

## 3. commands:写动词实现

- [x] 3.1 [code] `apps.rs`:`get` / `create`(`--slug --display-name --platforms` 逗号解析)/ `update`(PATCH,slug 不可变)/ `delete`(`--yes` 守门 → DELETE)。成功走 `emit`(json/table)。
- [x] 3.2 [code] 新 `commands/channels.rs`:`list` / `create` / `set_default`(PATCH `is_default:true`)/ `promote`(`--version`)/ `rollback`(`--to-version` 可选)。`mod.rs` 注册。
- [x] 3.3 [code] `releases.rs`:`get` / `create`(`--version [--android-version-code] [--notes-file]`,建 draft)/ `update`(PATCH)/ `publish`(POST `/publish`)/ `yank`(`--yes` → POST `/yank`)。
- [x] 3.4 [code] 破坏性 `--yes` 守门:缺则返 `ApiError`-风格错误 + 非零 exit,不打服务器。

## 4. 校验 + docs

- [x] 4.1 [test] gates:`cargo fmt --all` / `cargo clippy --workspace --all-targets -D warnings` / `cargo build -p swarmhive-cli`;边界回归 `cargo tree -p swarmhive-cli | grep sea-orm` 必须空。
- [x] 4.2 [test] e2e:**改为**纯逻辑单测(`build_problem`/`ApiProblem::message`/`parse_platforms`,bin crate `#[cfg(test)]`)+ live-server 手动验证(`channels list` 新组、`apps get`、`apps get --output json` 404 → problem+json 到 stderr + 非零 exit、`--yes` 守门)。**CLI-binary testcontainers e2e deferred**:bin crate 无法被集成测试 import,CLI 走 reqwest 需真实 server 进程(无现成 harness,与 admin e2e 同样 deferred);endpoint 行为已由 `app_release_smoke`(in-process)覆盖。
- [x] 4.3 [docs] `docs/12-cli.md`:命令清单补全 + `releases publish` vs `publish {tauri|android}` 对照 + `--output json` / problem+json / 非零 exit 契约段 + 给 AI 的最小权限 token 建议。
- [x] 4.4 [docs] `dev-notes/knowledge/backend.md` CLI 段:管理命令 + 错误结构化 + json/stderr/exit 契约;`memory/project-cli-surface.md` 命令清单更新(若仍准确)。
- [x] 4.5 [docs] `openspec/changes/README.md` 进度表加 `add-cli-management-commands`。
