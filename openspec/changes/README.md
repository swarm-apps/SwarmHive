# SwarmHive Changes Index

按依赖与推进顺序排列的 MVP 提案集合。每个目录包含 `proposal.md`（必有）、`design.md`（涉及跨 crate / DB schema 时有）、`tasks.md`（拆好的工作单元）。

## 依赖图

```text
                ┌────────────────────────────┐
                │ add-toolchain-bump         │  Rust 2024 / 1.90 / sea-orm 2
                └─────────────┬──────────────┘
                              │
                              ▼
                ┌────────────────────────────┐
                │ add-crate-restructure      │  4 crate: api-types / entity / server (lib+bin) / cli
                └─────────────┬──────────────┘
                              │
                              ▼
                ┌────────────────────────────┐
                │ add-persistence-foundation │  Postgres + sea-orm + entity 首批
                └─────────────┬──────────────┘
                              │
                              ▼
                ┌────────────────────────────┐
                │ add-auth-and-rbac          │  session + Principal + permission
                └───┬────────────┬───────────┘
                    │            │
                    ▼            ▼
        ┌───────────────┐  ┌───────────────┐
        │ add-oauth-    │  │ add-pat-and-  │  并行
        │ github        │  │ api-token     │
        └───────┬───────┘  └───────┬───────┘
                │                  │
                ▼                  ▼
        ┌────────────────────────────────┐
        │ add-mail-infrastructure        │  邀请 / 密码重置依赖此
        └─────────────┬──────────────────┘
                      │
                      ▼
        ┌────────────────────────────────┐
        │ add-app-release-artifact       │  App / Channel / Release / Artifact
        └─────────────┬──────────────────┘
                      │
                      ▼
        ┌────────────────────────────────┐
        │ add-storage-and-presign-upload │  StorageBackend + presign + complete
        └───┬───────────────────────┬────┘
            │                       │
            ▼                       ▼
   ┌─────────────────────┐  ┌─────────────────────┐
   │ add-update-check-   │  │ add-update-check-   │  可并行
   │ tauri               │  │ rn-android          │
   └─────────────────────┘  └─────────────────────┘
                       │
                       ▼
              ┌────────────────────────────────┐
              │ add-telemetry-events           │
              └─────────────┬──────────────────┘
                            │
                            ▼
              ┌────────────────────────────────┐
              │ add-openapi-and-admin-client   │  贯穿性：随时可加
              └────────────────────────────────┘
```

## 与 docs/09 阶段映射

| 阶段 | proposals |
| --- | --- |
| 0 项目骨架 | `add-toolchain-bump`, `add-crate-restructure` |
| 1 核心模型 + 管理 API | `add-persistence-foundation`, `add-app-release-artifact`（部分） |
| 2 RBAC + 鉴权 | `add-auth-and-rbac`, `add-oauth-github`, `add-pat-and-api-token`, `add-mail-infrastructure` |
| 3 S3 存储 | `add-storage-and-presign-upload` |
| 4 存储初始化向导 | `add-storage-and-presign-upload`（Admin wizard 部分） |
| 5 CLI 本地发布 | `add-pat-and-api-token`（CLI login）+ `add-storage-and-presign-upload`（CLI publish） |
| 6 Tauri 更新链路 | `add-update-check-tauri` |
| 7 RN Android 链路 | `add-update-check-rn-android` |
| 8 CI/CD | docs/06 工作流，不单独立 proposal（复用 CLI） |
| 9 Admin 统计与埋点 | `add-telemetry-events`, `add-openapi-and-admin-client` |
| 10 OTA Provider 探索 | 未列入 MVP proposals |

## 推进建议

- toolchain → crate-restructure → persistence → auth 四步是**严格串行**，是后续所有 proposal 的基座。
- oauth-github / pat-and-api-token / mail-infrastructure 可并行（互不冲突）。
- storage-and-presign-upload 必须在 app-release-artifact 落地后才能动，因为它依赖 Release / Artifact 实体。
- update-check-tauri 与 update-check-rn-android 可双线推进。
- openapi-and-admin-client 是横切关注点：建议在每个 proposal 落 handler 时**同步加 utoipa 注解**，不要积压到最后做一次性补齐。

## 当前进度（2026-05-26）

| Proposal | 状态 |
| --- | --- |
| add-toolchain-bump | ✅ 归档 `archive/2026-05-26-add-toolchain-bump/` |
| add-crate-restructure | ✅ 归档 `archive/2026-05-26-add-crate-restructure/` |
| add-persistence-foundation | ✅ 归档 `archive/2026-05-26-add-persistence-foundation/` |
| add-auth-and-rbac | ✅ 归档 `archive/2026-05-26-add-auth-and-rbac/` |
| add-openapi-and-admin-client | ✅ 进行中（基础设施 + 现有 handler 注解；admin client / CI gate / CLI client 是 Non-goals，留后续 proposal） |
| add-pat-and-api-token | ✅ apply 完成（35/35 tasks，新增 9 集成测试；解锁 CLI auth + Bearer 鉴权链路），待归档 |
| add-oauth-github / add-mail-infrastructure | ⏳ 可与 pat-and-api-token 并行 |
| add-app-release-artifact 等下游 | 🚧 阻塞中 |
