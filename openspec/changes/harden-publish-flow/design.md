## Context

一次真实的多 target 发布事故暴露了「发布」语义在 server / CLI / action 三层的错误分散(详见 proposal)。当前线上已打了**临时悲观锁补丁**:`uploads.rs` 的 `complete` 在事务内对 release 行 `lock_exclusive` 把同 release 的并发 `complete` 串行化——它能止血,但把并发降成串行、且没触及「发布是 complete 副作用」「artifact 写入非原子」「artifact 表无唯一约束」这三个根因。

约束:
- Rust 2024 / sea-orm 2.0-rc / PostgreSQL only;`complete` 走 presign 直传 + 回调(server 不中转字节)
- sea-orm RC 版对 partial / 条件唯一索引的 schema-sync **有已知 bug**(dev-notes 多处佐证),涉及索引特性时须走 `swarmhive-migration` 的 raw SQL 而非 schema-sync
- `swarmhive-action` 是独立仓库(本地 checkout 于 `/Volumes/yexiyue/swarmhive-action`),其 CLI 通过 `npx @swarm-hive/cli` 调用
- 下游(SwarmDrop / SwarmDrop-RN)已有 release.yml 在用旧路径,需保持过渡兼容

## Goals / Non-Goals

Goals: 见 proposal 的 What/Capabilities——把发布收敛成「上传到 draft → 一次 finalize」、artifact 写入原子化、CLI 错误可观测、CI token / 接入易用化。

Non-Goals: 见 proposal 的 Non-goals——不改 RBAC 模型、不实现 OTA、不改 updater 协议、不强制下游立即迁移。

## Decisions

### D1: artifact 写入用 `ON CONFLICT DO UPDATE` 原子 upsert + 唯一索引兜底
当前 `upsert_artifact` 是 SELECT-then-INSERT(两步非原子)。改为 sea-orm `Entity::insert(am).on_conflict(OnConflict::columns([ReleaseId,Platform,Target,Arch,Abi]).update_columns([Filename,SizeBytes,Sha256,StorageBackendId,ObjectKey,SignatureMetadata]))`。前置加 `(release_id,platform,target,arch,abi)` 唯一索引(当前缺失)作为最终兜底。
- **备选**:保留悲观锁(串行)——否决,牺牲并发且治标;应用层串行队列——否决,复杂且仍非 DB 级保证。
- **关键坑**:`arch`/`abi` 可空,Postgres 默认 NULL 互不相等 → `ON CONFLICT` 对 NULL 行不命中、会重复 INSERT 撞约束。**决策**:唯一索引建成 `... NULLS NOT DISTINCT`(PG15+),用 `swarmhive-migration` raw SQL 落地(不走 schema-sync,规避 RC bug)。
- **关键坑**:`ON CONFLICT` 路径不触发 sea-orm `before_save`,`created_at` 必须显式 `Set`(backend.md 已记此坑)。

### D2: 发布与 complete 解耦,新增幂等 `finalize` 端点
`complete` 删除 `req.publish` 分支(count/mark_published/emit),只写 artifact + 标记 session。新增 `POST /releases/{version}/finalize`:加排他锁(仅此一处,且是 release 级单次操作而非 per-target)→ count artifact ≥1 → mark_published(幂等,已发布原样返回 200)→ emit。
- **备选**:complete 保留 publish flag 但加锁(现状补丁)——否决,语义仍是「每个 target 都可能发布」。
- **收益**:配合 D1,多 target 发布从「O(并发数) 抢发布」降为「N 次无副作用上传 + O(1) 幂等 finalize」。**D1+D2 落地后移除 complete 的 `lock_exclusive` 临时补丁**,恢复真并发。

### D3: CLI `publish` 默认上传到 draft,新增 `release finalize`
`publish.rs` 的 `publish = !no_publish`(默认 true)改为默认上传后不发布;新增 `release finalize` 子命令调 D2 端点。
- **取舍**:这是 CLI 行为变更。过渡策略:保留显式发布选项(如 `publish --finalize` 一步上传并发布,内部 = 上传 + finalize),让单 target 用户无感;多 target CI 用「N 个 publish + 1 个 finalize」。版本上随 CLI 次版本号发布并在 README/CHANGELOG 标注。

### D4: notes PATCH 条件化 + 后置
`post_ensure` 返回既有 release 的 `release_notes`;仅当 `notes.is_some() && notes != existing` 才 PATCH,且把 PATCH 移到 artifact 上传**之后**。加 `--skip-notes-update`。
- **收益**:重发/补传 notes 未变时完全不碰 `release:update`;即便 token 缺该权限,artifact 也已先传成功。这直接消除本次「在上传前撞 403 → 0 产物」的失败模式。

### D5: CLI 退出码分层 + action 据此红/绿
`ApiProblem` 加 `retryable`(2xx=success;408/429/5xx/超时=true;401/403/409/422=false)。`main.rs` 永久错误 `exit 2` + `::error::`,可重试 `exit 1` + `::warning::`。action 捕获退出码:`exit 2` 直接标红;`exit 1` 才走宽松。
- **收益**:终结「CI 全绿 + artifact 丢失」——本次最难排查的组合。下游可去掉无脑 `continue-on-error: true`。

### D6: `tokens create --preset ci-publish`(CLI 端展开)
`tokens.rs` 加 `--preset`,在 CLI 端映射为 `app:read,release:read,release:create,release:update,release:publish,release:promote,artifact:upload`(关键是内置本次缺的 `release:update`)。
- **备选**:server 端预设 role——否决,改 RBAC 模型超出本变更范围;CLI 端映射最小且够用。

### D7: action 内置 updater-bundle 选取 + 钉稳定 CLI 版本
`action.yml` 新增 `artifact-paths` input + 一段 **node/python**(非 bash,规避 windows `D:/` 盘符 `test -f` 不可靠)的选取脚本:清单内匹配同名 `.sig`、白名单只保留 `.app.tar.gz|.AppImage|.AppImage.tar.gz|.nsis.zip|-setup.exe`、显式排除 `.deb/.dmg/.msi/.rpm`、windows 优先 `-setup.exe`。`cli-version` 默认从 `latest` 改为具体稳定版 + 打印 resolved 版本。
- **收益**:用户删掉 ~30 行手写 Pick bash;根除本次两坑(D:/ 路径选空、`.deb` 也被签名导致误选安装包)。

### D8: 403 problem 携带 `required_permission` + remediation hint
`error.rs` 的 Forbidden 已部分有 `required_permission`,补 `remediation_hint`;CLI 打印 403 时追加一行可复制命令(`swarmhive tokens create --kind api --preset ci-publish`);action 透传到 `::error::`。

## Risks / Trade-offs

- [部署 Postgres < 15,无 `NULLS NOT DISTINCT`] → 退化方案:arch/abi 用 sentinel 空串代替 NULL,或对「无 arch/abi」与「有 arch/abi」分别建 partial unique index;迁移前先确认线上 PG 版本。
- [先移除悲观锁、后建索引会出现窗口期竞态] → **顺序硬约束**:必须「先建唯一索引 + upsert 改 ON CONFLICT」就绪,**再**移除 `lock_exclusive`;并发回归测试通过才合并。
- [`publish` 默认改 draft 是行为变更] → 过渡:保留一步式 `publish --finalize`;CHANGELOG / action README 标注;下游 workflow 迁移到「N publish + 1 finalize」。
- [finalize 是新端点 + CLI 子命令,旧 action 不会用] → 过渡期 server **同时**接受旧 `complete(publish=true)`(标记 deprecated)与新 finalize,直到下游升级。
- [跨仓库:action 改动在独立仓库] → 本 change 的 tasks 显式包含 `/Volumes/yexiyue/swarmhive-action` 的改动,但其发布走 action 自己的 tag。

## Migration Plan

1. `swarmhive-migration` 加 artifact `(release_id,platform,target,arch,abi)` 唯一索引(raw SQL,`NULLS NOT DISTINCT`)。
2. `upsert_artifact` 改 `ON CONFLICT DO UPDATE`(显式 `created_at`)。
3. 新增 `finalize` 端点;`complete` 删除 publish 副作用;**移除** `lock_exclusive` 补丁;加并发集成测试(N target 并发 complete + 一次 finalize,断言 N 个 artifact 全留存、发布一次)。
4. `error.rs` Forbidden 加 remediation hint;`api-types` 加 finalize DTO / `retryable` / preset。
5. CLI:`release finalize` 子命令、`publish` 默认 draft(+ `--finalize` 兼容)、notes 条件化后置、退出码分层、`tokens create --preset ci-publish`、`init --setup-ci-token`。
6. action:`artifact-paths` + 内置选取、退出码红绿、`cli-version` 钉稳定版、README 生产样板 + 权限清单 + 版本矩阵。
7. 下游 SwarmDrop / SwarmDrop-RN 的 release.yml 切到新 action 用法(删手写 Pick、去 continue-on-error、改 N publish + finalize)。
- **Rollback**:任一步出问题可回退该步;server 过渡期双轨(旧 complete(publish) + 新 finalize)保证下游不被强制升级。

## Open Questions

- 线上部署的 PostgreSQL 主版本是否 ≥ 15(决定唯一索引用 `NULLS NOT DISTINCT` 还是 sentinel/partial index)?
- `publish` 默认改 draft 是否需要 CLI 主版本号(破坏性变更)?过渡期保留 `--finalize` 多久?
- `swarmhive-action` 是否升 v2 承载「内置 Pick + 默认 CLI 版本变更 + 退出码语义」这组破坏性变更?
