# Tasks: add-artifacts-table-and-guided-upload

纯前端，零后端。纯函数先行（可单测），再改展示与上传组件，最后收尾。

## 1. 纯函数工具 + 单测（先行）

- [x] 1.1 [code] 新增 `apps/admin/src/lib/upload/artifact-display.ts`：`friendlyArch(platform, target, abi)` —— target triple → 友好名，Android 用 abi 原值，未知 triple 回退原值
- [x] 1.2 [code] 同文件 `platformRowSpans(platforms[])`：输入已按 platform 排序，返回每行 rowSpan（段首=段长，其余=0）
- [x] 1.3 [test] `artifact-display.test.ts` 7 个 vitest 单测（友好名映射 / 未知回退 / android abi / rowSpan 段计算）全过

## 2. 产物展示改表格（`ArtifactsDrawer`）

- [x] 2.1 [code] `ArtifactsDrawer` 分组卡片 → **ProTable 扁平表**：平台(`onCell` rowSpan 合并)/ 架构(`friendlyArch` Tag)/ 文件 / 大小(右对齐 tabular-nums)/ sha256(`render` 里 `Typography.Text` copyable+ellipsis.tooltip，避开 #3872)/ 签名(Tag：已签 success / 未签 default)/ 下载(按钮，href `/download/:slug/:ver/:id`)
- [x] 2.2 [code] `expandable.expandedRowRender`：完整 sha256(可复制) + 签名全文(JSON) + 上传时间
- [x] 2.3 [test] `typecheck` PASS + biome 干净（删掉不再用的 `List` import）

## 3. 引导式上传（`UploadArtifacts`）

- [x] 3.1 [code] `mode: guided | batch` 的 `Segmented` 切换（默认 guided）
- [x] 3.2 [code] **guided**：`ProFormSelect` 选平台 + `ProFormDependency` 按平台切字段 —— Tauri 露 `target`(friendly label/triple value) + 安装包 + 可选 `.sig`；Android 露 `abi` + `.apk`（versionCode 是 release 级，提示在版本信息设）。提交构造单个 `StagedItem` → 走既有 `uploadItems`(hash→presign→定长 PUT→complete)
- [x] 3.3 [code] **batch**：保留 `Upload.Dragger multiple` + `classifyArtifact`；与 guided 共享抽出的 `uploadItems`
- [x] 3.4 [test] `typecheck` PASS + biome 干净（`handleUpload` 抽成 `uploadItems(targets)`，两模式复用）

## 4. 收尾 + docs

- [ ] 4.1 [test] 手动验收全部 Acceptance（表格 rowSpan/友好名/sha256/签名/展开 + 引导式双平台 + 批量保留）
- [x] 4.2 [docs] `dev-notes/knowledge/admin-spa.md` 新增「产物表格 + 引导式上传」段（ProTable rowSpan + sha256 规避 #3872 + 引导式 ProFormDependency + uploadItems 复用）
- [x] 4.3 [docs] `openspec/changes/README.md` 进度表加入 `add-artifacts-table-and-guided-upload`
- [x] 4.4 [code] `biome check --write`（干净）+ `lingui:extract`（445 条）+ `admin build`（✓ 1.27s）
