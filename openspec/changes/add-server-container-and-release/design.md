# Design — add-server-container-and-release

## 构建 / 交付数据流

```text
                         repo (monorepo: pnpm ws + cargo ws)
                                      │
        ┌─────────────────────────────┼──────────────────────────────┐
        │  Dockerfile (单一自包含)      │   server-release.yml (CI)     │
        │                             │                               │
        ▼                             │                               ▼
  ┌───────────────┐                   │                    ┌────────────────────┐
  │ stage: spa     │  pnpm install     │      job: binaries │ matrix:            │
  │ node:22-slim   │  pnpm admin:build  │      (CI 直跑)      │  ubuntu-latest     │  (x86_64-gnu)
  │ → apps/admin/  │                   │                    │  ubuntu-24.04-arm  │  (aarch64-gnu)
  │   dist          │                   │                    └─────────┬──────────┘
  └───────┬────────┘                   │       pnpm admin:build        │
          │ COPY dist                  │       cargo build --release   │
          ▼                            │         --features embed-spa  │
  ┌───────────────┐                    │                    ┌──────────▼─────────┐
  │ stage: build   │ cargo-chef cook    │                    │ tar.gz             │
  │ rust + cmake   │ (deps 缓存层)       │                    │  swarmhive-server  │
  │ COPY dist →    │ cargo build         │                    │  + config/default  │
  │ rust-embed 嵌入 │  --features         │                    └──────────┬─────────┘
  │ release binary │  embed-spa          │                               │ upload
  └───────┬────────┘                    │                               ▼
          │ COPY binary + config         │                     GitHub Release (server/v*)
          ▼                             │
  ┌───────────────┐                     │      job: image
  │ stage: runtime │ debian-slim         │      buildx --platform
  │ ca-certificates│ 非 root             │      linux/amd64,linux/arm64
  │ /app/config/   │ EXPOSE 3030         │      docker build -f Dockerfile .
  │ ENTRYPOINT     │ HEALTHCHECK /healthz│      → push ghcr.io/swarm-apps/
  └───────┬────────┘                     │              swarmhive-server:{ver,latest,sha}
          ▼
  本地 `docker build .` 与 CI image job 共用同一 Dockerfile（自包含、不依赖外部预构建 dist）
```

**关键点**：Dockerfile **自包含**（自己跑 SPA 构建），保证本地 `docker build .` 与 CI 行为一致；binaries job 不复用 Docker，单独跑一次 `pnpm admin:build`（SPA 构建 ~30s，双跑可接受，换来 Dockerfile 不被 CI 特化）。

## 运行时路由（嵌入 SPA 后）

```text
  请求 ──► axum Router
            ├─ /healthz                       → health handler
            ├─ /api/*  (含 /api/openapi.json,  → 业务 handler / Redoc
            │          /api/docs)               (matched route, 优先于 fallback)
            └─ 其它任意路径  ──► .fallback(spa::handler)   [仅 embed-spa feature]
                                   │
                                   ├─ SpaAssets::get(path) 命中 → 200 + mime_guess
                                   └─ 未命中 → index.html (200)  ← SPA client-side route 回退
```

`.fallback` 只接 axum 未匹配的路由，故 `/api/*`、`/healthz`、`/api/docs` 这些已注册路由天然优先，fallback 不会遮蔽它们。feature 关闭时不挂 fallback，未匹配路由仍是 axum 默认 404（与当前行为一致）。

## 决策记录

### embed-spa 用 cargo feature 门控（不是 cfg(debug_assertions)）

`#[derive(RustEmbed)] #[folder = "../../apps/admin/dist"]` 在 `dist` 不存在时**编译期报错**。dev/CI 普遍没有 `dist`（它 gitignored、只在需要时构建），若无条件嵌入会让 `cargo build`/`cargo test`/clippy 全部依赖先跑 `pnpm admin:build`，破坏现有工作流。故用 `embed-spa` feature 把整个 `spa` 模块 + `build_router` 的 `.fallback` 门控起来：

- **默认关**：`cargo build/test/clippy`、集成测试、e2e drift gate 全不变，无需 `dist`。
- **显式开**：Dockerfile 与 binaries job 用 `--features embed-spa`，构建前保证 `dist` 已就位。

rust-embed 在 release 构建下把文件字节嵌入二进制（与目标三元组无关），故跨架构（aarch64）构建嵌入同一份 dist 没有问题。

### 为什么不接 cargo-dist 发 server

cargo-dist 单 workspace 单配置，server 一旦纳入会和 CLI 的 `cli/v*` 发布耦合（dist plan 会多出一个 release、tag 触发互相串台，见 toolchain.md 的 tag-namespace 血泪）。且 cargo-dist 不会跑 `pnpm admin:build`，无法嵌入 SPA。故 server 走独立 `server-release.yml`，与 cli/sdk 三条线靠 `<name>/v*` tag 命名空间彻底解耦。

### 镜像/二进制都只覆盖 Linux

server 是自托管后端，部署面 99% 是 Linux（x86 云主机 + ARM Graviton/Ampere）。镜像出 `linux/amd64` + `linux/arm64`（buildx 多架构 manifest，native runner 无 QEMU）；裸二进制出 `x86_64`/`aarch64-unknown-linux-gnu`（glibc 动态，因 aws-lc-sys + ring 的 C 依赖让 musl 静态构建得不偿失，运行时依赖仅 `ca-certificates`，全栈 rustls 无需 OpenSSL）。

### tag → 镜像/release tag 映射

`server/vX.Y.Z`（斜杠，与 `cli/v*`、`sdk/v*` 对称）→ 镜像 tag `X.Y.Z` + `X.Y` + `latest` + `sha-<short>`；GitHub Release 名 `server/vX.Y.Z`。`workflow_dispatch` 仅出 `sha` 标签镜像、不动 Release（便于无 tag 试构）。
