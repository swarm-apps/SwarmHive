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
        └─────────────────┬────────────────────┘
                          │
                          ▼
        ┌──────────────────────────────────────┐
        │ add-web-artifact-upload               │  跨 crate：ArtifactsDrawer 浏览器直传
        │ (api-types + server + admin)          │  (hash-wasm + presign PUT + .sig 落库) + CORS 端点
        └──────────────────────────────────────┘     依赖 storage-and-presign-upload / app-release-artifact /
                                                      add-releases-page-ui / add-storage-wizard-page

        ┌──────────────────────────────────────┐
        │ add-cli-device-login                  │  RFC 8628 device flow 替换 ROPC cli-token
        │ (api-types + server + cli + admin)    │  依赖 add-pat-and-api-token + ① login-bootstrap-ui；
        └──────────────────────────────────────┘     旁路 ③ oauth（仅共享 /login 闸门，可任意顺序）

        ┌──────────────────────────────────────┐
        │ add-self-service-account              │  个人账户统一到 /profile + self-service 改名/改密码；
        │ (api-types + server + admin)          │  设置回归组织级 manage 门控
        └──────────────────────────────────────┘     依赖 ① login-bootstrap（密码强度）+ ③ oauth（OAuth-only 设密）+ ④ invite-reset（提升 helper）
```

## 与 docs/09 阶段映射

| 阶段 | proposals |
| --- | --- |
| 0 项目骨架 | `add-toolchain-bump`, `add-crate-restructure` |
| 1 核心模型 + 管理 API | `add-persistence-foundation`, `add-app-release-artifact`（部分） |
| 2 RBAC + 鉴权 | `add-auth-and-rbac`, `add-pat-and-api-token`, `add-login-and-owner-bootstrap-ui`, `add-mail-infrastructure`, `add-oauth-github-and-provider-config`, `add-invite-and-password-reset`, `add-registration-policy-and-self-register`, `add-self-service-account` |
| 3 S3 存储 | `add-storage-and-presign-upload` |
| 4 存储初始化向导 | `add-storage-and-presign-upload`（Admin wizard 部分） |
| 5 CLI 本地发布 | `add-pat-and-api-token`（CLI login 初版）+ `add-cli-device-login`（CLI login 升级为 RFC 8628 device flow，废弃 ROPC）+ `add-storage-and-presign-upload`（CLI publish） |
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

## 当前进度（2026-06-01）

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
| add-oauth-github-and-provider-config | ✅ 归档 `archive/2026-06-01-add-oauth-github-and-provider-config/`（52/57 tasks，剩 5 为手动 GitHub e2e + 前端页面/Playwright 测试 deferred 到 foundation harness）：entity `oauth_provider`（`UNIQUE(kind)`）+ api-types oauth DTO + `auth/oauth/{mod,github}` IdentityProvider trait + GithubProvider（oauth2 5.0 + verified-email-only）+ `routes/{oauth,oauth_providers}` 11 endpoint（flow + CRUD，auth:manage 门控，bootstrap-410）+ `PermissionName::AuthManage` + seed；admin `/login` OAuth 按钮 + `Settings>Authentication` CRUD + `Profile` linked accounts；`oauth_smoke`（wiremock GitHub）6/6 + openapi_surface 全绿。新能力 `oauth-and-provider-config` |
| add-invite-and-password-reset | ✅ 归档 `archive/2026-05-28-add-invite-and-password-reset/`（server `routes::{invite,password_reset,verify_email,users}` + `services::account_token`（argon2+blake3 双层一次性 token）+ 10 endpoints + `dump-openapi` bin；admin SPA 4 公开页 + `/users` + verify banner + 设置账户页；E2E `account_token_smoke.rs` 9/9） |
| add-registration-policy-and-self-register | 📝 proposal/design/specs/tasks 就绪（73 tasks，Phase 4，依赖 ①②③④） |
| add-self-service-account | ✅ 归档 `archive/2026-06-01-add-self-service-account/`（22/22 tasks）：`PATCH /users/me`（改显示名）+ `PUT /users/me/password`（改/设密码，OAuth-only 可设密、改密踢其它 session、仅 cookie 会话重发当前）+ 个人账户合并到 `/profile`（账户信息/安全/登录方式 tab）+ 设置回归组织级 manage 门控 + `MeResponse.has_password`；`upsert_credentials`/`revoke_user_sessions` 提升到 `auth/service.rs`；`account_smoke` 5/5（含 Bearer 无孤儿 session 回归）。新能力 `self-service-account` |
| add-app-release-artifact | ✅ 归档 `archive/2026-05-29-add-app-release-artifact/`（40/40：entity 6 表 + api-types DTO + `routes/{apps,releases}` 18 endpoints 发布列车指针模型 + CLI `apps/releases/artifacts list` + openapi_surface/app_release_smoke 测试全绿；spec → `specs/app-release-artifact/`） |
| add-storage-and-presign-upload | ✅ 归档 `archive/2026-05-29-add-storage-and-presign-upload/`（45/45：entity `storage_backend`/`upload_session` + artifact FK + api-types storage/upload DTO + `storage/{mod,s3}` trait + `routes/{storage,uploads,download}` + S3 原生 checksum presign + Content-MD5 通用闸 + 幂等 complete + 302 下载 + hot-swap backend；CLI `verify/publish/storage` + `swarmhive.toml` + cargo-dist 0.32/release.yml/composite action；openapi_surface + storage_smoke（MinIO）测试全绿；spec → `specs/storage-and-presign-upload/`） |
| add-apps-page-ui | ✅ 归档 `archive/2026-05-29-add-apps-page-ui/`（纯前端：`lib/api/apps.ts` + `usePermissions` helper + 实化 `routes/_auth/apps.tsx` 应用 CRUD + channel 管理；消费既有 app-release-artifact endpoint，零后端改动；typecheck/biome/vitest 全绿，schema.gen.ts 无 diff。页面渲染测试 + e2e deferred 到 foundation test harness——见 admin-spa.md） |
| add-releases-page-ui | ✅ 归档 `archive/2026-05-29-add-releases-page-ui/`（纯前端：`lib/api/releases.ts` + 共享 `errors.ts` + 实化 `routes/_auth/releases.tsx` app 选择器(`?app=`) + 版本生命周期 create/edit/publish/yank + artifacts 只读抽屉 + 发布列车 promote/rollback；消费既有 app-release-artifact endpoint，零后端改动；typecheck/biome/vitest(17) 全绿，schema.gen.ts 无 diff。页面渲染/e2e deferred 到 foundation harness） |
| add-storage-wizard-page | ✅ 归档 `archive/2026-05-29-add-storage-wizard-page/`（纯前端：`lib/api/storage.ts` + 新页 `settings/storage.tsx` backend 列表/建(带 RustFS/OSS 预设)/改(secret 留空保留)/test/activate + 点亮 settings 菜单存储项；消费既有 storage-and-presign-upload endpoint，零后端改动；typecheck/biome/vitest(21) 全绿，schema.gen.ts 无 diff。页面渲染/e2e deferred 到 foundation harness） |
| add-tokens-page-ui | ✅ 归档 `archive/2026-05-29-add-tokens-page-ui/`（纯前端，消费 `add-pat-and-api-token` 既有端点零后端改动：顶层「令牌」页 `routes/_auth/tokens.tsx` + `lib/api/tokens.ts`（列本人 token / 创建 PAT 或 API〔API 勾权限子集 = `ALL_PERMISSIONS.filter(has)`〕/ 明文一次性 `TokenRevealModal` / 撤销 / `tokenStatus` 推导）+ 顶层菜单「令牌」。创建按 `token:manage` 门控；v1 不做管理他人 token、不做 per-app scope。gates 全绿：typecheck / vitest 35（tokens 5）/ biome / admin build（routeTree 重生成）/ lingui extract；schema.gen.ts 无新增 diff。整页渲染/e2e deferred 到 foundation harness。新能力 `tokens-page-ui`） |
| add-web-artifact-upload | ✅ 归档 `archive/2026-05-29-add-web-artifact-upload/`（跨 crate：api-types `CompletePart.signature` + `CorsConfig{Request,Result}`；server `Storage::put_cors` + `POST /storage/backends/:id/cors` + `upsert_artifact` 写 `signature_metadata`；admin `lib/upload/{hash.worker,hash,classify}` hash-wasm+Comlink Worker 流式 hash + `lib/api/uploads` XHR 直传 + ArtifactsDrawer `UploadArtifacts`（拖拽/平台分类/`.sig` 配对/发布+promote）+ storage 页一键 CORS；hash-wasm+comlink 新依赖。gates 全绿：cargo clippy/fmt、storage_smoke 7/7（+signature/+cors 两测）、openapi_surface 5/5、admin typecheck、vitest 30/30（classify 9）、admin build（产出 hash.worker chunk）、schema.gen.ts 重生成。整页渲染/e2e deferred 到 foundation harness。新能力 `web-artifact-upload` + 修改 `storage-and-presign-upload`） |
| add-cli-management-commands | ✅ 归档 `archive/2026-05-29-add-cli-management-commands/`（CLI-only,消费 `add-app-release-artifact` 既有端点零后端/零 api-types 改动:`apps {get,create,update,delete --yes}` + 新 `channels {list,create,set-default,promote,rollback}`（收编并移除 top-level promote/rollback 桩）+ `releases {get,create,update,publish,yank --yes}`；`client.rs` 加 `ApiProblem`/`patch_json`/`delete_no_content`/`post_empty_json`/`emit_one`/`emit_ack`,`main` 改 `dispatch()` + `render_error`（json→stdout 成功 / stderr problem+json / 非零 exit）。gates 全绿:fmt、clippy --workspace --all-targets -D warnings、cargo test --workspace、CLI 单测 5、sea-orm 边界 0；live-server 手动验证 channels/apps get/404 problem 契约通过。CLI-binary e2e deferred（无 harness,bin crate 不可 import；endpoint 由 app_release_smoke 覆盖）。新能力 `cli-management`） |
| add-cli-device-login | ✅ 归档 `archive/2026-06-01-add-cli-device-login/`（47/48 tasks，剩 1 为手动浏览器走查）：RFC 8628 device flow 替换 ROPC `/auth/cli-token`；新 `device_authorization` entity（`UNIQUE(device_code_hash)`）+ `routes/device.rs` 5 endpoint（code/token/lookup/approve/deny，原子 claim）+ api-types `device.rs` + 删 Cli* DTO/`cli_token_smoke` + CLI 重写 `login.rs`（webbrowser 开 /device）+ admin public `routes/device.tsx`；`device_login_smoke` + openapi_surface（`removed_cli_token_endpoint_returns_404`）全绿。新能力 `cli-device-login` |
| add-cli-storage-mail-admin | ✅ 归档 `archive/2026-05-29-add-cli-storage-mail-admin/`（storage CLI `{get,create,update,test,activate,cors}` 零后端改动；**mail DTO 提升到 api-types**（新 `api-types/src/mail.rs` + 3 枚举统一 lowercase〔`MailLogStatus` 一并从 PascalCase 统一,破坏性但用户拍板〕、entity 承双向 `From`、`routes/mail.rs` 改 `api::*` 只留 `LogsQuery`,schema 取舍 A 枚举收紧)；mail CLI `providers{list,create,update,activate,delete --yes,test}` / `templates{list,get,set,preview,restore-defaults}` / `logs` / `status`;密钥三路 `--secret-stdin`>env>明文 flag + update 省略=保留;`client.rs` 加 `put_json`/`resolve_secret`。gates:clippy --workspace -D warnings ✓、admin typecheck ✓(枚举收紧)、schema.gen.ts regen、命令树 --help ✓;全工作区测试进行中。新能力 `storage-cli-admin` + `mail-cli-admin`） |
