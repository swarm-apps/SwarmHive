# add-toolchain-bump

## Why

SeaORM 2.0 强制要求 Rust 2024 edition + MSRV ≥ 1.85（见 sea-orm-2 skill 速查）。实际试编时 `url` / `reqwest` / `aws-sdk-s3` 间接依赖的 `icu_*` 2.2.0 已要求 1.86，且后续依赖更新还会持续抬高 floor。综合权衡，把 MSRV 设为 **1.90**（在 SeaORM 下限之上留 5 版 headroom），避免短期内被生态再次拉超。后续所有持久层 / 鉴权 / 业务实体的 proposal 都依赖 SeaORM 2.0 的 Entity Loader、Nested ActiveModel、`raw_sql!` 宏与 RBAC 能力。当前 workspace 仍然是：

```toml
edition      = "2021"
rust-version = "1.80"
```

→ 不升级则整条 MVP 推进路径无法启动。

## What

把 workspace 升级到 Rust 2024 / 1.90，并锁定工具链：

- `Cargo.toml` `[workspace.package]` 改 `edition = "2024"`、`rust-version = "1.90"`。
- `rust-toolchain.toml` 锁 `channel = "1.90.0"`、`components = ["rustfmt", "clippy"]`、`profile = "minimal"`。
- 修复升级到 edition 2024 后产生的 lint / 编译警告（主要是 `unsafe_op_in_unsafe_fn`、`async fn 生命周期`、`temp 借用` 等）。
- `cargo fmt` / `cargo clippy --workspace --all-targets` 全过。
- pre-commit hook（`lefthook.yml`）确认仍能识别新 edition 的源码。
- CI workflow 升级 Rust 安装版本。

## Acceptance

- `cargo build --workspace`、`cargo test --workspace` 在 1.90 上通过。
- `cargo +stable` 兜不住的语法零容忍。
- CI 使用 `actions-rust-lang/setup-rust-toolchain@v1` 或等价方案锁 1.90。
- `git grep -nE 'edition = "2021"|rust-version = "1\\.80"'` 在 `Cargo.toml` 中无匹配。

## Non-goals

- 不在本 proposal 引入任何业务依赖（sea-orm / argon2 等留给后续 proposal）。
- 不修改业务代码逻辑，只做编译期兼容修复。
- 不动 `apps/admin` 的 Node / pnpm 工具链版本。

## Depends on

- 无前置。

## Maps to docs

- 隐式前置：[docs/03-architecture.md](../../../docs/03-architecture.md)（数据库与 ORM 选型）、[docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 0。
