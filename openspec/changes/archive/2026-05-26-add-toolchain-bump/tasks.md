# tasks

- [x] [code] 修改 `Cargo.toml` `[workspace.package]`: `edition = "2024"`, `rust-version = "1.90"`
- [x] [code] 新建 `rust-toolchain.toml`（channel = `1.90.0`, components = rustfmt + clippy）
- [x] [code] `cargo build --workspace` 并修编译错误
- [x] [code] `cargo clippy --workspace --all-targets -- -D warnings` 修 edition 2024 新增 lint
- [x] [code] `cargo fmt --all`（edition 2024 默认 import 排序，自动 fix 通过）
- [x] [code] 新建 `.github/workflows/ci.yml`（rust fmt/clippy/build/test 矩阵 + node biome/admin typecheck/build）
- [x] [docs] 在 [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 0 任务列表里追加 "Rust 2024 / MSRV 1.90"
- [x] [docs] [CLAUDE.md](../../../CLAUDE.md) 工具链段补一句
- [x] [test] `cargo test --workspace`（即便测试为空也跑一次确保 wiring 没断）
