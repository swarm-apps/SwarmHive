# OpenSpec Workflow

## 概览

SwarmHive 是 **proposal-driven** 项目：所有非 trivial 改动都先经 OpenSpec change proposal（`openspec/changes/<name>/`），再用 `/opsx:apply` 实施，最后 `/opsx:archive`。这是项目的核心方法论，比代码本身约束更严。

## 何时走 OpenSpec 流程

| 场景 | 走 OpenSpec？ |
|---|---|
| 新增 crate / 删 crate / 改 crate 边界 | ✅ 必须 |
| 新增数据库实体 / schema 变更 | ✅ 必须 |
| 新 API endpoint 集合（一组业务 endpoint） | ✅ 必须 |
| 加新依赖（影响多 crate） | ✅ 通常需要 |
| 修一个 typo、改一行 lint 配置 | ❌ 直接改 |
| 单文件 bug 修复（不触及边界） | ❌ 直接改 |
| docs 同步 / memory 更新 | ❌ 直接改 |

**疑问时走 OpenSpec**：保留决策记录的成本远低于回头补 archaeology。

## Change 命名约定

格式：`add-<kebab-case>` / `update-<kebab-case>` / `remove-<kebab-case>`。

**Why**：与 OpenSpec 工具的归档 / list 工具友好。

示例（来自项目）：
- `add-toolchain-bump` ✓
- `add-crate-restructure` ✓
- `add-persistence-foundation` ✓
- `add-auth-and-rbac` ✓
- `update-rbac-add-oauth` ✗（用 `add-oauth-github` 更精准）

**相关文件**：`openspec/changes/README.md` 依赖图。

## Change 目录结构

```text
openspec/changes/<name>/
├── proposal.md      ← 必有（Why / What / Acceptance / Non-goals / Depends on / Maps to docs）
├── design.md        ← 涉及跨 crate 边界或 DB schema 时有
├── tasks.md         ← 实施拆分（必有，给 /opsx:apply 跑用）
└── specs/           ← 新能力的 spec（可选）
```

### proposal.md 模板要素

每份 proposal 都包含：

1. **Why**：动机；引用相关 docs 章节或前置 proposal
2. **What**：具体改动清单（实体 / endpoint / 文件）
3. **Acceptance**：可测试的验收准则（命令 + 期望输出）
4. **Non-goals**：边界，明确**不**做什么
5. **Depends on**：前置 proposal（严格串行的基座要标）
6. **Maps to docs**：相关 docs/* 章节

**正确做法**：proposal 不超 500 字主体；越严格越好——非 goals 段越具体越能挡住 scope creep。

### design.md 何时需要

跨 crate 边界、DB schema 改动、复杂控制流时需要 design.md。**必须画一张 ASCII 数据流图**（项目约定，见 `openspec/config.yaml` rules）。

### tasks.md 粒度

任务粒度 **0.5–1 天**。标记 `[code]` / `[test]` / `[docs]`，便于并行。`/opsx:apply` 跑时按顺序逐个勾选 `- [ ]` → `- [x]`。

**相关文件**：`openspec/changes/add-persistence-foundation/tasks.md` 等已写好的示例。

## 依赖图 / 阶段映射

`openspec/changes/README.md` 是**总目录**：包含依赖图（哪个 proposal 必须先于哪个）+ 阶段映射（MVP 路线图阶段 0–10 对应 proposal）。

**正确做法**：
- 加新 proposal 时同步更新 README.md 的依赖图（哪怕只是加一条边）
- 推进 proposal 前先看 README.md 确认前置已完成
- 删 / 改 proposal 时同步 README.md，否则依赖图会引用幽灵节点

**当前推进顺序**：

```text
add-toolchain-bump  ✓ apply 完
add-crate-restructure  ✓ apply 完
add-persistence-foundation  ← 下一步
add-auth-and-rbac
↓ 之后可并行：oauth-github / pat-and-api-token / mail-infrastructure
↓ 业务主线：app-release-artifact → storage-and-presign-upload → update-check-{tauri,rn-android} → telemetry-events
↓ 横切：openapi-and-admin-client（每个有 handler 的 proposal 同步加注解，不积压）
```

**相关文件**：`openspec/changes/README.md`。

## /opsx 命令流

| 命令 | 何时用 |
|---|---|
| `/opsx:explore` | 进入 explore 模式思考问题，**不写代码**。可创建 OpenSpec artifact（proposal / design） |
| `/opsx:propose <name>` | 一步生成 proposal + design + tasks |
| `/opsx:apply <name>` | 按 tasks.md 逐条实施；自动勾选完成项 |
| `/opsx:archive <name>` | proposal 全部 task 完成后归档到 `openspec/changes/archive/` |

**正确做法**：
- explore 中如果发现实施层冲突，**先 pause apply** 回到 explore 改 proposal/design，再继续 apply（fluid workflow）
- apply 中遇到设计问题 → 直接更新 proposal/design/tasks，不是绕过
- 完成 apply 后**当下归档**，别留 stale "completed but not archived" 的 change

**相关文件**：本项目已有 archive 目录 `openspec/changes/archive/`（OpenSpec 默认结构）。

## spec delta 校验坑（`openspec validate` 静默/误报）

写 `changes/<name>/specs/<capability>/spec.md` 这类 delta 文件时，`openspec validate` 的解析器有几个非显然规则，踩中会报错或静默丢内容：

**正确做法**：
- **delta 文件不要加 H1**：直接以 `## ADDED Requirements` / `## MODIFIED Requirements` 开头。多写一个 `# 标题` 不会立即报错但属多余（known-good 归档 delta 全部无 H1）。
- **每个 requirement 描述的「第一物理行」必须含 `SHALL`/`MUST`**：校验器只取 `### Requirement:` 之后描述段的**第一行**来判 normative。若手动换行把 `SHALL` 挤到第二行 → 报 `must contain SHALL or MUST`（即使后文有 SHALL）。把首句重排成 `主语 SHALL ...` 放在同一物理行（别在 SHALL 前断行）。
- **Scenario 必须正好 4 个 `#`**（`#### Scenario:`）：3 个 `#` 或用 bullet 会**静默丢失**该 scenario，且"每个 requirement 至少一个 scenario"校验会失败。
- **MODIFIED 必须整段复制原 requirement 再改**：从 `### Requirement:` 标题到所有 scenario 全文搬过来编辑，标题文字要与 `openspec/specs/<capability>/spec.md` 逐字一致（空白不敏感）；只写改动片段会在 archive 时丢细节。
- 调试：`openspec change show <name> --json --deltas-only` 看解析器实际抓到的 `requirement.text`（就是它判 SHALL 的那一行），一眼看出首行是否截断。

**相关文件**：`openspec/changes/*/specs/**/spec.md`；范例 `openspec/changes/archive/2026-06-03-add-artifacts-table-and-guided-upload/specs/web-artifact-upload/spec.md`（含 MODIFIED 段）。

## 跨 proposal 联动

改动可能影响其他还没启动的 proposal——必须扫描并更新。

**示例**：`add-crate-restructure` 删了 `swarmhive-core` crate，则 `add-auth-and-rbac`、`add-mail-infrastructure`、`add-app-release-artifact`、`add-oauth-github` 这些**还没启动**的 proposal 里的 `swarmhive-core/src/` 路径都要批量改为 `swarmhive-server/src/` 或 `swarmhive-entity/src/`。

**正确做法**：apply 一个 proposal 收尾时跑：

```bash
grep -rn "<被删的旧概念>" openspec/changes/ docs/ memory/
```

把残留的命名 / 路径都更新掉。这条放进 apply tasks.md 的"7. docs / memory 同步"段或类似位置。

**相关文件**：`openspec/changes/add-crate-restructure/tasks.md` "8. 跟随其他 proposal 联动" 段（示范）。

## OpenSpec config.yaml

```yaml
schema: spec-driven
context: |
  # 项目背景 + tech stack + conventions
rules:
  proposal:
    - 每个 proposal 聚焦一个可独立验收的能力
    - 始终包含 "Non-goals" 段
  design:
    - 跨 crate 边界必须画一张 ASCII 数据流图
  tasks:
    - 任务粒度 0.5–1 天
```

**正确做法**：rules 出现变化（比如加新约定）要同时改 `config.yaml`，让 `/opsx:propose` 生成新 artifact 时遵守。

**相关文件**：`openspec/config.yaml`。

## 与 docs/ 和 memory/ 的关系

| 系统 | 职责 | 生命周期 |
|---|---|---|
| `docs/01-*.md` ~ `docs/14-*.md` | 产品 / 架构设计文档 | 长期，proposal 落地时同步更新 |
| `memory/project-*.md` | 项目级决策上下文（why、how to apply） | 长期，新决策时新增条目 |
| `openspec/changes/*` | 单个改动的提案 + 实施记录 | 临时（apply 完归档） |
| `openspec/specs/*` | 新能力的形式化 spec | 长期，跟随能力演进 |

**正确做法**：
- proposal 落地时**必须**同步更新 docs/ 中对应章节（proposal tasks.md 的 docs section）
- proposal 中产生的决策（"为什么这么选"）要落到 memory/，不要只留在 proposal.md 里——proposal 归档后查阅会很麻烦

**相关文件**：`docs/README.md`、`memory/MEMORY.md`。

## explore 模式的边界

`/opsx:explore` 是**思考模式**，不写代码。允许：

- Read / Grep / WebFetch 探索代码 / 文档
- 创建 OpenSpec artifact（proposal、design、specs）
- 修改 docs/ 和 memory/（属于 "capturing thinking"）

**不允许**：
- 写应用代码（`crates/*/src/*.rs`、`apps/admin/src/*.tsx`）
- 改 `Cargo.toml` / `package.json` 这种实现层配置

如果 explore 中用户要求实现代码，提醒退出 explore 后再 apply（或直接 `/opsx:propose` + `/opsx:apply`）。

**相关文件**：用户 global skill `opsx:explore` 描述。
