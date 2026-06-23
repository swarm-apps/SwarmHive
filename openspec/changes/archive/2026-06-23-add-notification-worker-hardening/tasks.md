# Tasks — add-notification-worker-hardening

## 1. 投递事务边界重构(#1)

- [x] 1.1 [code] `notify/worker.rs::deliver_due_batch`:短事务认领一批 due delivery 后 `commit()` 释放行锁,投递循环移出事务。
- [x] 1.2 [code] `deliver_one` 去掉 `db: &C` 参数,改 `&self`:`delivery_request` 走 `&self.db`(只读),外部投递在事务外;成功/失败路径各开独立短事务写结果(`mark_success`/`mark_failure` + `record_attempt` + `update_endpoint_health`)。
- [x] 1.3 [code] 保留前置错误(disabled/secret 坏)只标 failed、不动 endpoint 健康的原语义;每条投递 persist 失败只 `warn!`、不中断整批。
- [x] 1.4 [test] `app_release_smoke`:现有 worker/retries-dead/auto-disable/emit-rollback 测试全绿(行为保持);新增 `notification_worker_processes_mixed_outcomes_in_one_batch`(同批一 204 一 500 → 各自独立落终态;诚实注释为多投递行为测试,非 DB 隔离回归)。

## 2. 索引(#2)

- [x] 2.1 [code] `swarmhive-migration`:新增 `m20260623_000001_notification_indexes`(5 索引,raw `CREATE INDEX IF NOT EXISTS` + `DO/to_regclass` 守卫,`down` 为 `DROP INDEX IF EXISTS`),注册进 `lib.rs::migrations()`。
- [x] 2.2 [code] migration crate lib.rs / 文件 doc 注明「索引例外」(仅 schema-sync 表达不了的二级索引,不含建表/改列)。
- [x] 2.3 [test] `db_smoke::notification_indexes_present_and_migrations_idempotent`:sync + `run_migrations` 后查 `pg_indexes` 断言 5 索引;二次 `run_migrations` 幂等无报错。

## 3. 轮换护栏(#3)

- [x] 3.1 [code] `routes/notifications.rs::rotate_webhook_secret`:`previous_secret_expires_at` 未过期 → 409 Conflict(资源状态冲突),拒绝再次轮换。
- [x] 3.2 [test] `app_release_smoke`:扩展 rotation 测试,断言宽限期内二次轮换返 409 + DB secret 未变。

## 4. Admin 按钮(#4)

- [x] 4.1 [code] `lib/api/notifications.ts`:加 `canRotateSecret(providerKind?)` 纯函数。
- [x] 4.2 [code] `settings/notifications/index.tsx`:用 `canRotateSecret(row.provider_kind)` 条件渲染「轮换密钥」按钮。
- [x] 4.3 [test] `notifications.test.ts`:`canRotateSecret` 对 generic/undefined → true,feishu/slack/dingtalk/discord → false。
- [x] 4.4 [test] `app_release_smoke::notification_rotate_rejects_non_generic_endpoint`:后端对 IM(feishu)endpoint 轮换返 422(#4 后端半,审查 F10)。

## 5. Whitespace(#5)

- [x] 5.1 [chore] 清 `archive/2026-06-22-add-notification-im-providers/design.md:13` 行尾空白。
- [ ] 5.2 [chore] 清 `openspec/specs/*/spec.md` 的 EOF 多空行(归档重写后统一清);`git diff --check main...HEAD` 无输出。

## 6. Docs / 知识库同步

- [x] 6.1 [docs] `docs/15-notifications.md`:补「投递事务边界」+「轮询表索引」两节 + 轮换护栏说明。
- [x] 6.2 [docs] `dev-notes/knowledge/project-notifications.md`:新增「Worker 加固」段。
- [x] 6.3 [docs] `openspec/changes/README.md`:依赖图 + 状态表加本 change。

## 7. Gates + 归档

- [x] 7.1 [test] `cargo fmt --all --check` + `clippy --workspace --all-targets -D warnings` + db_smoke/app_release_smoke 通知 smoke。
- [x] 7.2 [test] admin typecheck + lint + vitest + build;schema.gen.ts 无 diff。
- [x] 7.3 [chore] 对抗式审查(独立 lane,5 维度/19 finding)→ 采纳 6:①错误吞掉改 `?` 传播(系统性 DB 故障浮出 tick 级)②轮换护栏 422→409 Conflict ③轮换测试加 DB-state 未变断言 ④新增 IM rotate→422 后端测试 ⑤批测试改名+诚实注释 ⑥首次轮换 None 放行注释;驳回 1:`>` vs `>=` 边界(护栏与 worker 同用 `>` 本就自洽,改 `>=` 反而不一致)。
- [ ] 7.4 [chore] commit(feat)+ `openspec archive` + commit(chore)。
