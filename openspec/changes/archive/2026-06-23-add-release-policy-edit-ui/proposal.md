# add-release-policy-edit-ui

## Why

`release` 实体早有灰度 / 强制更新策略列(`rollout_percent` 灰度放量、`min_version` Tauri 强更下限、`android_min_version_code` RN Android 强更下限),server `UpdateReleaseRequest` + update handler 也**完整支持写入与校验**(`add-update-check-tauri` / `add-update-check-rn-android` 落地)。但 Admin 的 `EditReleaseDrawer` 只暴露 `version`/`android_version_code`/`release_notes`——**灰度发布与强制更新目前只能走 CLI / API**,后台运维看不到也调不了。

## What

- **admin `releases/-shared.tsx`**:`EditReleaseDrawer` 新增 3 个策略字段 + 校验(纯前端,消费既有 `PATCH /apps/:slug/releases/:version`,零后端):
  - `rollout_percent`(`ProFormDigit` 1-100 + rule,预填 `current ?? 100`)。
  - `min_version`(`ProFormText` + semver pattern rule〔允许空〕,预填 current)。
  - `android_min_version_code`(`ProFormDigit` min 1,预填 current)。
  - 新 `EditReleaseValues` 类型(= `CreateReleaseValues` + 3 策略字段);create 抽屉不变。
- **`policyUpdateFields(values, editing)` helper**(`-shared.tsx`,两处 handler 共用,**DRY + 保证两入口一致**):对比**初值**实现直觉化清空,匹配后端单层 Option 语义:
  - `min_version`:非空→该值;清空已设下限→`"0.0.0"`(移除);原本无下限留空→`null`(不改)。
  - `rollout_percent`:<100→设灰度;原有灰度填回 100→`100`(取消);原无灰度且 100→`null`(不改,**避免 NULL→100 漂移**)。
  - `android_min_version_code`:`?? null`。
- **两处 handler 同步**:列表页 `releases/index.tsx::handleEdit` 与详情页 `releases/$version.tsx::handleEdit`(操作此前是复制的)都 `...policyUpdateFields(values, editing)`,catch 改 surface `error.detail`(后端 422 具体字段错误浮出,不再吞成通用文案)。
- **详情页展示**:`$version.tsx` 元信息 Descriptions 增列当前灰度 % / 强更下限,让运维一眼看到当前策略。

## Acceptance

- `pnpm --filter @swarm-hive/admin typecheck` + `lint` + `build`;`schema.gen.ts` 无 diff(零后端 / 零 DTO 改动)。
- `vitest` 全绿(若加纯函数则附单测)。
- 手动:编辑某 release 设 `rollout_percent=50` + `min_version=1.2.0` → 保存 → 详情页展示更新后的策略;`update-check` 端点据此灰度 / 强更(端点行为已由 `update_check_tauri_smoke` / RN smoke 覆盖,本 change 不重测端点)。

## Non-goals

- **零后端 / 零 DTO 改动**——`UpdateReleaseRequest` 与 update handler 已支持全部字段。
- **不**改后端「单层 Option,清空走 sentinel(min_version=0.0.0 / rollout=100)」语义;前端用 tooltip 解释,不引入额外的「清除」按钮或 nullable 三态。
- **不**做独立 `Policies` 页(docs/08 早期设想);策略挂在 release 编辑抽屉里就近编辑,符合「就近操作」。
- 整页渲染测试 deferred 到 foundation harness(admin-spa.md 既有缺口)。

## Depends on

`add-releases-page-ui`(EditReleaseDrawer 宿主)+ `add-update-check-tauri` / `add-update-check-rn-android`(策略列 + update handler 写入校验)——均已归档。

## Maps to docs

- `docs/08-admin-and-analytics.md`(Policies / Releases 段)
- `docs/05-update-protocol.md` 或 `docs/04-platform-support.md`(灰度 / 强更语义,如相关)
