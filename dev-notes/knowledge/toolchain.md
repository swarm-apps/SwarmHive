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
pnpm --filter @swarmhive/admin typecheck           # 单包命令
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

## 已知 Windows quirk

- 重命名 Rust 源目录可能留下空父目录被 rust-analyzer / VSCode file watcher 锁住——非致命，只要内部没 `Cargo.toml`（workspace 会忽略）。关 watcher 清掉
- `core.autocrlf` 开着，预期 `LF will be replaced by CRLF` 警告。**不要**试图"修"它
