# Repository Guidelines

## 项目结构与模块组织

SwarmHive 是面向 Tauri 桌面应用和 React Native Android 应用的自托管更新发布中枢。Rust workspace 在 `crates/`：`swarmhive-server` 是 Axum 后端，`swarmhive-cli` 产出二进制 `swarmhive`，`swarmhive-api-types` 存放共享 DTO，`swarmhive-entity` 与 `swarmhive-migration` 负责 SeaORM 实体和迁移。前端在 `apps/`：`admin` 是 Vite/React 管理后台，`docs` 是 Next/Fumadocs 文档站。SDK 与 shadcn registries 在 `packages/`，产品文档在 `docs/`，OpenSpec 变更在 `openspec/changes/`，agent 集成指南在 `skills/`。

## 开发工作流

非平凡改动前先阅读 `docs/README.md`、`docs/03-architecture.md`，并按主题查看 `dev-notes/knowledge/`。OpenSpec 工作遵循 `dev-notes/knowledge/openspec-workflow.md`。保持核心边界：`swarmhive-cli` 只依赖 API 类型，不依赖 entity/SeaORM；SDK 保持无 UI；UI 组件通过 `registry-web` / `registry-rn` 分发；Web Admin 负责查看、配置和分析，发布主路径优先走 CLI。

## 构建、测试与本地开发

- `pnpm install`：安装前端依赖并写入 lefthook hooks。
- `pnpm admin:dev` / `pnpm docs:dev`：启动管理后台或文档站。
- `cargo run -p swarmhive-server`：启动后端，默认监听 `:3030`。
- `cargo run -p swarmhive-cli -- <subcommand>`：运行 CLI，例如 `login`、`publish`、`storage`。
- `pnpm admin:build` / `pnpm docs:build`：构建两个 Web 应用。
- `cargo build --workspace`：编译全部 Rust crate。
- `cargo test --workspace`：运行 Rust 单元与集成测试。
- `cargo clippy --workspace --all-targets`：执行 Rust lint。
- `pnpm --filter @swarm-hive/sdk test`：运行 SDK Vitest 测试。
- `pnpm --filter @swarm-hive/admin test` / `test:e2e`：运行后台单测与 Playwright smoke 测试。
- `pnpm lint` / `pnpm format`：用 Biome 检查或修复 TS、JSON、CSS 等文件。

## 代码风格与命名约定

仓库使用 LF、空格缩进；默认 2 空格，Rust 为 4 空格。Rust 必须通过 `cargo fmt`，workspace 禁用 `unsafe_code`；依赖统一放在根 `Cargo.toml` 的 `[workspace.dependencies]`。TypeScript/React 使用 Biome，行宽 100，启用推荐规则和自动整理 imports。Rust 模块与文件用 `snake_case`；React 组件和 Provider 用 `PascalCase`；hooks 用 `useXxx`。品牌名遵循 `SwarmHive` / `swarmhive` / `SWARMHIVE`。开发者注释用中文，面向用户的 CLI help、OpenAPI 描述、错误标题和日志 action 保持英文。

## 测试指南

优先在变更所属包附近补测试。Rust 集成测试放在对应 crate 的 `tests/` 目录，常见命名为 `*_smoke.rs`，部分测试依赖 Docker/Testcontainers。前端、SDK、registry 包使用 Vitest；管理后台浏览器流程使用 Playwright。修改 OpenAPI 生成类型时，先运行对应的 `openapi` 或 `codegen` 脚本。不要手改 `apps/admin/src/routeTree.gen.ts` 或 registry 的生成 JSON。

## 提交与 Pull Request 规范

提交信息使用 Conventional Commits，并由 commitlint 校验；常见格式包括 `feat(cli): ...`、`fix(server): ...`、`test(admin): ...`、`chore(release): ...`。保持提交聚焦，避免把生成物和无关修改混在一起。PR 应说明用户可见变化、列出验证命令、关联 issue 或 OpenSpec 变更；涉及 `admin` 或 `docs` UI 时附截图。

## 架构与配置提示

本地 Admin 默认在 `:5173`，代理 `/api` 与 `/healthz` 到 server `:3030`；API 路径应保持在 `/api/...` 下。存储抽象只支持 S3-compatible 后端，单机部署通过 RustFS 走同一接口，不新增本地文件系统后端。从 `.env.example` 开始配置本地环境，不提交密钥；`config/default.toml` 只放非敏感默认值，部署差异优先用 `SWARMHIVE_<SECTION>__<KEY>` 环境变量覆盖。
