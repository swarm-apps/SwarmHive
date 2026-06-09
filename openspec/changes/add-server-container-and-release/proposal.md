# add-server-container-and-release

## Why

server 是唯一没有发布管线的产物：CLI 走 `cli/v*`（cargo-dist + crates.io），SDK 走 `sdk/v*`（npm），server 在 cargo-dist 里被显式 `dist = false` 排除，至今**没有任何可分发形态**。同时 `docker-compose.yml`、`dist-workspace.toml`、self-host 文档与 `openspec/config.yaml` context 都已把"单二进制 + rust-embed 内嵌 admin SPA + 容器/源码构建"当成既定部署形态在写，但 `rust-embed`（`Cargo.toml` 已声明 8.x）从未接线——`build_router` 只挂 API + Redoc，没有 SPA fallback。本 change 兑现这条承诺：让 server 单二进制同时服务 `/api` 与 admin 后台，并为它建立独立的容器镜像（GHCR）与单文件二进制（GitHub Release）发布线。

## What

1. **server 服务嵌入的 admin SPA**（能力，见 `specs/server-spa-embedding/`）：新增 `crates/swarmhive-server/src/spa.rs`，用 `#[derive(RustEmbed)]` 嵌入 `apps/admin/dist`，在 `build_router` 末尾挂一个 `.fallback`，把非 `/api`、非 `/healthz` 的路径交给 SPA（静态资源按 mime 返回，其余路由回退 `index.html`）。整段由 `embed-spa` cargo feature 门控——默认关闭，dev/CI/集成测试零行为变化；release 容器/二进制构建显式开启。
2. **多阶段 Dockerfile + `.dockerignore`**（根目录）：node 阶段 `pnpm admin:build` 产出 `dist` → rust 阶段（cargo-chef 缓存依赖 + 装 `cmake` 给 aws-lc-sys）`cargo build --release -p swarmhive-server --features embed-spa` 把 dist 嵌进二进制 → `debian:bookworm-slim` 运行时（仅 `ca-certificates`、非 root、携 `config/default.toml`、`EXPOSE 3030`）。
3. **`.github/workflows/server-release.yml`**（`server/v*` tag + `workflow_dispatch` 触发）：① buildx 构建 `linux/amd64` + `linux/arm64` 双架构镜像推 `ghcr.io/swarm-apps/swarmhive-server`（`packages: write`、metadata-action tag 策略、gha layer cache）；② 矩阵在 native runner（`ubuntu-latest` + `ubuntu-24.04-arm`）构建 Linux x86_64/aarch64 的 `embed-spa` 二进制，打 `tar.gz` 挂 GitHub Release。
4. **文档 / compose / 知识库同步**：self-host 加容器部署段 + 生产 compose 示例（`deploy/`）；`dev-notes/knowledge/toolchain.md`、`docs/03`/`docs/06` 同步。

## Acceptance

- `cargo build -p swarmhive-server`（默认 feature）仍通过；`cargo test --workspace` 不受影响。
- `pnpm admin:build && cargo run -p swarmhive-server --features embed-spa` 后：`curl localhost:3030/` 返回 admin SPA 的 `index.html`，`curl localhost:3030/api/v1/version` 仍返回版本 JSON，`/api/openapi.json`、`/healthz` 不被 fallback 遮蔽。
- `docker build -t swarmhive-server .` 成功产出一个能 `docker run` 起来、`/healthz` 200、根路径出 admin 后台的镜像。
- 推 `server/v0.1.0` tag 后，GHCR 出 `ghcr.io/swarm-apps/swarmhive-server:0.1.0`/`:latest` 多架构镜像，GitHub Release 挂上 `swarmhive-server-<ver>-x86_64-unknown-linux-gnu.tar.gz` 与 `…-aarch64-…tar.gz`。

## Non-goals

- **不**给 server 接 cargo-dist（保持 `dist = false`，与 CLI 发布解耦；server 走本 change 的独立工作流）。
- **不**出 macOS / Windows server 二进制（自托管后端只覆盖 Linux x86_64/aarch64；镜像同样仅 linux 双架构）。
- **不**做 nginx/caddy TLS 终止的自动化（生产 compose 示例里给反代占位说明，不内置证书签发）。
- **不**改 session cookie `with_secure`、不引入新 endpoint/实体/DB schema、不改 api-types/entity/cli 边界。
- **不**实现 admin UI 的新功能（Dashboard 真实数据等仍归属各自 proposal）。

## Depends on

- `add-admin-frontend-foundation`（已归档）—— SPA 构建产物 `apps/admin/dist` 是嵌入源。
- 既有 `cli-release.yml` / `publish-sdk.yml` 建立的 `<name>/v<version>` tag 命名空间约定 —— 本 change 新增 `server/v*` 与之对齐。

## Maps to docs

- `docs/03-architecture.md`（单二进制 + 嵌入 SPA、single-server compose 部署形态）
- `docs/06-cicd.md`（发布管线）
- `apps/docs/content/docs/self-host/index.mdx`（第 2、6 节部署形态）
- `dev-notes/knowledge/toolchain.md`（Release 分发段）
