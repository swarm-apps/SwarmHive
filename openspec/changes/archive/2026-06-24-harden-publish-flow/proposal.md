## Why

SwarmHive 的「发布」语义在 server / CLI / action 三层被错误地分散，导致一次真实的多 target 发布事故：四个平台的 CI job 各自报成功、线上 release 却只剩一个 artifact、CI 全绿——最难排查的组合。根因不是单个 bug：server 把「发布」当成每个 target `complete(publish=true)` 的副作用，且 artifact 写入是非原子 SELECT-then-INSERT（artifact 表连唯一约束都没有）；CLI 把「改 notes」与「传 artifact」绑死、notes 的 PATCH 发生在上传之前、又用单一 `exit(1)` 抹平所有失败类型；action 把「挑 updater bundle / 错误是否致命 / 要不要并发发布」全甩给用户在 release.yml 里手写 bash，再叠一层 `continue-on-error` 把 CLI 的 exit 1 吞成绿。

本变更系统性收敛发布语义（发布从 per-target 副作用 → 一次显式 finalize；artifact 写入原子化），并把接入复杂度从「~60 行易错 YAML + 手挑 7 个权限且极易漏 `release:update`」降到「一条 `init` 命令 + <20 行声明式 workflow」。依据 docs/03（发布与 CI/CD）、docs/13（CLI）。

## What Changes

**Server**
- artifact 写入改为数据库原子 upsert（`INSERT ... ON CONFLICT (release_id,platform,target,arch,abi) DO UPDATE`），并新增该元组的唯一索引 migration（**当前缺失**）作为并发兜底
- 把「发布」从 `complete(publish=true)` 的 per-target 副作用解耦成显式幂等端点 `POST /api/v1/apps/{slug}/releases/{version}/finalize`；`complete` 只负责写 artifact + 标记 upload session
- `403 Forbidden` 的 problem+json 携带 `required_permission` + 可执行补救提示（remediation hint）
- **移除**本次事故的临时悲观锁补丁（`complete` 内对 release 行 `lock_exclusive`），由原子 upsert + finalize 取代

**CLI**
- `publish` 的 notes PATCH 条件化（仅当 notes 实际变化才 PATCH）+ 移到 artifact 上传之后 + 新增 `--skip-notes-update`
- `publish` 默认只上传到 draft（不发布），新增 `release finalize` 子命令；多 target 流程变为「N 个 target 各自上传 → 最后一次 finalize」
- 退出码分层：永久错误（权限/配置）`exit 2`、可重试（网络/宕机）`exit 1`；`ApiProblem` 增加 `retryable`
- `tokens create --preset ci-publish`：一条命令展开为含 `release:update` 的完整发布权限集
- `init --setup-ci-token`：引导创建 CI token + 打印 `gh secret set SWARMHIVE_TOKEN` + 生成可 copy-paste 的 release.yml 样板

**swarmhive-action**（配套，仓库 swarm-apps/swarmhive-action，本地 checkout 于 `/Volumes/yexiyue/swarmhive-action`）
- 内置 updater-bundle 选取：用户只传 `artifact-paths`，action 跨平台可靠选取（不依赖文件系统 `test -f`，白名单只保留真 updater bundle `.app.tar.gz/.AppImage/.nsis.zip/-setup.exe`，显式排除安装包 `.deb/.dmg/.msi/.rpm`，windows 优先 `-setup.exe`）
- 按 CLI 退出码决定 step 红/绿，终结 `continue-on-error` 吞掉权限错误
- `cli-version` 默认钉稳定版（不再用会无声滑动的 `latest`）+ 日志打印 resolved 版本
- README 补「生产就绪」完整样板（Tauri 4 target + RN Android）、版本矩阵、CI token 权限清单

**Docs**
- CLI auth / RBAC 文档补「CI token 权限要求」：首发 vs 重发的权限差异，明确 `release:update`

## Capabilities

### New Capabilities
<!-- 本次无全新 capability;均为现有能力的需求修正(finalize 端点作为 storage-and-presign-upload 的需求新增建模)。 -->

### Modified Capabilities
- `app-release-artifact`: artifact 写入并发安全(原子 upsert + `(release_id,platform,target,arch,abi)` 唯一约束);发布语义从 per-target `complete` 副作用收敛为显式 finalize
- `storage-and-presign-upload`: `complete` 不再触发发布;新增幂等的 release finalize 端点;多 target 上传与发布解耦;403 携带可执行补救提示
- `cli-management`: `publish` notes 条件化 + 上传后 PATCH;`publish` 默认 draft + 新增 `release finalize` 子命令;退出码分层(永久 vs 可重试);`tokens create --preset ci-publish`
- `cli-project-init`: `init --setup-ci-token` 引导建 CI token + 生成 workflow 样板

## Impact

- **Server**: `crates/swarmhive-server/src/routes/uploads.rs`、`uploads/service.rs`、`releases.rs`、`error.rs`;新增 artifact 唯一索引 migration;移除 `lock_exclusive` 临时补丁;新增并发集成测试(N target 同时 finalize 断言 artifact 全留存)
- **CLI**: `crates/swarmhive-cli/src/commands/{publish,tokens,init}.rs`、`client.rs`、`main.rs`
- **共享类型**: `crates/swarmhive-api-types`(finalize 请求/响应、`retryable`、token preset)
- **跨仓库**(swarm-apps/swarmhive-action,本地 `/Volumes/yexiyue/swarmhive-action`): `action.yml` + `README.md`
- **下游用户 workflow**: SwarmDrop / SwarmDrop-RN 的 `.github/workflows/release.yml` 可删手写 Pick bash、去 `continue-on-error`、大幅简化
- **Docs**: CLI auth / RBAC 的 CI token 权限清单

## Non-goals

- 不改多租户 / RBAC 模型本身(仅补「CI 发布」预设权限集 + 文档)
- 不实现 OTA provider(仍是扩展层,MVP 不做)
- 不改 update-check(updater)协议或客户端 SDK
- 不强制下游用户立即迁移:旧的 per-target `publish=true` 路径保持兼容,新的「上传到 draft + finalize」是更安全的推荐路径
- 不引入新存储后端 / 不改 presign 直传机制本身
