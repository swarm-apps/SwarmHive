# Tasks — add-cli-telemetry

## 1. commands/telemetry.rs

- [x] 1.1 [code] 新文件 `commands/telemetry.rs`:5 个 async 查询函数(overview/summary/adoption/funnel/distribution),`get_json` 拉数据 + `emit`/`emit_one` 输出;query 走 `format!`(`?app=&days=&dim=`)。
- [x] 1.2 [code] 各定义 `*Row`(tabled)+ row mapper(adoption 的 `version=null` 显示「(total)」;summary 的 Option pct/version 友好显示)。
- [x] 1.3 [code] `commands/mod.rs` 注册 `pub mod telemetry;`。

## 2. main.rs 命令

- [x] 2.1 [code] `Command::Telemetry { command: TelemetryCommand }` + `TelemetryCommand` 枚举(5 variant,`--app`/`--days default 30`/`--dim default platform`)。
- [x] 2.2 [code] dispatch 5 分支。

## 3. Gates + Docs

- [x] 3.1 [test] `cargo build/clippy --workspace --all-targets -D warnings/fmt --check`;`telemetry --help` + 各子命令 `--help` smoke;`cargo test -p swarmhive-cli`;`cargo tree` 无 sea-orm/entity。
- [x] 3.2 [docs] `docs/12-cli.md` 加 telemetry 命令段;`openspec/changes/README.md` 状态表加本 change。

## 4. 审查 + 归档

- [x] 4.1 [chore] 对抗式审查(8 项核查)→ 1 finding:百分比 `{p}%` 改 `{p:.1}%` 对齐 server 0.1% 精度;其余 7 项 OK(query 拼串/路径参数名/DTO 反序列化/row 映射/dispatch 顺序/边界无泄漏/clap 无撞名)。
- [ ] 4.2 [chore] commit(feat)+ `openspec archive` + commit(chore)。
