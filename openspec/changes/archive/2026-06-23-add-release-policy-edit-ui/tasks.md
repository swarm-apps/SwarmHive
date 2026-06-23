# Tasks — add-release-policy-edit-ui

## 1. EditReleaseDrawer 加策略字段

- [x] 1.1 [code] `releases/-shared.tsx`:新增 `EditReleaseValues`(= `CreateReleaseValues` + `min_version?`/`rollout_percent?`/`android_min_version_code?`)。
- [x] 1.2 [code] `EditReleaseDrawer` 改 `DrawerForm<EditReleaseValues>`:`initialValues` 预填 3 字段(`rollout_percent: editing.rollout_percent ?? 100` 等),加 3 个 ProForm 字段 + tooltip。

## 2. handler 透传(两处)

- [x] 2.1 [code] `releases/index.tsx::handleEdit` 签名改 `EditReleaseValues`,`updateRelease` body 加 `min_version`/`rollout_percent`/`android_min_version_code`(`?? null` / `|| null`,null=不改)。
- [x] 2.2 [code] `releases/$version.tsx::handleEdit` 同步同改(复制的 handler)。

## 3. 详情页展示当前策略

- [x] 3.1 [code] `releases/$version.tsx` 元信息 Descriptions 增「灰度放量」(`rollout_percent ?? 100`%)+「强更下限」(`min_version` / `android_min_version_code`,无则「无」)。

## 4. Gates + Docs

- [x] 4.1 [test] `pnpm --filter @swarm-hive/admin typecheck` + `lint` + `vitest` + `build`;`schema.gen.ts` 无 diff。
- [x] 4.2 [docs] `docs/08-admin-and-analytics.md` Policies/Releases 段:策略改为「release 编辑抽屉就近编辑」(非独立 Policies 页)。
- [x] 4.3 [docs] `dev-notes/knowledge/admin-spa.md`:EditReleaseDrawer 策略字段 + 清空 sentinel 语义(min_version=0.0.0 / rollout=100)的前端处理范式。
- [x] 4.4 [docs] `openspec/changes/README.md`:状态表加本 change。

## 5. 审查 + 归档

- [x] 5.1 [chore] 对抗式审查(独立 lane,2 维度/17 finding)→ 采纳:① 抽 `policyUpdateFields(values, editing)` helper 对比初值修正清空语义(清空已设下限→0.0.0、原无下限留空→不改;rollout <100 设 / 填回 100 取消 / 无灰度且 100 不改避免漂移)+ DRY 两 handler ② rollout 1-100 rule + min_version semver pattern validator ③ 两处 catch 改 surface `error.detail`。驳回/跳过:android min={0}(versionCode≥1 标准)、版本号 disabled 字段(既有非本 change)、EditReleaseValues 含 version(vestigial 无 `name` 不提交)。
- [ ] 5.2 [chore] commit(feat)+ `openspec archive` + commit(chore)。
