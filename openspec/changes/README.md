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
              └─────────────┬──────────────────┘
                            │
                            ▼
              ┌────────────────────────────────┐
              │ add-admin-frontend-foundation  │  Admin SPA 地基：i18n + 主题 + 错误链 + auth guard + 测试
              └─────────────┬──────────────────┘    （依赖 add-auth-and-rbac + add-openapi-and-admin-client）
                            │
            ┌───────────────┴────────────────┐
            ▼                                ▼
  ┌─────────────────────────┐    ┌──────────────────────────────┐
  │ ① add-login-and-owner-  │    │ ② add-mail-infrastructure    │  独立可并行
  │   bootstrap-ui          │    │                              │
  └─────────┬───────────────┘    └─────────┬────────────────────┘
            │                              │
            ▼                              │
  ┌─────────────────────────────────┐      │
  │ ③ add-oauth-github-and-         │      │
  │   provider-config               │      │
  └─────────┬───────────────────────┘      │
            │                              │
            └──────────────┬───────────────┘
                           ▼
              ┌────────────────────────────────┐
              │ ④ add-invite-and-password-     │
              │   reset (依赖 ①+②)             │
              └─────────────┬──────────────────┘
                            │
                            ▼
              ┌────────────────────────────────────────┐
              │ ⑤ add-registration-policy-and-self-    │
              │   register (依赖 ①+②+③+④)               │
              └─────────────┬──────────────────────────┘
                            │
                            ▼
        ┌──────────────────────────────────────┐
        │ 后续 Admin page proposals             │  apps / releases / tokens / storage-config
        │ (add-apps-page-ui, add-releases-      │  全部 inherit foundation 的 Provider 链 / auth guard /
        │  page-ui, add-storage-wizard-page,    │  i18n / 主题 / 错误链 / 测试栈
        │  ...)                                 │  + ①②③④⑤ 提供的完整账号系统
        └──────────────────────────────────────┘
```

## 与 docs/09 阶段映射

| 阶段 | proposals |
| --- | --- |
| 0 项目骨架 | `add-toolchain-bump`, `add-crate-restructure` |
| 1 核心模型 + 管理 API | `add-persistence-foundation`, `add-app-release-artifact`（部分） |
| 2 RBAC + 鉴权 | `add-auth-and-rbac`, `add-pat-and-api-token`, `add-login-and-owner-bootstrap-ui`, `add-mail-infrastructure`, `add-oauth-github-and-provider-config`, `add-invite-and-password-reset`, `add-registration-policy-and-self-register` |
| 3 S3 存储 | `add-storage-and-presign-upload` |
| 4 存储初始化向导 | `add-storage-and-presign-upload`（Admin wizard 部分） |
| 5 CLI 本地发布 | `add-pat-and-api-token`（CLI login）+ `add-storage-and-presign-upload`（CLI publish） |
| 6 Tauri 更新链路 | `add-update-check-tauri` |
| 7 RN Android 链路 | `add-update-check-rn-android` |
| 8 CI/CD | docs/06 工作流，不单独立 proposal（复用 CLI） |
| 9 Admin 统计与埋点 | `add-telemetry-events`, `add-openapi-and-admin-client`, `add-admin-frontend-foundation` |
| 10 OTA Provider 探索 | 未列入 MVP proposals |

## 推进建议

- toolchain → crate-restructure → persistence → auth 四步是**严格串行**，是后续所有 proposal 的基座。
- oauth-github / pat-and-api-token / mail-infrastructure 可并行（互不冲突）。
- storage-and-presign-upload 必须在 app-release-artifact 落地后才能动，因为它依赖 Release / Artifact 实体。
- update-check-tauri 与 update-check-rn-android 可双线推进。
- openapi-and-admin-client 是横切关注点：建议在每个 proposal 落 handler 时**同步加 utoipa 注解**，不要积压到最后做一次性补齐。
- admin-frontend-foundation 在 add-auth-and-rbac（archived，提供 `/api/v1/auth/me`）+ add-openapi-and-admin-client（archived，提供 `/api/openapi.json` 与 utoipa 注解）之后推进；本 proposal 把 typed admin client 接入也吞下（原 add-openapi-and-admin-client 的 admin 端 Non-goal）。每个后续 Admin business page proposal（apps / releases / tokens / users / storage-config）都依赖它继承 Provider 链 / auth guard / i18n / 主题 / 错误链 / 测试栈。
- **账号 onboarding 五连击**（① login+bootstrap → ② mail → ③ oauth → ④ invite+reset → ⑤ self-register policy）：①② 独立可并行；③ 依赖 ①；④ 依赖 ①②；⑤ 收尾依赖 ①②③④。决策档见 [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md)。

## 当前进度（2026-05-28）

| Proposal | 状态 |
| --- | --- |
| add-toolchain-bump | ✅ 归档 `archive/2026-05-26-add-toolchain-bump/` |
| add-crate-restructure | ✅ 归档 `archive/2026-05-26-add-crate-restructure/` |
| add-persistence-foundation | ✅ 归档 `archive/2026-05-26-add-persistence-foundation/` |
| add-auth-and-rbac | ✅ 归档 `archive/2026-05-26-add-auth-and-rbac/` |
| add-openapi-and-admin-client | ✅ 归档 `archive/2026-05-27-add-openapi-and-admin-client/`（基础设施 + 现有 handler 注解 + admin typed client 接入由 add-admin-frontend-foundation 收尾） |
| add-pat-and-api-token | ✅ 归档 `archive/2026-05-27-add-pat-and-api-token/`（37/37 tasks；CLI auth + Bearer 鉴权链路解锁） |
| add-admin-frontend-foundation | ✅ 归档 `archive/2026-05-27-add-admin-frontend-foundation/`（70/70 tasks；Provider 链 / auth guard / i18n / 主题 / 错误链 / 测试栈 + typed openapi-fetch client） |
| add-login-and-owner-bootstrap-ui | ✅ 归档 `archive/2026-05-28-add-login-and-owner-bootstrap-ui/`（39/39 tasks；e2e 集成测试 deferred 到 CI） |
| add-mail-infrastructure | ✅ 归档 `archive/2026-05-28-add-mail-infrastructure/`（server `mail::{Mailer,SmtpMailer,ConsoleMailer,TemplateEngine,seed}` + `crypto::SecretKey`（AES-256-GCM）+ `/api/v1/mail/*` 12 endpoints + admin SPA `/settings/mail` + mailpit dev seed） |
| add-oauth-github-and-provider-config | 📝 proposal/design/specs/tasks 就绪（57 tasks，Phase 2，依赖 ①），重命名自 add-oauth-github |
| add-invite-and-password-reset | ✅ 归档 `archive/2026-05-28-add-invite-and-password-reset/`（server `routes::{invite,password_reset,verify_email,users}` + `services::account_token`（argon2+blake3 双层一次性 token）+ 10 endpoints + `dump-openapi` bin；admin SPA 4 公开页 + `/users` + verify banner + 设置账户页；E2E `account_token_smoke.rs` 9/9） |
| add-registration-policy-and-self-register | 📝 proposal/design/specs/tasks 就绪（73 tasks，Phase 4，依赖 ①②③④） |
| add-app-release-artifact | 🚧 apply 完成（40/40：entity 6 表 + api-types DTO + `routes/{apps,releases}` 18 endpoints 发布列车指针模型 + CLI `apps/releases/artifacts list` + openapi_surface/app_release_smoke 测试全绿；待归档） |
| add-storage-and-presign-upload | 📝 proposal/design/specs/tasks 就绪（45 tasks；S3 原生 checksum presign + 幂等 complete + 302 下载；依赖 add-app-release-artifact） |
| 其余下游 page proposal | 🚧 等具体 page / storage 启动 |
