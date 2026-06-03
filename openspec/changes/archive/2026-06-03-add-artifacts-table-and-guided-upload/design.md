## Context

版本 tab（`apps/$slug/releases.tsx`）的 `ArtifactsDrawer`（产物展示）与 `UploadArtifacts`（上传）目前是「按 platform 分组的 Card + List/Descriptions」+「拖多文件 + `classifyArtifact` 文件名分类 + 表格确认」。用户要表格展示 + 平台引导式上传。web 调研定论：产物是明细记录 → table；matrix（平台×架构）有稀疏陷阱 + 长字段塞不进，否决。

约束：AntD 6 + ProTable；artifact 字段 `platform`(`tauri-desktop`/`react-native-android`) / `target` / `arch` / `abi` / `filename` / `size_bytes` / `sha256` / `signature_metadata`；复用既有 hash worker + presign + 定长 PUT + complete 链路（零后端）。

## Goals / Non-Goals

**Goals:** 产物用扁平表 + platform `rowSpan` + 架构友好名 + sha256 截断可复制 + 签名 status Tag + 展开行；上传支持「选平台→选架构→传包」引导式（按平台定制字段）+ 保留拖拽批量。

**Non-Goals:** 不改后端 / 上传链路逻辑 / latest.json / 矩阵视图 / 虚拟滚动 / 分页。

## Decisions

### D1. 展示：扁平表 + 按 platform `rowSpan` 合并首列

每行一个 artifact。`data` 先按 `platform` 稳定排序，预算 `rowSpanMap[index]`（每个 platform 段首行 = 段长度，其余 = 0）。platform 列 `onCell: (_, i) => ({ rowSpan: rowSpanMap[i] ?? 0 })`。

```
平台         架构/目标          文件              大小    签名    操作
桌面     ┐   macOS Apple Silicon swarmdrop.dmg    16 MB  已签✓   ↓ ⋯
         │   Windows x64          swarmdrop.msi    15 MB  已签✓   ↓ ⋯
         ┘   …                                                     
Android      arm64-v8a            app-debug.apk    161MB  未签     ↓ ⋯
  ▸ 展开：完整 sha256(可复制) · minisign 签名全文 · 上传时间 · 下载次数
```

**备选（否决）：** 矩阵（稀疏 + sha256/sig 塞不进单元格）；纯分组 group-header（割裂 size/sha256 同列对比）。3–8 产物规模 rowSpan 是分组视觉与明细可扫描的最优折中。

### D2. 架构友好名：`friendlyArch(platform, target, abi)` 纯函数

新增 `lib/upload/arch-label.ts`（可单测）：target triple → 友好名表
`aarch64-apple-darwin`→「macOS Apple Silicon」、`x86_64-apple-darwin`→「macOS Intel」、`x86_64-pc-windows-msvc`→「Windows x64」、`aarch64-pc-windows-msvc`→「Windows ARM64」、`x86_64-unknown-linux-gnu`→「Linux x64」、`aarch64-unknown-linux-gnu`→「Linux ARM64」；未知 triple 回退原值。Android 直接用 `abi`（`arm64-v8a` 等）。原始 triple 放 tooltip / 展开行。

### D3. sha256 列：避开 ProTable `ellipsis + copyable + render` 失效坑

ProTable 列级 `ellipsis`/`copyable` 与自定义 `render` 同设会失效（pro-components #3872 / #1405）。故 sha256 列**只用 `render`**，内部用 `Typography.Text` 自带能力：

```tsx
render: (_, r) => (
  <Typography.Text
    copyable={{ text: r.sha256 }}
    ellipsis={{ tooltip: r.sha256 }}
    style={{ maxWidth: 160, fontFamily: "monospace" }}
  >
    {r.sha256}
  </Typography.Text>
)
```

### D4. 引导式上传：模式切换 + `ProFormDependency` 按平台切字段

`UploadArtifacts` 加 `mode: "guided" | "batch"`（Segmented 切换，默认 guided）。

- **guided（ProForm）**：`ProFormSelect` 选平台 → `ProFormDependency name={["platform"]}` 监听，按平台渲染：
  - Tauri：`ProFormSelect` 选 target（triple，`options` 用友好名 label / triple value）+ 安装包上传 + 可选 `.sig`（拖/选，或与包同名自动配对）。
  - Android：`ProFormSelect` 选 abi + `ProFormDigit` versionCode + `.apk` 上传。
  - 提交：构造单个 `StagedItem`（platform/target/abi/signature 已由表单显式给出）→ 走既有 `hash → presign → 定长 PUT → complete`。
- **batch（保留现状）**：`Upload.Dragger multiple` + `classifyArtifact` + 表格确认 → 既有链路。

两模式共享底层 `hashFile` / `presignUpload` / `putToStorage` / `completeUpload`，只是 `StagedItem` 的来源不同（表单 vs 文件名分类）。

### D5. 签名 status：`valueEnum` 自动 Badge 色

签名列 dataIndex 派生 `signatureState`（`signature_metadata != null ? "signed" : "unsigned"`），`valueEnum: { signed: {text:"已签", status:"Success"}, unsigned: {text:"未签", status:"Default"} }` —— ProTable 自动出 Badge 色 + 工具栏筛选。

## Risks / Trade-offs

- **[rowSpan 排序错位]** → 合并前必须 stable sort by platform；rowSpanMap 与排序后的 data 索引严格对齐。加单测覆盖 rowSpanMap 计算。
- **[ProTable copyable/ellipsis 坑]** → D3 用 Typography.Text render 规避（已知 issue）。
- **[引导式 + 批量双 UI]** → 共享上传链路、仅 UI 分模式，复杂度可控；用 Segmented 明确切换。
- **[页面级渲染测试缺 harness]**（admin-spa.md）→ 纯函数（arch-label / rowSpanMap）补单测；整表渲染靠 tsc + 手动 + 后续 e2e。

## Migration Plan

1. 新增 `lib/upload/arch-label.ts` + `rowSpan` 计算工具 + 单测（纯函数先行）。
2. 改 `ArtifactsDrawer`：分组卡片 → ProTable（列 + expandable）。
3. 改 `UploadArtifacts`：加 guided/batch Segmented + guided ProForm（ProFormDependency 切平台）；batch 保留。
4. docs / admin-spa.md 同步 + `lingui:extract` + gates。

**回滚：** 前端文件级改动，`git revert` 即可；无数据 / schema / 后端耦合。

## Open Questions

无 —— 展示结构、架构显示、上传交互三个方向已拍板。
