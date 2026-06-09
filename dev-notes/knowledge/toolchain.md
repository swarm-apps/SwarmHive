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

`.github/workflows/ci.yml` 三个 job：

- **rust** matrix（ubuntu + macos）：fmt --check / clippy / build / CLI 依赖边界 guard / test
- **node**：biome lint:ci（全仓）/ admin typecheck+vitest+build / **build sdk →** sdk·registry-rn·registry-web 各自 typecheck+vitest / docs typecheck
- **e2e**（needs rust+node）：admin Playwright + OpenAPI drift gate（postgres service）

`actions-rust-lang/setup-rust-toolchain@v1` 自动读 `rust-toolchain.toml` 锁版本，不需要在 yaml 里再指定。

**坑：registry / docs 经 `workspace:*` 依赖 sdk，而 `packages/sdk/dist` 是 gitignored、无 tsconfig path 把 `@swarm-hive/sdk` 指回源码** —— 所以 node job 跑 registry/docs 的 typecheck/test **前必须先 `pnpm --filter @swarm-hive/sdk build`**（docs.yml 部署链路同理，已是这个顺序）。registry 的脚本名是 **`build:registry`**（shadcn build，非 `build`）；docs 只有 `typecheck`/`build` 没有 `test`。2026-06-05 前 node job 只跑 admin，sdk 30 测试 + registry-rn 11 测试游离门禁外，已补齐。

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

### npm 发 @swarm-hive/sdk（TS 包，独立于 cargo-dist，2026-06-05 加）

**关键认知：cargo-dist 的 `release.yml` 那个 `publish-npm` job 发的是 CLI 二进制的 npm wrapper（`@swarm-hive/cli`，匹配 `*-npm-package.tar.gz`），跟 tsdown 构建的 `@swarm-hive/sdk` 完全无关。** SDK 是 pnpm/TS 包、无 Cargo.toml，进不了 cargo-dist 产物清单。发 SDK 走独立的 `.github/workflows/publish-sdk.yml`（tag `sdk/v*` 或 workflow_dispatch 触发，build→test→`pnpm --filter @swarm-hive/sdk publish --access public --no-git-checks`，复用 `NPM_TOKEN` secret）。

- `packages/sdk/package.json` 已 publish-ready：`publishConfig.access=public`（scoped 公开包必需）、`files:["dist"]`、无 `private`、加了 `prepublishOnly: tsdown`（防发过期 dist；`npm pack` 不受影响）。README.md 自动进包（npm 永远收 README/package.json/LICENSE，不看 `files`）。
- **⚠️ tag 碰撞坑（最终用 tag-namespace 配置层解决，2026-06-05 多轮实测）**：cargo-dist `release.yml` 默认触发器是贪婪的 `**[0-9]+.[0-9]+.[0-9]+*`，**也匹配 `sdk-v0.1.0`** → 推 SDK tag 会误触发 cargo-dist（plan 步失败）。
  - **走过的弯路（别重蹈）**：① 手改 release.yml tag 模式为 `v*` → `dist host` 启动会重生成并比对，手改触发 exit 255 硬失败中止整个 release（`regenerated: @@ ... run 'dist init'`）。② 加 `allow-dirty = ["ci"]` 能让 `dist generate` 跳过该文件（实测确实不覆盖），但 dist 升级时不自动更新它。
  - **正解（已应用）**：`dist-workspace.toml [dist]` 加 **`tag-namespace = "cli"`** → `dist generate` 产出 **`.github/workflows/cli-release.yml`**（删掉旧 `release.yml`），触发模式 `'cli**[0-9]+.[0-9]+.[0-9]+*'`，配置驱动、`dist generate` 每次都生成正确内容、无需 allow-dirty。**CLI tag 形如 `cli/v0.1.0`（斜杠分隔）**——实测 `cli-v0.1.0`（短横）被拒（`unexpected character 'c' while parsing major version`），接受 `cli/v0.1.0` / `cli/0.1.0` / `swarmhive-cli/0.1.0`。`sdk-v*` 不含 `cli` 前缀 → 不再误触发。
- **CLI 与 SDK 版本/tag 解耦**：CLI 走 **`cli/v0.1.0`** tag（cargo-dist cli-release.yml + crates.io publish-crates.yml 共同触发，都匹配 `cli**…`；要求 `crates/swarmhive-cli/Cargo.toml` version 与 tag 一致）；SDK 走 **`sdk/v0.1.0`** tag 或 workflow_dispatch（publish-sdk.yml，触发模式 `sdk/v[0-9]+…`，要求 `packages/sdk/package.json` version 与 tag 一致）。**统一斜杠规范 `<name>/v<version>`**：`cli/v*` 与 `sdk/v*` 对称、互不碰撞。首发都 `0.1.0`（注：0.1.0 这次 CLI 用的是迁移前的 `v0.1.0` tag，0.2.0 起用 `cli/v*`；SDK 0.1.0 是 workflow_dispatch 手动发的，无 tag）。
- 前置：`@swarm-hive` npm scope/org 已存在 + 账号有发布权限；首发前两包都是 E404。
- **⚠️ 首发踩的两个认证坑（2026-06-05 实测）**：
  1. **没设 `NPM_TOKEN` secret** → `${{ secrets.NPM_TOKEN }}` 解析成空 → `NODE_AUTH_TOKEN` 空 → `npm error code ENEEDAUTH`。`gh secret list --repo swarm-apps/SwarmHive` 查得（空=未设）。**必须先在仓库 Settings → Secrets → Actions 加 `NPM_TOKEN`**（对 @swarm-hive scope 有 publish 权限的 npm automation token）。publish-sdk.yml 已加空 token 守卫，空了直接报清晰错。
  2. **`.npmrc` 位置坑**：`pnpm publish` 底层调 `npm publish`、**在 `packages/sdk/` 内运行**，只读该目录 `.npmrc` + userconfig，**不读 workspace 根 `.npmrc`**（我第一版 `echo >> .npmrc` 写根目录无效，仍 ENEEDAUTH）。正解：`npm config set //registry.npmjs.org/:_authToken="${NODE_AUTH_TOKEN}"`（写 userconfig，npm 全局读，与 cwd 无关）再 `pnpm publish`。
  - 注意 auth 在到达 registry 前就失败，**版本号不会被烧**，修完同版本可重发。cargo-dist 的 release.yml 用 `npm publish`（非 pnpm），但**同样需要 `NPM_TOKEN`**（缺了 publish-npm/publish-homebrew job 会失败，但 GitHub Release + 二进制照常出，补 secret 后 re-run 那两个 job 即可）。

**相关文件**：`.github/workflows/publish-sdk.yml`、`packages/sdk/package.json`、`.github/workflows/release.yml`（贪婪 tag 模式 :45）。

### crates.io 发布 swarmhive-api-types + swarmhive-cli（2026-06-05 加）

cargo-dist 只发**二进制** + npm wrapper + homebrew，**不发 crates.io**。`cargo install swarmhive-cli` 路径靠独立的 `.github/workflows/publish-crates.yml`（同 `v*` tag 触发，与 release.yml 并行）。

- **依赖顺序硬约束**：`swarmhive-cli` 依赖 `swarmhive-api-types`，crates.io **不认 path 依赖**，必须**先发 api-types**、cli 才能解析。workflow 里 `cargo publish -p swarmhive-api-types` → `cargo publish -p swarmhive-cli`（cargo publish 会等新版本在 index 可用后返回，cli 才解析得到）。本地实测：api-types 没上 crates.io 时 `cargo package -p swarmhive-cli` 直接 `no matching package ... in crates.io index`。
- **path 依赖必须带 version**：root `Cargo.toml` 的 `swarmhive-api-types = { path = "...", version = "0.1.0" }` —— 没 version 则 `cargo publish` cli 失败。version 必须与 api-types 实际版本 semver 兼容（都 0.1.0）。
- **只发这两个**：`swarmhive-entity` / `swarmhive-server` 不发 crates.io（cli 不依赖 entity/sea-orm，是[[architecture]]的边界 guard；server 是应用不是库）。
- **secret**：需 `CARGO_REGISTRY_TOKEN`（crates.io → Account → API Tokens，scope 含 publish-new + publish-update）。workflow 有空 token 守卫 + tag/版本一致性检查。幂等：已发布同版本的 `already exists` 被容忍跳过（便于 re-run）。
- 版本：api-types 同 cli 一起 0.1.0 首发（`cargo publish --dry-run` 验过 api-types 打包 + verify 编译通过）。

**相关文件**：`.github/workflows/publish-crates.yml`、`crates/swarmhive-api-types/Cargo.toml`、根 `Cargo.toml`（workspace.dependencies api-types version）。

### server 容器镜像 + 单文件二进制（`server/v*`，2026-06-09 加）

server 是第三条独立 release 线（CLI=`cli/v*`、SDK=`sdk/v*`、server=`server/v*`），**不**走 cargo-dist（`crates/swarmhive-server/Cargo.toml` 显式 `[package.metadata.dist] dist = false`）。`.github/workflows/server-release.yml`（tag `server/v*` + `workflow_dispatch`）一条 tag 同出 **GHCR 多架构镜像** + **GitHub Release Linux 二进制**，都用 `--features embed-spa` 把 `apps/admin/dist` 经 rust-embed 内嵌（单镜像/单二进制同服务 `/api` 与 admin 后台）。

**rust-embed 接线（`crates/swarmhive-server/src/spa.rs` + `lib.rs` build_router fallback）**：
- 用 **`embed-spa` cargo feature 门控**，不是无条件嵌入。`#[derive(RustEmbed)] #[folder = "../../apps/admin/dist"]` 在 `dist` 不存在时**编译期报错**，而 dev/CI/集成测试普遍没构建过 SPA。默认关 → `cargo build/test/clippy` 零变化、无需 dist；release 构建前先 `pnpm admin:build` 再 `--features embed-spa`。
- fallback 用 `.fallback(spa::fallback_handler)`，只接 axum **未匹配**的路由，故 `/api/*`、`/healthz`、`/api/docs` 天然优先不被遮蔽；命中嵌入资源按 `mime_guess` 返回，未命中回退 `index.html`(200) 让前端路由接管。feature 关时不挂 fallback，未匹配仍 404。

**Dockerfile（根目录，多阶段 node→rust→debian-slim）实战坑**：
- **`pnpm install --frozen-lockfile` 会跑 root `postinstall: lefthook install` → 在容器里炸**。lefthook（2.1.8 npm wrapper）**无条件 exec `git`** 且需要 `.git` 仓库，而 node:slim 没 git、`.dockerignore` 又排除了 `.git`。`LEFTHOOK=0` **无效**（lefthook 的 `install` 命令不理这个 env）。**正解**：spa 阶段 `apt-get install -y git` + `git init -q .`（仅本阶段的临时空仓库），让 lefthook install 正常写 hooks 退出 0——等价 CI 里有 `.git` 的环境，且不必 `--ignore-scripts`（那会让 esbuild/rollup 的平台二进制 postinstall 缺失，风险大）。CI 的 binaries job 不受此坑影响（runner 有 git + `actions/checkout` 带 `.git`）。
- **aws-lc-sys（aws-sdk-s3 的 rustls 加密后端，Cargo.lock 里 `aws-lc-sys` + `cmake` crate）构建期需要 `cmake`**（C 编译器 rust 镜像自带，无 `bindgen` → 不需要 libclang）。rust 阶段 + binaries job 都 `apt-get install -y cmake`。
- **全栈 rustls（reqwest `rustls-tls-native-roots` / lettre `tokio1-rustls-tls` / oauth2 `rustls-tls` / sea-orm `runtime-tokio-rustls`）→ 运行时镜像不需要 OpenSSL，只需 `ca-certificates`**（native-roots 读 OS 证书库）。runtime 用 `debian:bookworm-slim` + `ca-certificates` + 非 root。无 musl → glibc 动态构建（aws-lc-sys/ring 的 C 依赖让 musl 静态构建得不偿失）。
- cargo-chef 缓存依赖层（`cargo chef cook --release` 单独一层，源码改动命中缓存）；`/usr/local/cargo/registry` + pnpm store 都用 BuildKit `--mount=type=cache`。
- 镜像携 `config/default.toml`，server 读 cwd `/app/config/default.toml`；生产用 env `SWARMHIVE_*__*` 覆盖（DB URL / SECRET_KEY / BASE_URL 必填）。

**工作流多架构（避免 QEMU）**：镜像用 **native runner 矩阵 + manifest 合并**——`ubuntu-latest`(amd64) + `ubuntu-24.04-arm`(arm64) 各 `build-push-action` 按 digest push（`outputs: ...push-by-digest=true`），再 `image-merge` job 用 `docker buildx imagetools create` 合 manifest 打 tag（`metadata-action` 的 `type=match,pattern=server/v(\d+\.\d+\.\d+),group=1`）。单 runner QEMU 模拟 arm64 跑 LTO release 极慢，故弃用。二进制矩阵同样 native runner（host 三元组，无 cross），`softprops/action-gh-release` 上传。`packages: write` 权限给镜像 job，`contents: write` 给二进制 job。

**相关文件**：`.github/workflows/server-release.yml`、根 `Dockerfile`、`.dockerignore`、`deploy/docker-compose.yml`、`crates/swarmhive-server/{Cargo.toml,src/spa.rs,src/lib.rs}`、`openspec/changes/add-server-container-and-release/`。

## 已知 Windows quirk

- 重命名 Rust 源目录可能留下空父目录被 rust-analyzer / VSCode file watcher 锁住——非致命，只要内部没 `Cargo.toml`（workspace 会忽略）。关 watcher 清掉
- `core.autocrlf` 开着，预期 `LF will be replaced by CRLF` 警告。**不要**试图"修"它
