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

- [ ] 5.1 `action.yml` 新增 `artifact-paths` input + 内置 updater-bundle 选取(node/python,不依赖文件系统 `test -f`;白名单 `.app.tar.gz|.AppImage|.AppImage.tar.gz|.nsis.zip|-setup.exe`,排除 `.deb/.dmg/.msi/.rpm`,windows 优先 `-setup.exe`);output `updater`
- [ ] 5.2 按 CLI 退出码决定 step 红/绿:`exit 2` 标红(权限/配置),`exit 1` 才走宽松;把 403 的 `remediation_hint` 透传到 `::error::`
- [ ] 5.3 `cli-version` 默认从 `latest` 改为具体稳定版;step 开头打印 resolved 版本 + output `resolved_cli_version`
- [ ] 5.4 README:补「生产就绪」完整样板(Tauri 4 target + RN Android,可 copy-paste)、CI token 权限清单、action↔CLI 版本矩阵;评估是否升 v2 承载破坏性变更

## 6. 下游 workflow 简化(SwarmDrop / SwarmDrop-RN)

- [ ] 6.1 `.github/workflows/release.yml` 删掉手写 Pick updater bundle bash,改用 action 的 `artifact-paths`
- [ ] 6.2 去掉无脑 `continue-on-error: true`,依赖 action 的退出码红/绿语义
- [ ] 6.3 多 target 改为「N 个 target publish 到 draft + 末步一次 `release finalize`」(或一步式过渡用法)

## 7. Docs

- [ ] 7.1 CLI auth / RBAC 文档补「CI token 权限要求」小节:首发 vs 重发的权限差异、明确 `release:update`、`--preset ci-publish` 说明;同步到 action README
