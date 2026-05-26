# design

## Crate 拓扑

```text
                       ┌─────────────────────────────┐
                       │  swarmhive-api-types        │
                       │  serde + utoipa::ToSchema   │
                       │  (zero ORM / HTTP / IO)     │
                       └────────────┬────────────────┘
                                    │
                ┌───────────────────┴───────────────────┐
                │                                       │
                ▼                                       ▼
   ┌──────────────────────────┐         ┌──────────────────────────┐
   │  swarmhive-entity         │        │  swarmhive-cli            │
   │  sea-orm 2.0              │        │  clap + reqwest +         │
   │  Entity / ActiveModel /   │        │  indicatif + keyring      │
   │  ActiveEnum + From<->     │        │  (no ORM, no DB driver)   │
   │  api-types conversions    │        │                           │
   └────────────┬─────────────┘         └──────────────────────────┘
                │
                ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  swarmhive-server  (lib + bin)                              │
   │                                                             │
   │  lib (swarmhive_server::*)                                   │
   │   ├─ config / state                                          │
   │   ├─ auth      (argon2, session, Principal, OAuth)           │
   │   ├─ services  (App/Release/Artifact 业务规则)               │
   │   ├─ storage   (S3 trait + aws-sdk-s3 impl, presign)         │
   │   ├─ mail      (lettre + minijinja, DB-backed templates)     │
   │   ├─ routes    (axum handlers, utoipa::path)                 │
   │   └─ error     (ApiError, RFC 9457)                          │
   │                                                             │
   │  bin (swarmhive-server)                                      │
   │   └─ tokio main + tracing-subscriber + Axum                  │
   └─────────────────────────────────────────────────────────────┘
```

## 反向依赖（不允许）

- ❌ `api-types` 依赖 entity / server / cli。
- ❌ `entity` 依赖 server / cli。
- ❌ `cli` 依赖 entity / server。
- ✅ `entity` 依赖 `api-types` （只为 From/Into 转换）。
- ✅ `server` 依赖 `api-types` + `entity`。
- ✅ `cli` 依赖 `api-types`。

每个 crate 加 `forbid(unsafe_code)` 与 `deny(missing_docs)`（暂留 warn，进入实现期再 deny）。

## api-types ↔ entity 转换的放置

转换实现写在 **entity crate** 里（`impl From<entity::user::Model> for api_types::User`），不在 server。

理由：

- 转换函数跟 Entity 字段一一对应，字段增减跟着 entity 改更顺手。
- server 业务层关注的是"用 service 返回 ApiUser"，不需要看转换细节。
- api-types 不能反过来 import entity（会形成环），所以转换只能在 entity 这边。

每个 entity model 出口提供两种结构：

```rust
// entity::user
pub use Model as Entity;          // 内部数据库视图
pub fn to_api(model: &Model) -> api_types::User { ... }
impl From<&Model> for api_types::User { ... }   // optional, looks nicer
```

## server lib + bin 切分

`Cargo.toml` 显式声明 lib 和 bin，避免 cargo 默认推断只看 `main.rs`：

```toml
[lib]
name = "swarmhive_server"
path = "src/lib.rs"

[[bin]]
name = "swarmhive-server"
path = "src/bin/server.rs"
```

`src/lib.rs` 只做：

```rust
pub mod auth;
pub mod config;
pub mod error;
pub mod mail;
pub mod routes;
pub mod services;
pub mod state;
pub mod storage;

pub fn build_router(state: state::AppState) -> axum::Router { ... }
```

`src/bin/server.rs`：

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cfg = swarmhive_server::config::load()?;
    let state = swarmhive_server::state::AppState::new(cfg).await?;
    let app = swarmhive_server::build_router(state);
    // ... bind + serve
    Ok(())
}
```

集成测试在 `tests/` 直接 `use swarmhive_server::build_router;`。

## 当前文件迁移映射

```text
crates/swarmhive-core/src/lib.rs                        →  删除（占位空 lib）
crates/swarmhive-core/Cargo.toml                        →  删除

crates/swarmhive-server/src/main.rs                     →  crates/swarmhive-server/src/bin/server.rs（保持现内容）
crates/swarmhive-server/src/lib.rs                      →  新建（pub mod 声明）

crates/swarmhive-api-types/src/lib.rs                   →  新建（空 lib + #![doc] 注释）
crates/swarmhive-api-types/Cargo.toml                   →  新建

crates/swarmhive-entity/src/lib.rs                      →  新建（空 lib）
crates/swarmhive-entity/Cargo.toml                      →  新建

crates/swarmhive-cli/src/main.rs                        →  保持
crates/swarmhive-cli/Cargo.toml                         →  去掉 swarmhive-core 依赖
```

## 风险

- **历史 commit 引用 `swarmhive-core`**：仓库刚起步，core 还没承载业务，删它无历史包袱。
- **未来若要拆 worker / cron binary**：本结构已支持——worker 直接依赖 `swarmhive-entity` + `swarmhive-api-types`，跟 server 复用模型。
- **api-types 的版本同步**：server 和 CLI 同 workspace，pin 同一 path 引用，不会版本漂移。

## Open questions

- entity crate 内部如何组织子目录？倾向**按资源分模块**（`user/mod.rs`、`app/mod.rs`、…）而非"全堆在 src/"。`add-persistence-foundation` 实施时具体定。
- api-types 要不要按 endpoint 分 mod？倾向**按资源分**（`mod user; mod app; mod release;`），与 entity 镜像。
