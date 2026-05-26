# tasks

## 1. workspace 拓扑

- [x] [code] `Cargo.toml` `[workspace.members]` 改成 4 crate：`crates/swarmhive-api-types`、`crates/swarmhive-entity`、`crates/swarmhive-server`、`crates/swarmhive-cli`
- [x] [code] 删除 `crates/swarmhive-core/` 目录
- [x] [code] `[workspace.dependencies]` 删除 `swarmhive-core = { path = ... }`，新增 `swarmhive-api-types = { path = "crates/swarmhive-api-types" }` 与 `swarmhive-entity = { path = "crates/swarmhive-entity" }`

## 2. swarmhive-api-types crate

- [x] [code] 新建 `crates/swarmhive-api-types/Cargo.toml`（仅 serde、serde_json、chrono、uuid、utoipa、garde 可用；不引 sea-orm / axum / tokio / reqwest）
- [x] [code] 新建 `crates/swarmhive-api-types/src/lib.rs`（含 `#![forbid(unsafe_code)]` + 顶部 doc 注释说明边界）+ 迁移原 core 的 `Platform` / `Channel` enum
- [x] [code] workspace 顶部 `[workspace.dependencies]` 添加 `utoipa = { version = "5", features = ["chrono", "uuid"] }`、`garde = { version = "0.22", features = ["full"] }`（首次 pin）

## 3. swarmhive-entity crate

- [x] [code] 新建 `crates/swarmhive-entity/Cargo.toml`（依赖 swarmhive-api-types + serde + chrono + uuid；sea-orm 由 `add-persistence-foundation` 引入）
- [x] [code] 新建 `crates/swarmhive-entity/src/lib.rs`（空 lib + 顶部 doc 注释 + `REGISTRY_GLOB` 常量；entity 模块由 `add-persistence-foundation` 填充）
- [x] [code] **不**引入 sea-orm-migration（用户决定阶段 0~1 沿用 schema-sync）

## 4. swarmhive-server lib + bin

- [x] [code] `crates/swarmhive-server/src/main.rs` 移到 `crates/swarmhive-server/src/bin/server.rs`
- [x] [code] 新建 `crates/swarmhive-server/src/lib.rs`（pub mod 占位：config、state、routes、auth、services、storage、mail、error）+ `build_router(state)` 工厂
- [x] [code] 创建空模块文件：`src/config/mod.rs`、`src/state.rs`、`src/routes/mod.rs`（含 health 路由迁移）、`src/auth/mod.rs`、`src/services/mod.rs`、`src/storage/mod.rs`、`src/mail/mod.rs`、`src/error.rs`
- [x] [code] 把 health route 从 `src/bin/server.rs` 抽到 `src/routes/health.rs`；version route 抽到 `src/routes/version.rs`，由 lib::build_router 装配
- [x] [code] `Cargo.toml` 显式声明 `[lib]` 与 `[[bin]]` target；依赖加 `swarmhive-api-types` + `swarmhive-entity`
- [x] [code] 删除 `swarmhive-core` 依赖

## 5. swarmhive-cli

- [x] [code] `Cargo.toml` 删除 `swarmhive-core` 依赖；新增 `swarmhive-api-types`
- [x] [code] `src/main.rs`：去掉 `swarmhive_core::VERSION` 引用（version 命令只打印 CLI 自己的版本）

## 2.b 删除 swarmhive-core

- [x] [code] 删除 `crates/swarmhive-core/` 目录（在 4 crate 都 build 通过后执行）

## 6. 验证

- [x] [code] `cargo build --workspace` 通过（11s）
- [x] [code] `cargo clippy --workspace --all-targets -- -D warnings` 通过（零警告）
- [x] [code] `cargo fmt --all -- --check` 通过（执行 fmt 后 import 按字母序排好）
- [x] [code] `cargo test --workspace` 通过（0 tests, wiring 包含 lib + bin + doc-tests 都跑过）
- [x] [test] `cargo metadata` 确认列出 4 个 swarmhive-* crate（api-types / entity / server / cli），不含 core
- [x] [test] `cargo tree -p swarmhive-cli | grep -i sea-orm` 无输出 ✓
- [x] [test] `cargo tree -p swarmhive-api-types` 无 axum / tokio / sea-orm / reqwest ✓

## 7. docs / memory 同步

- [x] [docs] [docs/03-architecture.md](../../../docs/03-architecture.md) "仓库组织" 段：crate 树更新（删 core，加 api-types / entity，server 标 lib+bin） + 新增 "Rust crate 边界（硬约束）" 条目
- [x] [docs] [memory/project-backend-tech-stack.md](../../../memory/project-backend-tech-stack.md) crate 拓扑 + "core crate 取消"说明 + sea-orm-migration crate 决定不引入
- [x] [docs] [openspec/changes/README.md](../README.md) 依赖图：插入 add-crate-restructure 节点（toolchain → restructure → persistence）
- [x] [docs] [openspec/config.yaml](../../config.yaml) context：4 crate 拓扑说明
- [x] [docs] [CLAUDE.md](../../../CLAUDE.md) Repository layout 段：crate 列表更新 + log filter 去掉 swarmhive_core

## 8. 跟随其他 proposal 联动

- [x] [docs] [add-persistence-foundation/proposal.md](../add-persistence-foundation/proposal.md) 已修订：entity 落 swarmhive-entity；不引入 migration；Depends-on 加 add-crate-restructure
- [x] [docs] [add-persistence-foundation/design.md](../add-persistence-foundation/design.md) 已修订：crate 边界图 + entity 写法用 swarmhive-entity 路径 + schema-sync 唯一策略
- [x] [docs] [add-persistence-foundation/tasks.md](../add-persistence-foundation/tasks.md) 已修订：路径相应更新 + 新增 api-types 系列任务
- [x] [docs] 其他后续 proposal 中扫描 `swarmhive-core` 残留引用（add-auth-and-rbac / add-mail-infrastructure / add-app-release-artifact / add-oauth-github 各处路径已批量更新到 swarmhive-server 或 swarmhive-entity）
