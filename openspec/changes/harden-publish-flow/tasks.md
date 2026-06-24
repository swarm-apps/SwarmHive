## 1. Server — artifact 写入原子化(根除并发丢失)

- [x] 1.1 确认线上部署 PostgreSQL 主版本 → **PG 15+**(用户 2026-06-24 拍板):dev DB=PG17.10、生产 Coolify-managed≥15,统一走 `NULLS NOT DISTINCT`。testcontainers 默认 PG11(不支持 NND)随本 change 升到 PG17,让测试覆盖真实约束路径。另两个 open question 同时拍板:publish 默认改 draft 走 **CLI major 版本号**;`swarmhive-action` 升 **v2**。
- [x] 1.2 `swarmhive-migration` 新增 `m20260624_000001_artifact_unique_nulls_not_distinct`:raw SQL DROP 掉 schema-sync 旧的普通唯一索引 `idx-artifact-release_variant`(NULL-distinct,对可空列无效),建 `uq_artifact_release_variant`(`NULLS NOT DISTINCT`),`to_regclass` 守卫。同步去掉 entity `artifact` 的 `#[sea_orm(unique_key="release_variant")]` 注解(否则 dev schema-sync 会重建旧索引 → 两个冲突的唯一索引 → ON CONFLICT 推断歧义)。**为什么走 migration 而非 schema-sync**:① NND 是 schema-sync 表达不了的索引语义;② migration 经 `run_migrations` 无条件执行,不受生产 `auto_sync=false` 影响 —— 这正是线上「约束从未落地」的根因(见对用户洞察的回应)。
- [x] 1.3 `crates/swarmhive-server/src/routes/uploads/service.rs::upsert_artifact` 改为 `artifact::Entity::insert(model).on_conflict(OnConflict::columns([ReleaseId,Platform,Target,Arch,Abi]).update_columns([Filename,SizeBytes,Sha256,StorageBackendId,ObjectKey]) + 带签名时 update SignatureMetadata).exec_without_returning`;`created_at` 显式 `Set`(on_conflict 跳过 before_save)。保留「无签名重传不抹既有签名」语义(signature_metadata 仅在带签名时进 update_columns)。
- [x] 1.4 测试 `same_target_reupload_is_idempotent_upsert`(storage_smoke):同 target(arch/abi=NULL)重传两次 → 仍 1 行、sha 更新为最新 —— 直接验证 NND 索引让 NULL 行也参与冲突收敛。

## 2. Server — 发布与 complete 解耦 + 幂等 finalize 端点

- [x] 2.1 finalize 端点响应复用既有 `api::Release`(与 `publish_release` 一致),不新增冗余 wrapper DTO。`retryable` 落在 CLI 侧 `ApiProblem::retryable()`(从状态码派生,非 wire 字段);`ci-publish` token preset 落在 CLI 侧映射(`tokens.rs::preset_permissions`);均无需 api-types 改动。server 的 `remediation_hint` 作为 wire 字段落在 `error.rs::Problem`(见 3.1)。
- [x] 2.2 新增 `POST /api/v1/apps/{slug}/releases/{version}/finalize` handler + 共享领域函数 `releases::finalize_publish`(发布副作用唯一来源):release 行 `lock_exclusive`(单次、release 级)→ 锁内幂等判定(Published 原样返回 / Yanked 拒绝)→ 校验 artifact ≥ 1 → `mark_published` → emit `ReleasePublished`;返回 `FinalizeOutcome{release, newly_published}` 让调用方据此决定提交后审计。
- [x] 2.3 `uploads.rs::complete` 删除发布副作用(原 count / mark_published / emit 三段)。`complete` 只:校验 part → 原子 upsert artifact → 标记 session 完成 → commit。
- [x] 2.4 **移除** `complete` 内对 release 行的 `lock_exclusive` 临时补丁(由 1.3 原子 upsert + `uq_artifact_release_variant` 唯一索引 + 2.2 finalize 取代);artifact 写入事务不再加任何锁。
- [x] 2.5 并发集成测试(storage_smoke,PG17):`concurrent_multi_target_complete_then_finalize_keeps_all_artifacts`(旧测试迁到新流程:4 target 并发 complete 到 draft + 一次 finalize,断言 4 artifact 全留存、发布一次)+ `finalize_is_idempotent` + `finalize_rejects_release_with_no_artifacts`。**testcontainers 全仓从 PG11 升 PG17**(18 文件 `Postgres::default().with_tag("17-alpine")` + `ImageExt`),否则 NND migration 在每个 boot server 的测试里语法报错。
- [x] 2.6 过渡兼容:server 仍接受旧 `complete(publish=true)`,但标 **DEPRECATED**(api-types 字段 doc + OpenAPI 响应描述 + `tracing::warn`),内部委托给同一条 `finalize_publish`(release 级锁 + 幂等 + 校验 artifact ≥ 1);artifact 先提交故发布失败不回滚已传产物。测试 `deprecated_complete_publish_true_still_publishes_and_keeps_artifacts` 覆盖。待下游升级后移除。

## 3. Server — 403 携带可执行补救提示

- [x] 3.1 `error.rs::Problem` 新增 `remediation_hint`(Option,403 时填),在 `into_response` 集中按 `required_permission` 生成:发布链路权限(`CI_PUBLISH_PERMISSIONS`,含 `release:update`)→「重建带 ci-publish 预设的 token」一行命令,其余 → 找 org 管理员授权。回归 `storage_smoke::developer_cannot_publish_on_complete` 断言 `required_permission` + `remediation_hint` 含 `--preset ci-publish`。

## 4. CLI — publish / finalize / notes / 退出码 / token / init

- [x] 4.1 `commands/publish.rs`:既有 release 用 `get_json_opt` 取既有 notes;notes PATCH 条件化(纯决策 `notes_need_update`:仅 `notes != existing` 才 PATCH)且移到 complete **之后**;新增 `--skip-notes-update`。
- [x] 4.2 `commands/publish.rs`:complete 改 `publish:false` **默认上传到 draft**;新增 `--finalize`(上传后调 finalize 端点);`--channel` 隐含 finalize(草稿不能 promote)。**移除 `--no-publish`**(默认即 draft;随 CLI major 版本号,见 open question 拍板)。dry-run / emit_result 同步反映 draft/finalize。
- [x] 4.3 新增 `swarmhive releases finalize --app <slug> --version <v>` 子命令(归到既有 `releases` 组,非 spec 字面的 `release` 单数 —— 与仓库命名一致),调 2.2 端点。
- [x] 4.4 `client.rs::ApiProblem::retryable()`(408/429/5xx=true;4xx 其余=false);`main.rs::classify_error` 把网络层 reqwest 错(timeout/connect)也判可重试,本地/未知判永久;永久 `exit 2`、可重试 `exit 1`;`GITHUB_ACTIONS=true` 时发 `::error::`(永久)/ `::warning::`(可重试)annotation 并透传 remediation hint。
- [x] 4.5 `commands/tokens.rs`:`--preset ci-publish` 展开为 7 权限集(含 `release:update`);与 `--permissions` 互斥、仅 `--kind api`。
- [x] 4.6 `commands/init.rs`:`--setup-ci-token`——写 `.github/workflows/release.yml` 样板(action v2 + 「N 上传 draft → 1 finalize」+ 版本统一去前导 v)、给出 `tokens create --preset ci-publish` 与 `gh secret set SWARMHIVE_TOKEN` 命令;`--json` 输出 `suggested_token_command`/`github_secret_name`/`suggested_secret_command`/`suggested_workflow_path`/`workflow_created` 且无交互。**保持 init 离线**:不实际调 API 建 token,只给命令(与 init 既有「纯本地不联网」语义一致)。
- [x] 4.7 CLI 单测:notes 条件化决策(`notes_need_update` 未变跳过 / 变化触发 / create·skip·absent 跳过)、退出码分层(`classify_error` 403→2 / 503→1 / 本地→2)、`--preset ci-publish` 含 `release:update` + 互斥校验、`ApiProblem::retryable` + `remediation_hint` 提取。

## 5. swarmhive-action(独立仓库 `/Volumes/yexiyue/swarmhive-action`)

- [x] 5.1 `action.yml` 新增 `artifact-paths` input + 内置 updater-bundle 选取(`scripts/pick-bundles.mjs`,**node** 非 bash,不依赖 `test -f`;平台感知白名单:tauri `.app.tar.gz|.AppImage(.tar.gz)|.nsis.zip|-setup.exe` / android `.apk`,排除 `.deb/.dmg/.msi/.rpm`,windows 优先 `-setup.exe`、linux 优先 `.AppImage.tar.gz`);output `updater`。本地用临时产物树验证选取逻辑通过。
- [x] 5.2 退出码红/绿:`set -uo pipefail`(去 `set -e`)捕获 CLI 退出码 → `exit $code`,任何非零标红(终结 `continue-on-error` 吞错);CLI 已按 exit 2(`::error::`)/ exit 1(`::warning::`)+ 透传 403 remediation hint。output `exit-code`。
- [x] 5.3 `cli-version` 默认 `latest` → 钉 `1.0.0`;Resolve CLI version step 打印 resolved 版本 + output `resolved-cli-version`。
- [x] 5.4 README 重写:Tauri 4-target matrix(版本统一)+ finalize / RN Android 生产样板、CI token 权限清单(首发 vs 重发 + `release:update`)、action↔CLI 版本矩阵、v1→v2 迁移。**升 v2**(open question 拍板);新增 `finalize` input + finalize-only 模式。提交在独立仓库分支 `feat/v2-harden-publish`(`c378999`),发布走自己的 tag。

## 6. 下游 workflow 简化(SwarmDrop / SwarmDrop-RN)

- [x] 6.1 删手写 Pick updater bundle bash(jq/grep/sed/test -f),改用 action v2 `artifact-paths`(给 glob)。
- [x] 6.2 去掉 `continue-on-error: true`,依赖 action 退出码红/绿(权限/配置 exit 2 不再被吞)。
- [x] 6.3 SwarmDrop(多 target):每 target 上传到 draft + 新增 `finalize-swarmhive` job 一次 finalize + promote stable;SwarmDrop-RN(单 target):一步 upload→draft + finalize。各自独立仓库分支 `ci/swarmhive-action-v2`(SwarmDrop `aad0789` / SwarmDrop-RN `512f643`)。SwarmNote / -RN 用 UpgradeLink,不在范围(侦察确认)。

## 7. Docs

- [x] 7.1 `docs/13-rbac.md` 补「CI token 权限要求」(首发 vs 重发 + `release:update` 坑 + `--preset ci-publish`,修正旧推荐权限**漏 `release:update`**);`docs/12-cli.md` 更新上传流程图 / publish 默认 draft + `--finalize` / `releases finalize` / 退出码分层 / `init --setup-ci-token`;`docs/06-cicd.md` action@v2 + finalize 流程;同步 `skills/swarmhive-cli/{SKILL.md,references/command-reference.md}`(默认 draft、finalize、退出码、`--preset`、`--setup-ci-token`、新增 tokens 段)+ action README。
