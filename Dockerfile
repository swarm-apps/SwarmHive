# syntax=docker/dockerfile:1
#
# 需要 BuildKit(用了 `--mount=type=cache`)。Docker 23+ 默认即 BuildKit;更老的 Docker 用
# `DOCKER_BUILDKIT=1 docker build .` 或 `docker buildx build .`。
#
# SwarmHive server —— 单镜像同时服务 /api 与内嵌 admin 后台。
#
#   docker build -t swarmhive-server .
#   docker run -p 3030:3030 \
#     -e SWARMHIVE_DATABASE__URL=postgres://user:pass@host:5432/swarmhive \
#     -e SWARMHIVE_SECRET_KEY="$(openssl rand -base64 32)" \
#     -e SWARMHIVE_SERVER__BASE_URL=https://updates.example.com \
#     swarmhive-server
#
# 多阶段:① node 构建 admin SPA → ② rust(cargo-chef 缓存 + cmake 给 aws-lc-sys)
# 把 dist 经 rust-embed 嵌进 release 二进制 → ③ debian-slim 瘦运行时。
# 自包含:本地 `docker build .` 与 CI(server-release.yml)共用本文件,不依赖外部预构建 dist。

# ─────────────────────────────────────────────────────────────────────────────
# ① SPA 阶段:pnpm 构建 apps/admin → apps/admin/dist
# ─────────────────────────────────────────────────────────────────────────────
FROM node:22-bookworm-slim AS spa
ENV PNPM_HOME=/pnpm
ENV PATH="/pnpm:$PATH"
# git:root package.json 的 postinstall 是 `lefthook install`,它无条件 exec git 并需要
# 一个 .git 仓库才能写 hooks。镜像里 .git 被 .dockerignore 排除,故装 git 后 `git init`
# 一个临时空仓库(仅本阶段,不带进后续阶段),让 lefthook install 正常写入并退出 0
# —— 等价于 CI 里"有 .git"的环境,且不必 --ignore-scripts(避免 esbuild/rollup 平台二进制缺失)。
RUN apt-get update \
    && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/*
RUN corepack enable
WORKDIR /app
# .dockerignore 已剔除 node_modules / dist / target;整源拷入由 pnpm store cache 兜增量。
COPY . .
RUN git init -q . \
    && git config user.email ci@swarmhive.local \
    && git config user.name swarmhive-ci
# pnpm store 走 BuildKit cache mount,源码变更也不必重下依赖。
RUN --mount=type=cache,target=/pnpm/store \
    pnpm config set store-dir /pnpm/store \
    && pnpm install --frozen-lockfile
# 构建 admin SPA(schema.gen.ts 已入库;routeTree.gen.ts 由 vite 插件生成)。
RUN pnpm admin:build

# ─────────────────────────────────────────────────────────────────────────────
# ② Rust 构建阶段:cargo-chef 依赖缓存 + 把 SPA 嵌入二进制
# ─────────────────────────────────────────────────────────────────────────────
FROM rust:1-bookworm AS chef
# aws-lc-sys(aws-sdk-s3 的 rustls 加密后端)构建需要 cmake;C 编译器 rust 镜像自带。
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# 先只用 recipe.json cook 依赖 —— 这一层只随 Cargo.lock/Cargo.toml 变,源码改动命中缓存。
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo chef cook --release --recipe-path recipe.json
# 复制全部源码 + 上一阶段构建好的 SPA(rust-embed 的 #[folder] 指向 apps/admin/dist)。
COPY . .
COPY --from=spa /app/apps/admin/dist apps/admin/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p swarmhive-server --bin swarmhive-server --features embed-spa

# ─────────────────────────────────────────────────────────────────────────────
# ③ 运行时:debian-slim + ca-certificates(全栈 rustls,无需 OpenSSL),非 root
# ─────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
# 系统非 root 用户运行(server 不写本地磁盘,元数据落 Postgres、产物落 S3)。
RUN useradd --system --uid 10001 --user-group --home-dir /app --shell /usr/sbin/nologin swarmhive
WORKDIR /app
COPY --from=builder /app/target/release/swarmhive-server /usr/local/bin/swarmhive-server
# server 读取 cwd 下的 config/default.toml;生产用 env(SWARMHIVE_*__*)覆盖。
COPY config/default.toml ./config/default.toml
USER swarmhive
EXPOSE 3030
# 生产默认 info 级别;可用 RUST_LOG 覆盖。
ENV RUST_LOG=info,swarmhive_server=info
# 容器自检命中 /healthz(server 绑 0.0.0.0:3030)。
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3030/healthz || exit 1
ENTRYPOINT ["swarmhive-server"]
