# Tasks — add-server-container-and-release

> 状态(2026-06-09):代码 / Dockerfile / 工作流 / 文档全部落盘并经本地端到端实测
> (`docker build` + `docker run` + curl 全端点 200,SPA fallback 正确);唯一不能本地跑的
> 是 GitHub 上的 tag 触发 CI —— 待推 `server/v0.1.0` 后观测 GHCR 镜像 + Release 产物,再 archive。

## 1. server 嵌入 admin SPA(embed-spa feature)

- [x] 1.1 `crates/swarmhive-server/Cargo.toml` 加 `[features] embed-spa = []`
- [x] 1.2 新增 `crates/swarmhive-server/src/spa.rs`(`#[cfg(feature)]` 门控 `#[derive(RustEmbed)]` + `fallback_handler`,mime_guess,index.html 回退)
- [x] 1.3 `lib.rs` `pub mod spa;` + `build_router` 末尾 `#[cfg(feature = "embed-spa")] .fallback(...)`
- [x] 1.4 `cargo check` 默认 + `--features embed-spa` 均通过;`cargo fmt --check` 无 diff;`cargo clippy`(两 feature)`-D warnings` 全绿

## 2. Dockerfile + .dockerignore

- [x] 2.1 根 `Dockerfile` 多阶段(node spa → rust chef/builder + cmake → debian-slim runtime,非 root + ca-certificates + HEALTHCHECK)
- [x] 2.2 根 `.dockerignore`(剔除 node_modules/target/dist/.git/本地密文,保留所有 package.json)
- [x] 2.3 `docker build` 成功(镜像 198MB)+ `docker run`(配 pg)curl 实测:`/healthz` 200、`/api/v1/version` 200 json、`/` 200 SPA(`<title>SwarmHive Admin</title>`)、`/apps/swarmdrop` 200 回退 index、`/api/openapi.json` 不被遮蔽、`/assets/*.js` 200 text/javascript
  - 踩坑已记知识库:root `postinstall: lefthook install` 需 git+.git → spa 阶段装 git + `git init`

## 3. server-release.yml(GHCR + GitHub Release)

- [x] 3.1 `.github/workflows/server-release.yml` 触发 `server/v*` + `workflow_dispatch`;分级 permissions;concurrency
- [x] 3.2 image:native runner 矩阵(amd64 ubuntu-latest / arm64 ubuntu-24.04-arm)push-by-digest → image-merge 合 manifest(metadata-action `type=match server/v(\d+\.\d+\.\d+)`)
- [x] 3.3 binaries 矩阵 x86_64/aarch64-unknown-linux-gnu native runner:pnpm admin:build → cmake → cargo build --features embed-spa → tar.gz + sha256 → action-gh-release
- [x] 3.4 YAML 解析通过 + 对抗式 review(arm runner 公开仓库可用性已注释说明)
- [x] 3.5 真实 CI 已观测:推 `server/v0.1.0` → run 27185176609 全 5 job 绿,GHCR `:0.1.0/:0.1/:latest` 双架构(amd64+arm64)manifest + GitHub Release 两个 `tar.gz`(x86_64/aarch64)+ `.sha256` 均出

## 4. 文档 / compose / 知识库

- [x] 4.1 `apps/docs/content/docs/self-host/index.mdx` 加「用容器跑(推荐)」+ 校正第 6 节 embed-spa 措辞
- [x] 4.2 `deploy/docker-compose.yml` + `deploy/.env.example` + `deploy/README.md`(独立 project/卷,不接管 dev)
- [x] 4.3 `docs/03-architecture.md` + `docs/06-cicd.md`(server 容器/二进制发布线 + 三 tag 命名空间表)
- [x] 4.4 `dev-notes/knowledge/toolchain.md`(server-release + Docker 踩坑全记)+ `CLAUDE.md` 命令段
- [x] 4.5 `openspec/changes/README.md` 依赖图 / 阶段映射 / 进度表

## 5. 收尾

- [x] 5.1 `cargo fmt --check` + clippy 无回归(spa 默认门控关,不动既有测试面)
- [x] 5.2 对抗式 review(Dockerfile 实测 + 工作流 review;Finding 2/4 核验为非问题,Finding 3 注释化)
- [x] 5.3 `openspec archive add-server-container-and-release`(真实 CI 观测通过后归档)
