# Toolchain

## 概览

SwarmHive 是 **polyglot monorepo**：Cargo workspace（4 crate）+ pnpm workspace（apps/admin + 未来 packages/*）双轨。本文记录 Rust + JS 双栈的工具链约束、命名约定、git hooks、CI、提交规范。

## Rust 工具链

### edition 2024 + rust-toolchain channel = "stable"

```toml
# Cargo.toml [workspace.package]
edition = "2024"
rust-version = "1.94"

# rust-toolchain.toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

**Why**：
- SeaORM 2.0 强制 edition 2024（≥ 1.85）
- 现代 `url` / `reqwest` / `aws-sdk-s3` 间接拉的 `icu_*` 2.2 已要求 1.86
- `channel = "stable"` 让本地 / CI 都用最新 stable rustc，避免被生态再拉超时反复改 pin
- `rust-version = "1.94"` 是当前 stable 落地版本，作为 Cargo MSRV 标记便于下游消费者识别（不锁死 toolchain channel）
- 历史上曾 pin 过 `rust-version = "1.90"` / `channel = "1.90.0"`，后随依赖演进升到 1.94 并把 channel 简化为 stable

**不要做**：不要降到非 stable channel（如 `1.80.0`）——会被 ICU 等依赖立即破坏。详见 `openspec/changes/add-toolchain-bump/proposal.md`（其中 1.90 数字是历史决策记录）。

**相关文件**：`Cargo.toml`、`rust-toolchain.toml`、`CLAUDE.md` Conventions 段。

### Workspace 依赖集中管理

```toml
# 根 Cargo.toml [workspace.dependencies] 集中 pin
sea-orm = { version = "=2.0.0-rc.38", features = [...] }
axum = { version = "0.7", features = ["macros"] }
```

各 crate 的 `Cargo.toml` 用 `.workspace = true` 引用：

```toml
[dependencies]
sea-orm.workspace = true
axum.workspace = true
```

**正确做法**：
- 新增共享依赖必须先在根 `[workspace.dependencies]` pin 版本
- 不要在 per-crate `Cargo.toml` 直接 `version = "..."` 引第三方依赖（除非真的只有这个 crate 用）

**相关文件**：`Cargo.toml`、各 crate `Cargo.toml`。

### 依赖坑位（实测记录）

- **sqlx 不需要直接依赖**：sea-orm 自带 sqlx（sqlx 是 sea-orm 的异步驱动 + 连接池 + 类型系统）。sea-orm 的 `sqlx-postgres` / `runtime-tokio-rustls` / `with-uuid` / `with-chrono` / `with-json` feature 会**传递启用** sqlx 对应的 postgres/rustls/uuid/chrono/json，代码里零 `sqlx::` 直接调用（仅 `ConnectOptions::sqlx_logging`，那是 sea-orm API）。曾有过 server 直接依赖 sqlx + 根 workspace pin，已移除——`cargo check` 编译图零变化、Cargo.lock 仍是同一份 sqlx 0.8.6。需要自己 pin sqlx 版本时再加回。
- **sha2 / md-5 必须同 major**：Content-MD5 闸门用 `md-5` 算 MD5，复用与 `sha2` 同一个 `digest` crate 的 `Digest` trait（`Md5::new()` 借用作用域里 `use sha2::Digest` 引入的 trait）。两者**只升其一会让 `Digest` trait 版本分叉、编译失败**，必须成对升级。
- **`digest` 0.11 移除了 hasher 的 `std::io::Write` impl**：升 sha2/md-5 到 0.11 后 `std::io::copy(&mut file, &mut hasher)` 报 `Sha256: std::io::Write not satisfied`。改手动 `file.read(&mut buf)` 分块喂 `Digest::update`（见 `swarmhive-cli/src/commands/client.rs::hash_file` 泛型 helper，对 0.10/0.11 都成立）。
- **sha2 0.10 与 0.11 在树里并存**：我们直连升到 0.11，但 `aws-sigv4`（SigV4 签名，经 aws-sdk-s3 / aws-config）+ `rust-embed-utils` 仍拉 sha2 0.10。升我们的直连 sha2 **不能统一整棵树**，只是多出一份 0.11；要真正减重得等上游一起升。升级前用 `cargo tree -i <crate>` 确认 blast radius。

### Release profile（慢但产物小）

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true
```

**Why**：`lto = "thin"` + `codegen-units = 1` 显著拖慢 release build，但产物小。CI 上的 `--release` build 预期分钟级。

**相关文件**：`Cargo.toml` 末尾。

### `[lints.rust]` table 每个 crate 加 `unsafe_code = "forbid"`

新建 crate 时在 `Cargo.toml` 加：

```toml
[lints.rust]
unsafe_code = "forbid"
```

**IDE 可能报 schema 误警**（schema 比 cargo 1.74+ 落后），cargo 自身能正确解析。忽略 IDE 警告。

**相关文件**：每个 crate `Cargo.toml` 末尾。

## pnpm 工具链

### pnpm workspace + admin filter

```yaml
# pnpm-workspace.yaml
packages:
  - "apps/*"
  - "packages/*"
```

常用命令：

```bash
pnpm install                                       # 全 workspace 装依赖
pnpm admin:dev                                     # 由根 package.json 的 scripts 转发
pnpm --filter @swarm-hive/admin typecheck           # 单包命令
```

**相关文件**：`pnpm-workspace.yaml`、`package.json`。

### Node 22 + pnpm 10（CI）

CI 用 `pnpm/action-setup@v4` + `actions/setup-node@v4` cache=pnpm。

**相关文件**：`.github/workflows/ci.yml`。

## Lint / Format / Test 命令

### Rust

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

**注意 edition 2024 fmt 行为**：import 默认按 alphabetical 排序，旧 edition 2021 习惯的"source order"会被改。第一次 `cargo fmt --all` 可能产生大 diff，是预期。

**相关文件**：`.github/workflows/ci.yml` rust job、`lefthook.yml` pre-commit。

### JS / TS（Biome 统管全仓）

```bash
pnpm lint                # biome check .
pnpm format              # biome check --write .
pnpm lint:ci             # biome ci .
```

Biome 配 `biome.json` ignore：`target/`、`dist/`、`node_modules/`、`routeTree.gen.ts`、`pnpm-lock.yaml`、`CHANGELOG.md`。

**正确做法**：
- 不要用 ESLint / Prettier，全仓 Biome
- 不要在 Biome ignore 列表外编辑生成文件（如 `routeTree.gen.ts`）

**相关文件**：`biome.json`、`package.json` scripts。

## Lefthook（git hooks）

`lefthook.yml` 两阶段：

- **pre-commit**（parallel）：staged JS/TS/JSON 跑 biome check；staged Rust 跑 cargo fmt --check
- **commit-msg**：commitlint 校验 Conventional Commits

**正确做法**：
- 如果 `cargo fmt --check` 在 pre-commit 失败：跑 `cargo fmt --all` 重新 stage，**不要** `--no-verify` 绕过
- 如果 hook 本身坏了：先修 hook，不是绕过

**相关文件**：`lefthook.yml`。

## Conventional Commits

提交 subject 必须匹配 `type(scope): subject`。git-cliff（`cliff.toml`）按这个规范生成 `CHANGELOG.md`。

**正确做法**：
- 用 `feat:` / `fix:` / `docs:` / `chore:` / `refactor:` / `test:` / `ci:` / `build:` / `perf:`
- scope 用 crate 名简写：`server`、`cli`、`entity`、`api-types`、`admin`、`docs`、`openspec`
- 示例：`feat(server): add presign upload endpoint`、`docs(architecture): postgres-only decision`

**相关文件**：`commitlint.config.js`、`cliff.toml`。

## Changelog

```bash
pnpm changelog            # 完整重写 CHANGELOG.md
pnpm changelog:latest     # 仅 unreleased 段
```

**相关文件**：`cliff.toml`、`CHANGELOG.md`、`package.json` scripts。

## Brand 三态命名

`SwarmHive`（显示）/ `swarmhive`（路径、npm scope、Rust crate kebab）/ `SWARMHIVE`（env vars，如 `SWARMHIVE_TOKEN`、`SWARMHIVE_DATABASE__URL`）。

Rust crate 导入用下划线：`swarmhive_server`、`swarmhive_api_types`、`swarmhive_entity`。

**相关文件**：`CLAUDE.md` Conventions 段、`memory/project-design-principles.md` 第 10 条。

## 环境变量约定（figment `__` 嵌套）

```bash
SWARMHIVE_DATABASE__URL=postgres://...
SWARMHIVE_DATABASE__AUTO_SYNC=true
SWARMHIVE_SERVER__BIND=0.0.0.0:3030
```

双下划线 `__` 是 nested override 分隔符（figment 约定）。

**相关文件**：`crates/swarmhive-server/src/config/mod.rs`（待 `add-persistence-foundation` 填充）。

## CI

`.github/workflows/ci.yml` 两个 job 并行：

- **rust** matrix（ubuntu + macos）：fmt --check / clippy / build / test
- **node**：pnpm install / biome lint:ci / admin typecheck / admin build

`actions-rust-lang/setup-rust-toolchain@v1` 自动读 `rust-toolchain.toml` 锁版本，不需要在 yaml 里再指定。

**相关文件**：`.github/workflows/ci.yml`。

## Release 分发（cargo-dist / `dist`）

CLI（`swarmhive` 二进制）通过 [cargo-dist](https://axodotdev.github.io/cargo-dist/)（现更名 `dist`）打包分发，配置在根目录 `dist-workspace.toml`，生成 `.github/workflows/release.yml`。**server 不走这条链路**（容器 / 源码构建）。

### 版本与生成方式

```toml
# dist-workspace.toml [dist]
cargo-dist-version = "0.32.0"   # 与本地 dist 二进制版本保持一致
```

改完 `dist-workspace.toml` 后用 `dist generate` 重生成 `release.yml`，**不要手改 release.yml 的 build matrix**。`dist generate` 只读配置、只写 CI 文件，**不动 `dist-workspace.toml`**。

**坑：`dist init`（不是 `generate`）会重写 `dist-workspace.toml` 本身** —— `dist init` 会把 `[dist]` 表里的内联注释换成 dist 默认注释、把 `targets` 压成单行、并把交互/默认选项显式写回（如 `install-updater`），同时往根 `Cargo.toml` 追加 `[profile.dist]`。文件**头部注释（`[workspace]` 之前）会保留**，但 `[dist]` 表内的自定义注释会被清掉。所以：日常重生成用 `dist generate`；只有要新增 installer / 改 target 这类结构性改动才跑 `dist init`，且跑完检查它有没有顺手改掉你不想改的键（如 `install-updater`）。重要约定写文件头或本知识库，别指望 `[dist]` 表内联注释存活。

### 只发 CLI：必须给 server 显式 `dist = false`

`dist-workspace.toml` 的 `[workspace] members = ["cargo:crates/swarmhive-cli"]` **不足以**把分发限定到 CLI —— dist 仍会自动发现整个 cargo workspace 里所有带二进制的 crate。`swarmhive-server` 有 `swarmhive-server` + `dump-openapi` 两个 `[[bin]]`，会被一起打进 release（`dist plan` 里出现两个 release）。

**正确做法**：在 server crate `Cargo.toml` 加

```toml
[package.metadata.dist]
dist = false
```

验证：`dist plan --output-format=json` 的 `releases[]` 应只剩 `swarmhive-cli`。

### Homebrew installer 需要独立 tap 仓库 + secret

```toml
installers = ["shell", "powershell", "npm", "homebrew"]
tap = "swarm-apps/homebrew-tap"
publish-jobs = ["npm", "homebrew"]
```

**外部前置条件（否则 release 的 publish-homebrew-formula job 必失败）**：
1. 在 GitHub 建仓库 `swarm-apps/homebrew-tap`（dist 自动维护其内部结构）
2. 在**主仓库** `swarm-apps/SwarmHive` 加 secret `HOMEBREW_TAP_TOKEN`（带 `repo` scope 的 PAT）

用户安装路径：`brew install swarm-apps/tap/swarmhive`。

**homepage warning**：`dist generate` 对启用了 homebrew 的 crate 会校验 `homepage`。CLI crate 通过 `homepage.workspace = true` 继承根 `[workspace.package] homepage` 即可消除 warning（继承能被 dist 正确解析）；warning 若仍存在，多半是某个**未排除的 crate**（如忘了给 server 加 `dist = false`）缺 homepage。

**相关文件**：`dist-workspace.toml`、`.github/workflows/release.yml`、`crates/swarmhive-cli/Cargo.toml`、`crates/swarmhive-server/Cargo.toml`、根 `Cargo.toml`（`[workspace.package] homepage` + `[profile.dist]`）。

## 已知 Windows quirk

- 重命名 Rust 源目录可能留下空父目录被 rust-analyzer / VSCode file watcher 锁住——非致命，只要内部没 `Cargo.toml`（workspace 会忽略）。关 watcher 清掉
- `core.autocrlf` 开着，预期 `LF will be replaced by CRLF` 警告。**不要**试图"修"它
