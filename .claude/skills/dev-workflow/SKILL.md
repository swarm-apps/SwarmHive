---
name: dev-workflow
description: |
  SwarmHive 项目开发工作流技能。在以下场景自动调用：
  (1) 编写或修改任何 crates/*/src/ 或 apps/admin/src/ 下的代码
  (2) 添加新依赖（Cargo.toml / package.json）或修改配置文件
  (3) 完成一个 feature、修复一个 bug、应用一个 OpenSpec proposal
  (4) 创建、推进或归档 openspec/changes/*
  触发关键词：crate 改动、sea-orm entity、axum handler、AntD 组件、TanStack Query、OpenSpec proposal、依赖升级、配置变更、apply change、归档 change
---

# Dev Workflow — SwarmHive 项目开发工作流

## 工作流程

### 1. 开发前：加载相关知识

根据当前任务，读取 `dev-notes/knowledge/` 下的相关主题文件：

| 主题文件 | 何时读 |
|---|---|
| [architecture.md](../../../dev-notes/knowledge/architecture.md) | 跨 crate 边界、storage、上传链路、SDK / registry 分发、部署形态相关改动 |
| [backend.md](../../../dev-notes/knowledge/backend.md) | 写 sea-orm entity / axum handler / auth / mailer / storage trait / RFC 9457 错误 |
| [admin-spa.md](../../../dev-notes/knowledge/admin-spa.md) | 修 `apps/admin/`、加 AntD 组件、TanStack Router/Query、utoipa client 类型同步 |
| [toolchain.md](../../../dev-notes/knowledge/toolchain.md) | Cargo / pnpm workspace 改动、Biome / Lefthook / commitlint / git-cliff、Rust 工具链 |
| [openspec-workflow.md](../../../dev-notes/knowledge/openspec-workflow.md) | 创建 / 推进 / 归档 `openspec/changes/*`，proposal 命名、依赖图、tasks/design 模板 |

**读取方式**：使用 Read 工具读取对应文件，遵循其中记录的最佳实践和注意事项。

如果不确定读哪个，读取 `dev-notes/knowledge/` 目录列表，根据文件名判断。

### 2. 开发中：遵循最佳实践

同时参考以下通用 skill（如果与当前任务相关，自动调用）：

- `/sea-orm-2` — sea-orm 2.0 Entity 写法、关系建模、查询、嵌套 ActiveModel、raw_sql!、RBAC（项目本地）
- `/antd` — Ant Design 6 + Pro Components 用法、token、migration（项目本地）
- `/rust-best-practices` — Rust 通用规范（借用 / clone、错误处理、性能）
- `/rust-async-patterns` — Tokio、async trait、并发模式
- `/vercel-react-best-practices` — React 性能优化（项目不是 Next.js，但 React 部分通用）
- `/opsx:explore` / `/opsx:propose` / `/opsx:apply` / `/opsx:archive` — OpenSpec 流程

**优先级**：项目知识库 > 通用 skill > Claude 自身知识。当项目知识库中有明确记录时，以项目知识库为准。

### 3. 开发后：更新知识库

完成代码修改后，**检查是否产生了新的项目知识**：

**需要记录的内容**：
- 新引入的依赖及其正确用法（尤其是 sea-orm 2 RC、utoipa、garde 这种生态不稳定项）
- 发现的配置坑和 workaround（如 ICU 2.2 要求 1.86 → MSRV 1.90 的取舍）
- 做出的架构决策及原因（如 Postgres only、4 crate 边界、不引 sea-orm-migration）
- 与通用最佳实践不同的项目特定做法（如 schema-sync 取代 migration crate）
- 解决的 bug 的根因（如果不明显的话）

**不需要记录的内容**：
- 代码本身能表达的东西（看代码就能懂）
- 通用编程知识（不特定于本项目）
- git log / blame 能查到的东西
- 临时性的调试信息

**更新方式**：
1. 判断属于哪个主题文件
2. 追加新条目到对应文件的合适分类下
3. 如果现有主题都不合适，先考虑合并；新建主题文件需明确范围不重叠
4. 如果发现已有条目过时，更新或删除它

**条目格式**：

```markdown
### 条目标题

简短描述做了什么、为什么这样做。

**正确做法**：
- 具体的代码模式或配置

**不要做**（如果有）：
- 错误的做法及原因

**相关文件**：`path/to/file`
```

### 4. 代码质量检查

开发完成后，运行 `/simplify` 检查代码质量。lint/format/typecheck 命令：

```bash
# Rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Frontend / shared (Biome 管 JS/TS/JSON 全仓)
pnpm lint
pnpm --filter @swarmhive/admin typecheck
pnpm admin:build
```

**lefthook pre-commit 会自动跑 `biome check` + `cargo fmt --check`**。如果 `cargo fmt --check` 失败，跑 `cargo fmt --all` 重新 stage，**不要** `--no-verify` 绕过。
