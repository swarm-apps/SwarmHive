# add-crate-restructure

## Why

当前 workspace 用 `swarmhive-core / swarmhive-server / swarmhive-cli` 三 crate，但 server 和 CLI 的"业务"几乎完全不重叠——server 做 axum handler / DB / storage / auth / mailer，CLI 做 clap / reqwest / 进度条 / keyring。`swarmhive-core` 如果承载 server 业务，CLI 就被迫拖入 sea-orm / argon2 等编译期重依赖；如果只放纯 DTO，又承担不了 server 端业务的容器角色。

唯一**真正两端共享**的是 HTTP API DTO（request / response struct）。其他工具级共享（semver、sha256）用现成 crate 就够。

→ 拆掉 `swarmhive-core`（不是 server 业务的家），改为 4 crate：

```text
swarmhive-api-types  ← serde DTO + utoipa::ToSchema（薄共享层）
swarmhive-entity     ← sea-orm Entity / ActiveModel（仅 server 系）
swarmhive-cli        ← clap + reqwest + indicatif + keyring
swarmhive-server     ← lib + bin（业务/storage/auth/mail/handler/SPA embed）
```

## What

### 1. 删除 `swarmhive-core` crate

把它从 `[workspace.members]` 移除，物理目录 `crates/swarmhive-core/` 删掉。CLI 的 `Cargo.toml` 中 `swarmhive-core.workspace = true` 一并去掉。

### 2. 新建 `swarmhive-api-types`

- 路径：`crates/swarmhive-api-types/`
- 角色：纯 serde `Deserialize`/`Serialize` struct + `utoipa::ToSchema` 注解。
- 禁止依赖：sea-orm / axum / tokio / reqwest / clap。
- 允许依赖：serde、serde_json、chrono、uuid、utoipa、garde（DTO 校验注解）。
- 初始内容为空（仅 lib.rs + 一个 placeholder mod），具体 DTO 跟着后续 proposal 一起加（`add-auth-and-rbac` 加 `LoginReq/Resp`、`add-app-release-artifact` 加 release 系列等）。

### 3. 新建 `swarmhive-entity`

- 路径：`crates/swarmhive-entity/`
- 角色：sea-orm 2.0 Entity / ActiveModel / ActiveEnum，与 DB schema 一一对应；提供 `From<entity::user::Model> for api_types::User` 等转换 impl。
- 允许依赖：sea-orm、chrono、uuid、serde、`swarmhive-api-types`。
- 不依赖：axum / tokio / business logic / IO。
- 本 proposal 不放任何 entity（等 `add-persistence-foundation`），只建空 crate。

### 4. 把 `swarmhive-server` 改成 lib + bin 两 target

- `src/lib.rs`（新，pub use 暴露业务模块）。
- `src/bin/server.rs`（从原 `src/main.rs` 改名）。
- `Cargo.toml` 显式声明：

  ```toml
  [lib]
  name = "swarmhive_server"
  path = "src/lib.rs"

  [[bin]]
  name = "swarmhive-server"
  path = "src/bin/server.rs"
  ```

- 业务模块在 `src/` 下按职责组织（占位空模块，让结构先立起来）：

  ```text
  src/lib.rs
  src/config/        ← figment 加载
  src/state.rs       ← AppState
  src/routes/        ← axum handlers（按资源拆 mod）
  src/auth/          ← argon2 / session / Principal extractor
  src/services/      ← business services
  src/storage/       ← S3 trait + impl + presign
  src/mail/          ← lettre + minijinja
  src/error.rs       ← ApiError + RFC 9457 IntoResponse
  src/bin/server.rs  ← main: tokio + tracing-subscriber + Axum
  ```

- 加 `swarmhive-api-types` + `swarmhive-entity` 为依赖。

### 5. CLI 清理

- `swarmhive-cli/Cargo.toml` 去掉 `swarmhive-core.workspace = true`，改为 `swarmhive-api-types.workspace = true`。
- 已有 `src/main.rs` 保持入口；后续 CLI 子命令独立 mod 添加（不在本 proposal 范围）。

### 6. Workspace 共享依赖重新指派

`[workspace.dependencies]` 的内容按所属 crate 分组：

- **api-types 用**：serde、serde_json、chrono、uuid、utoipa、garde。
- **entity 用**：sea-orm、chrono、uuid、serde。
- **server 用**：axum、tower、tower-http、tokio、tokio-util、hyper、aws-sdk-s3、aws-config、figment、tracing、tracing-subscriber、thiserror、anyhow、rust-embed、mime_guess、async-trait、dashmap。
- **cli 用**：clap、reqwest、indicatif、toml、anyhow、thiserror、tracing、tracing-subscriber。

shared 三方依赖（serde / chrono / uuid / tracing）继续在 workspace 顶部 pin 版本，每 crate `workspace = true` 引用。

### 7. memory + docs 同步

- 修订 [memory/project-backend-tech-stack.md](../../../memory/project-backend-tech-stack.md)：crate 列表 + "core crate 不再存在"。
- 修订 [docs/03-architecture.md](../../../docs/03-architecture.md) "仓库组织" 段：crate 树更新为 4 crate（删除 `crates/swarmhive-core/`，新增 `crates/swarmhive-api-types/` 与 `crates/swarmhive-entity/`，server 标注 lib+bin）。
- [openspec/changes/README.md](../README.md) 依赖图：移除 core 节点。
- [openspec/config.yaml](../../config.yaml) context：crate 拆分说明。

## Acceptance

- `cargo build --workspace` 通过（4 crate，编译产物里 `swarmhive-core` 消失，新增 `swarmhive_api_types` + `swarmhive_entity`）。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo fmt --all -- --check` 通过。
- `cargo metadata --format-version=1 | jq '.workspace_members'` 列出 4 个 crate（不含 core）。
- CLI crate 的依赖树（`cargo tree -p swarmhive-cli`）**不包含** sea-orm。
- api-types crate 的依赖树**不包含** axum / tokio / sea-orm。
- entity crate 的依赖树**不包含** axum / tokio（除非 sea-orm 自身的 runtime feature 拉的）。
- server crate 仍能 build + 暴露 `/healthz`。

## Non-goals

- 不引入 sea-orm-migration crate（沿用 sea-orm `schema-sync`，待生产升级再切）。
- 不加任何业务 entity / DTO / handler（拆到后续 proposal）。
- 不动 `apps/admin` / `packages/*` 的 npm workspace 结构。
- 不动 `xtask` / `examples`（目前都不存在）。

## Depends on

- `add-toolchain-bump`（edition 2024 是 SeaORM 2.0 引入的硬性前提）

## Maps to docs

- [docs/03-architecture.md](../../../docs/03-architecture.md) "仓库组织" 段（本 proposal 完成后会同步修订）。
- [memory/project-backend-tech-stack.md](../../../memory/project-backend-tech-stack.md)。
