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

        ┌──────────────────────────────────────┐
        │ add-cli-publish-polish                │  CLI publish/verify 补齐 spec:--notes-file(changelog)
        │ (cli only,零 server)                  │  + --dry-run + --output json + 实现 init(dialoguer)
        └──────────────────────────────────────┘     依赖 cli-management + cli-storage-mail-admin +
                                                      storage-and-presign-upload（均已归档）

  通知层（横切，依赖 auth-and-rbac + mail-infrastructure + app-release-artifact）：
        ┌──────────────────────────────────────┐
        │ add-notifications                     │  事务性 outbox + email/webhook channel
        │ (api-types + entity + server + docs) │  Standard Webhooks + delivery retry/redelivery
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notifications-page-ui             │  admin SPA `/settings/notifications` 三 tab
        │ (admin only, 零 server)               │  (Endpoints/Subscriptions/Deliveries),消费既有端点
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notifications-cli                  │  swarmhive notifications {endpoints,
        │ (cli only, 零 server / 零 admin)       │  subscriptions,deliveries} 11 子命令 ↔ 11 endpoint
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notification-delivery-payload-log  │  delivery 存请求/响应快照(签名头+body)
        │ (entity+channel+worker+api+admin+cli)  │  + GET /deliveries/{id} 详情 + 行展开懒加载
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notification-secret-rotation-grace │  Standard Webhooks 零停机轮换:旧密钥保留 24h
        │ (entity+channel+worker+api+admin+cli)  │  双签(webhook-signature 多签名头)
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notification-endpoint-auto-disable │  endpoint 连续失败超 3 天自动停用 + UI 重启提示
        │ (entity+worker+api+admin+cli)          │  (failing_since 健康跟踪;Svix/Stripe 范式)
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notification-im-providers          │  飞书/Slack/钉钉/Discord 专用 provider:
        │ (api+entity+providers+channel+worker+  │  平台原生消息体 + 各自加签 + success 判定
        │  routes+admin+cli)                     │  (子调研 4 平台契约;channel 按 provider_kind 分叉)
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notification-delivery-attempts     │  per-attempt 历史时间线:append-only
        │ (entity+api+worker+routes+admin+cli)   │  notification_delivery_attempt 表 + 详情时间线
        └──────────────────┬───────────────────┘
                           ▼
        ┌──────────────────────────────────────┐
        │ add-notification-worker-hardening      │  PR #5 审查加固(无 schema/DTO 改动):
        │ (worker+migration+routes+admin)        │  投递事务边界(短认领→事务外投递→短结果)
        │                                        │  + 5 索引(migration raw CREATE INDEX)
        │                                        │  + 宽限期拒绝再轮换 409 + 非 generic 隐藏轮换钮
        └──────────────────────────────────────┘

  客户端 SDK / 展示层（独立分支，docs/14）：
        ┌─────────────────────┐   ┌─────────────────────────┐   ┌───────────────────────────┐
        │ add-update-sdk-core │ → │ add-registry-web-tauri  │ → │ add-docs-website          │
        │ headless 8 态状态机 │   │ tauriAdapter + 6 UI 组件 │   │ Fumadocs 官网+文档站       │
        └──────────┬──────────┘   └─────────────────────────┘   │ mock live preview(iframe) │
                   │              分发走 GitHub raw             │ → GitHub Pages 子路径站     │
                   │                                            └───────────────────────────┘
                   │                                            文档站只是 registry 展示层，不改分发链路
                   │
                   ▼  RN Android 主线（阶段7，Expo-first；调研拍板 2026-06-04）
        ┌────────────────────────────┐   ┌────────────────────────┐   ┌──────────────────────┐
        │ add-update-check-rn-android│ → │ add-sdk-android-check  │ → │ add-registry-rn      │
        │ server APK 端点 (阻塞项)    │   │ checkUpdateAndroid +   │   │ Expo-only: rnAdapter │
        │ versionCode 闸门/ABI 匹配   │   │ ReleaseInfo.kind? 接缝 │   │ + 6 RN 组件 + 安装器  │
        └────────────────────────────┘   └────────────────────────┘   └──────────────────────┘
                   │ (轻 OTA 接缝: kind 字段 + runtime_version 占位 + 注释，不预选形态)
                   ▼
        ┌────────────────────────────────────────┐
        │ add-ota-provider  (Phase 2 占位，不 apply)│  OTA = provider 扩展层；
        │ docs/11 两候选(自实现 Expo Updates 协议 / │  native 与 OTA 正交不重叠
        │ External Sync)保持开放，不预选            │  (expo-updates 结构性绝不装 APK)
        └────────────────────────────────────────┘

  发布交付（横切，docs/06；与 cli/v* · sdk/v* 三足解耦）：
        ┌────────────────────────────────────────┐
        │ add-server-container-and-release         │  server 单二进制内嵌 admin SPA(rust-embed
        │ (server crate + Dockerfile + CI)         │  embed-spa feature)+ Dockerfile + server/v*
        │                                          │  → GHCR 双架构镜像 + GitHub Release Linux 二进制
        └────────────────────────────────────────┘     依赖 add-admin-frontend-foundation(SPA dist 是嵌入源)
```

## 与 docs/09 阶段映射

| 阶段 | proposals |
| --- | --- |
| 0 项目骨架 | `add-toolchain-bump`, `add-crate-restructure` |
| 1 核心模型 + 管理 API | `add-persistence-foundation`, `add-app-release-artifact`（部分） |
| 2 RBAC + 鉴权 | `add-auth-and-rbac`, `add-pat-and-api-token`, `add-login-and-owner-bootstrap-ui`, `add-mail-infrastructure`, `add-oauth-github-and-provider-config`, `add-invite-and-password-reset`, `add-registration-policy-and-self-register`, `add-self-service-account` |
| 3 S3 存储 | `add-storage-and-presign-upload` |
| 4 存储初始化向导 | `add-storage-and-presign-upload`（Admin wizard 部分） |
| 5 CLI 本地发布 | `add-pat-and-api-token`（CLI login 初版）+ `add-cli-device-login`（CLI login 升级为 RFC 8628 device flow，废弃 ROPC）+ `add-storage-and-presign-upload`（CLI publish）+ `add-cli-publish-polish`（init + publish/verify 补 `--notes-file`/`--dry-run`/`--output json`） |
| 6 Tauri 更新链路 | `add-update-check-tauri` ✅（已 apply 2026-06-03，待 archive）+ `add-update-sdk-core` ✅ + `add-registry-web-tauri` ✅（已 apply 2026-06-03，待 archive）|
| 7 RN Android 链路 | `add-update-check-rn-android` |
| 8 CI/CD | docs/06 工作流，CLI/SDK 复用 cli/v*·sdk/v* 直接 `feat(ci)` 提交；**server 容器/二进制交付**单独立 `add-server-container-and-release`（含 rust-embed 内嵌 SPA 能力） |
| 9 Admin 统计、埋点与通知 | `add-telemetry-events`, `add-openapi-and-admin-client`, `add-admin-frontend-foundation`, `add-notifications` |
| 10 OTA Provider 探索 | 未列入 MVP proposals |

## 推进建议

- toolchain → crate-restructure → persistence → auth 四步是**严格串行**，是后续所有 proposal 的基座。
- oauth-github / pat-and-api-token / mail-infrastructure 可并行（互不冲突）。
- storage-and-presign-upload 必须在 app-release-artifact 落地后才能动，因为它依赖 Release / Artifact 实体。
- update-check-tauri 与 update-check-rn-android 可双线推进。
- openapi-and-admin-client 是横切关注点：建议在每个 proposal 落 handler 时**同步加 utoipa 注解**，不要积压到最后做一次性补齐。
- admin-frontend-foundation 在 add-auth-and-rbac（archived，提供 `/api/v1/auth/me`）+ add-openapi-and-admin-client（archived，提供 `/api/openapi.json` 与 utoipa 注解）之后推进；本 proposal 把 typed admin client 接入也吞下（原 add-openapi-and-admin-client 的 admin 端 Non-goal）。每个后续 Admin business page proposal（apps / releases / tokens / users / storage-config）都依赖它继承 Provider 链 / auth guard / i18n / 主题 / 错误链 / 测试栈。
- **账号 onboarding 五连击**（① login+bootstrap → ② mail → ③ oauth → ④ invite+reset → ⑤ self-register policy）：①② 独立可并行；③ 依赖 ①；④ 依赖 ①②；⑤ 收尾依赖 ①②③④。决策档见 [dev-notes/explore-summaries/2026-05-27-account-onboarding.md](../../dev-notes/explore-summaries/2026-05-27-account-onboarding.md)。
- **客户端 SDK 层**（docs/14 SDK 规划首次落地，架构修订为 1 npm + 2 registry + ports/adapter）：`add-update-sdk-core` ✅（`packages/sdk` headless 核心）→ `add-registry-web-tauri` ✅（tauriAdapter + useUpdate + 6 UI 组件，分发走 **GitHub raw**——`shadcn add` 是开发时操作、项目开源公开，**不做 server `/r` host**）→ `add-docs-website` ✅（已 apply 2026-06-04：Fumadocs 官网+文档站，dogfood mock live preview + iframe 隔离预览，GitHub Pages 子路径站 `swarm-apps.github.io/SwarmHive/`）。**RN Android 主线**（阶段7，Expo-first，调研拍板 2026-06-04）：`add-update-check-rn-android` 📝（server APK 端点，versionCode 整数闸门/ABI 匹配/`android_min_version_code` 强更，apply-ready，**阻塞项**）→ `add-sdk-android-check` ✅ 归档（`checkUpdateAndroid`+`normalizeAndroid`+`ReleaseInfo.kind?` 轻接缝）→ `add-registry-rn` ✅ 归档 `archive/2026-06-20-add-registry-rn`（41/45；2026-06-20 dogfood 把 registry-rn 接入 SwarmDrop-RN 宿主 app〔替换 UpgradeLink〕+ 构建签名 APK + 发布 SwarmHive + checkUpdateAndroid 端点验证确认 §10.1;§10.2–10.5 设备 tap-through deferred-真机回归）；`add-ota-provider` 📝（Phase 2 占位，OTA=provider 扩展层，docs/11 两候选保持开放不预选）。让 SwarmDrop/SwarmNote 从第三方 ToolSetLink 迁到自托管 SwarmHive。

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
| add-registration-policy-and-self-register | 🚧 实施中（2026-06-10 按已 ship 的 ①②③④ 重定基:`Invited`→`Provisioned` 改名 + 一次性 raw 迁移、无 pending_verify/backfill,58 tasks 按支柱 A(policy+OAuth 自助)优先重排;server + admin 双侧已落,集成测试 `{registration_policy,register,approval}_smoke` + `oauth_smoke` 自助注册段全绿;剩 Playwright e2e(待 mailpit/mock-GitHub 基建)与文档收尾） |
| add-self-service-account | ✅ 归档 `archive/2026-06-01-add-self-service-account/`（22/22 tasks）：`PATCH /users/me`（改显示名）+ `PUT /users/me/password`（改/设密码，OAuth-only 可设密、改密踢其它 session、仅 cookie 会话重发当前）+ 个人账户合并到 `/profile`（账户信息/安全/登录方式 tab）+ 设置回归组织级 manage 门控 + `MeResponse.has_password`；`upsert_credentials`/`revoke_user_sessions` 提升到 `auth/service.rs`；`account_smoke` 5/5（含 Bearer 无孤儿 session 回归）。新能力 `self-service-account` |
| add-app-release-artifact | ✅ 归档 `archive/2026-05-29-add-app-release-artifact/`（40/40：entity 6 表 + api-types DTO + `routes/{apps,releases}` 18 endpoints 发布列车指针模型 + CLI `apps/releases/artifacts list` + openapi_surface/app_release_smoke 测试全绿；spec → `specs/app-release-artifact/`） |
| add-notifications | ✅ 归档 `archive/2026-06-22-add-notifications/`（事务性 outbox + `NotificationChannel` email/webhook + Standard Webhooks 签名 + interval worker + notification 管理 API + docs；剩最终 gates / 前端 codegen 同步） |
| add-notifications-page-ui | ✅ 归档 `archive/2026-06-22-add-notifications-page-ui/`（纯前端，消费 `add-notifications` 既有 11 endpoint 零后端改动：`/settings/notifications` 三 tab Endpoints/Subscriptions/Deliveries——webhook endpoint CRUD + Test + 一次性 `whsec_` 轮换/创建 Modal + 订阅 event→channel(email/webhook)→可选 app + 投递日志四态徽章 + endpoint 过滤跳转 + redeliver；IA 拍板见 design.md(email 订阅是一等对象→否决纯 endpoint-中心 master-detail，采 3 平铺 tab + 轻量钻取)。后续 `add-notifications-cli` + 3 backend 增强。整页渲染/e2e deferred 到 foundation harness。新能力 `notifications-page-ui`） |
| add-notifications-cli | ✅ 归档 `archive/2026-06-22-add-notifications-cli/`（纯 CLI，只依赖 api-types 消费 `add-notifications` 既有 11 endpoint，零后端/零前端：`swarmhive notifications {endpoints,subscriptions,deliveries}` 11 子命令 ↔ 11 endpoint，复刻 mail 嵌套子命令 + tokens `emit_ack` 一次性 `whsec_`；endpoint `--endpoint <id|name>` 寻址、`--event/--channel/--status` 走 parse_enum、不引 uuid 直接 dep。gates：cargo build/clippy -D warnings/fmt --check/test(cli 5+api-types 12)/`--help` smoke 全绿。新能力 `notifications-cli`） |
| add-notification-im-providers | ✅ 归档 `archive/2026-06-22-add-notification-im-providers/`（最大的一个,跨 api-types+entity+server〔新 `notify/providers.rs`〕+channel+worker+routes+admin+cli：webhook_endpoint 加 `provider_kind` nullable 列〔None=generic,api 枚举名 `WebhookProviderKind` 避与 mail 重名〕；channel `deliver` 按 provider_kind 分叉——generic 走现有 Standard Webhooks,IM 走 `deliver_im`〔飞书空消息体签名入 body+code==0 / slack 无签名+HTTP200&&"ok" / 钉钉 HMAC 入 query+errcode==0 / discord 无签名+204,消息体 json! 构建〕；secret 语义按 kind〔generic whsec_ / 飞书钉钉用户加签密钥 / slack discord 无〕,rotate 仅 generic；admin provider 下拉+条件 secret+reveal 仅 generic+provider 列,CLI `--provider/--secret`+provider 列。gates：build/clippy -D warnings/fmt；providers 单测 7/notification smoke 9〔新增 feishu 重算 sign 比对 + slack 无签名 blocks〕/openapi_surface 6/db_smoke 4；admin typecheck/lint/build/vitest 52。新能力 `notification-im-providers`） |
| add-notification-endpoint-auto-disable | ✅ 归档 `archive/2026-06-22-add-notification-endpoint-auto-disable/`（跨 entity+worker+api-types+server+admin+cli：webhook_endpoint 加 `failing_since` nullable 列〔schema-sync / 生产 ALTER〕；worker `deliver_one` 落终态后 `update_endpoint_health`〔sent 清 / dead 记起始 + 超 `AUTO_DISABLE_AFTER_DAYS=3` 天自动 disabled,保留 failing_since 作标记〕,`mark_failure` 改返回 bool；update handler re-enable 清 failing_since；view 暴露 failing_since → Admin 红/橙健康标签 + CLI failing-since 列。gates：cargo build/clippy -D warnings/fmt；notification smoke 7〔新增 auto-disable：failing_since 置 4 天前 + 驱动 dead → 自动停用 + re-enable 清空〕/openapi_surface 6/db_smoke 4；admin typecheck/lint/build/vitest 52。新能力 `notification-endpoint-auto-disable`） |
| add-notification-secret-rotation-grace | ✅ 归档 `archive/2026-06-22-add-notification-secret-rotation-grace/`（跨 entity+channel+worker+api-types+server+admin+cli：webhook_endpoint 加 `previous_secret_encrypted`/`previous_secret_expires_at` 两 nullable 列〔schema-sync / 生产 ALTER〕；轮换时旧密钥移入 previous + 宽限 24h；worker 宽限期内解密旧密钥,`deliver_payload` 对同一 body 双签 → `webhook-signature` 头空格分隔多 `v1,`〔Standard Webhooks 零停机〕;view 暴露 `previous_secret_expires_at`〔Admin「轮换中」Tag / CLI rotating-until〕,轮换确认改双签文案。gates：cargo build/clippy -D warnings/fmt；notification smoke 6〔新增 dual-sign 验证：宽限期内新旧两签都验过、过期单签〕/openapi_surface 6/db_smoke 4；admin typecheck/lint/build/vitest 52。新能力 `notification-secret-rotation`） |
| add-notification-delivery-payload-log | ✅ 归档 `archive/2026-06-22-add-notification-delivery-payload-log/`（跨 entity+channel+worker+api-types+server+admin+cli：delivery 加 4 nullable 列存请求/响应快照〔request_body/timestamp/signature + response_body 截断 64KiB〕，schema-sync 加列〔生产 deployer ALTER〕；channel 成功路径补读响应体、捕获签名头，worker 落库；新 `GET /deliveries/{id}` → `DeliveryDetail`；admin Deliveries 行展开懒加载详情〔Request 签名头+body / Response code+body〕；CLI `deliveries get`。gates：cargo build/clippy -D warnings/fmt/notification smoke〔快照断言+详情端点〕/openapi_surface 6/db_smoke 4/admin typecheck+lint+build+vitest 52 全绿，schema.gen.ts 含 DeliveryDetail。新能力 `notification-delivery-log`） |
| add-notification-delivery-attempts | ✅ 归档 `archive/2026-06-22-add-notification-delivery-attempts/`（跨 entity+api-types+worker+routes+admin+cli：新 append-only `notification_delivery_attempt` 表〔delivery_id/attempt_no/四态 status〔复用 DeliveryStatus〕/response_code/请求签名头/截断 response_body/last_error/created_at〕,schema-sync 建表〔生产 deployer CREATE TABLE〕；worker `record_attempt` 在 `mark_success`/`mark_failure` 复用已克隆快照插行,与 delivery 更新同事务〔`deliver_one` 整体 begin/commit 包裹〕,`attempt_no = delivery.attempt + 1` 严格递增；`DeliveryDetail.attempts`〔`serde(default)` 跨版本兜底→openapi optional→前端 `?? []` 守护〕,`get_delivery` 按 attempt_no 升序填充；admin 详情面板「尝试时间线」段〔#序号+四态徽章+码+时间+last_error〕,CLI `deliveries get` 加 attempts 计数列。对抗式审查 5 agent/2 minor〔serde(default) 按全仓约定保留;smoke 中间断言已补〕。gates：cargo build/clippy --all-targets/fmt；notification smoke 10〔retries→dead 逐条校验 attempt_no 1..=5 + 中间 failed/终态 dead〕/openapi_surface 6〔含 DeliveryAttempt〕/db_smoke 4〔新表 schema-sync〕；admin typecheck/lint/build/vitest 52。新能力 `notification-delivery-attempts`） |
| add-notification-worker-hardening | ✅ 归档 `archive/2026-06-23-add-notification-worker-hardening/`（PR #5 外部审查〔gpt-5.5〕加固,无 schema/DTO 改动:**①投递事务边界**—`deliver_due_batch`/`deliver_one` 重构为「短事务认领→提交释放行锁→**HTTP/SMTP 在任何事务外**→每条结果各自短事务」,消除慢 webhook 长占行锁 + 整批回滚重发〔正确性 bug〕;前置错误仍只标 failed 不动健康;残留 crash 窗口靠 webhook-id 去重;单 worker 前提〔run_once interval 串行不重叠〕。**②索引**—5 复合索引〔outbox/delivery×2/subscription/attempt〕走 `swarmhive-migration` raw `CREATE INDEX IF NOT EXISTS`+`to_regclass` 守卫,是 migration crate「只管数据不管 schema」的明确例外〔唯一 dev+prod 都无条件幂等的机制〕。**③轮换护栏**—`previous_secret_expires_at>now` 时 rotate 返 409 Conflict(资源状态冲突),拒绝宽限期内二次轮换〔单 previous slot 覆盖会让早期接收端验签失败;非 generic 另走 422〕。**④Admin 钮**—`canRotateSecret` 非 generic 隐藏轮换钮。**对抗式审查** 5 维度/19 finding→采纳 6〔错误吞掉改 `?` 传播让系统性 DB 故障浮出 tick 级、422→409、测试加 DB 态断言/IM rotate 422 测试/改名诚实化〕,驳回 1〔`>` vs `>=` 边界:护栏与 worker 同用 `>` 本就自洽〕。gates:fmt/clippy --all-targets -D warnings;app_release_smoke 12〔+同批混合结果、+二次轮换 409+secret 未变、+IM rotate 422〕/db_smoke〔+pg_indexes 断言 5 索引 + 二次 run_migrations 幂等〕;admin typecheck/lint/build/vitest〔+canRotateSecret〕;schema.gen.ts 无 diff。修改能力 `notifications`/`notification-secret-rotation`/`notifications-page-ui`） |
| add-dashboard-overview | ✅ 归档 `archive/2026-06-23-add-dashboard-overview/`（api-types + server + admin:首页 `/_auth/index.tsx` 从硬编码 0/PLACEHOLDER 占位改为**全局速览**——新 `GET /api/v1/telemetry/overview?days=N`〔`telemetry:read`;既有 telemetry 端点全 per-app,故加跨 app 聚合〕返回 `app_count`/`release_count`/期内 `update_checks`/`downloads_completed`/按天 trend,**只汇总可加的 event_rollup_day,不碰 device distinct**;`TelemetryOverview`+`OverviewTrendPoint` DTO;admin 接真实数据〔4 卡 + plots Line 双系列趋势 + 7/30/90 Segmented + `enabled: has("telemetry:read")` 降级〕。**顺手修既有潜在 500**:`SUM(bigint)`→numeric 无法 decode 成 i64〔summary/funnel/distribution 有真实数据时都会 500,测试从没用非零 SUM 触发〕,抽 `sum_count_bigint()`=`Func::cast_as(...,bigint)` 4 处统一。gates:fmt/clippy --all-targets -D warnings;telemetry_smoke 5〔+overview SUM-with-data〕/openapi_surface 6〔+overview〕;admin typecheck/lint/vitest 54/build;schema.gen.ts 仅 +TelemetryOverview。新能力 `dashboard-overview`） |
| add-storage-and-presign-upload | ✅ 归档 `archive/2026-05-29-add-storage-and-presign-upload/`（45/45：entity `storage_backend`/`upload_session` + artifact FK + api-types storage/upload DTO + `storage/{mod,s3}` trait + `routes/{storage,uploads,download}` + S3 原生 checksum presign + Content-MD5 通用闸 + 幂等 complete + 302 下载 + hot-swap backend；CLI `verify/publish/storage` + `swarmhive.toml` + cargo-dist 0.32/release.yml/composite action；openapi_surface + storage_smoke（MinIO）测试全绿；spec → `specs/storage-and-presign-upload/`） |
| add-apps-page-ui | ✅ 归档 `archive/2026-05-29-add-apps-page-ui/`（纯前端：`lib/api/apps.ts` + `usePermissions` helper + 实化 `routes/_auth/apps.tsx` 应用 CRUD + channel 管理；消费既有 app-release-artifact endpoint，零后端改动；typecheck/biome/vitest 全绿，schema.gen.ts 无 diff。页面渲染测试 + e2e deferred 到 foundation test harness——见 admin-spa.md） |
| add-releases-page-ui | ✅ 归档 `archive/2026-05-29-add-releases-page-ui/`（纯前端：`lib/api/releases.ts` + 共享 `errors.ts` + 实化 `routes/_auth/releases.tsx` app 选择器(`?app=`) + 版本生命周期 create/edit/publish/yank + artifacts 只读抽屉 + 发布列车 promote/rollback；消费既有 app-release-artifact endpoint，零后端改动；typecheck/biome/vitest(17) 全绿，schema.gen.ts 无 diff。页面渲染/e2e deferred 到 foundation harness） |
| add-storage-wizard-page | ✅ 归档 `archive/2026-05-29-add-storage-wizard-page/`（纯前端：`lib/api/storage.ts` + 新页 `settings/storage.tsx` backend 列表/建(带 RustFS/OSS 预设)/改(secret 留空保留)/test/activate + 点亮 settings 菜单存储项；消费既有 storage-and-presign-upload endpoint，零后端改动；typecheck/biome/vitest(21) 全绿，schema.gen.ts 无 diff。页面渲染/e2e deferred 到 foundation harness） |
| add-tokens-page-ui | ✅ 归档 `archive/2026-05-29-add-tokens-page-ui/`（纯前端，消费 `add-pat-and-api-token` 既有端点零后端改动：顶层「令牌」页 `routes/_auth/tokens.tsx` + `lib/api/tokens.ts`（列本人 token / 创建 PAT 或 API〔API 勾权限子集 = `ALL_PERMISSIONS.filter(has)`〕/ 明文一次性 `TokenRevealModal` / 撤销 / `tokenStatus` 推导）+ 顶层菜单「令牌」。创建按 `token:manage` 门控；v1 不做管理他人 token、不做 per-app scope。gates 全绿：typecheck / vitest 35（tokens 5）/ biome / admin build（routeTree 重生成）/ lingui extract；schema.gen.ts 无新增 diff。整页渲染/e2e deferred 到 foundation harness。新能力 `tokens-page-ui`） |
| add-web-artifact-upload | ✅ 归档 `archive/2026-05-29-add-web-artifact-upload/`（跨 crate：api-types `CompletePart.signature` + `CorsConfig{Request,Result}`；server `Storage::put_cors` + `POST /storage/backends/:id/cors` + `upsert_artifact` 写 `signature_metadata`；admin `lib/upload/{hash.worker,hash,classify}` hash-wasm+Comlink Worker 流式 hash + `lib/api/uploads` XHR 直传 + ArtifactsDrawer `UploadArtifacts`（拖拽/平台分类/`.sig` 配对/发布+promote）+ storage 页一键 CORS；hash-wasm+comlink 新依赖。gates 全绿：cargo clippy/fmt、storage_smoke 7/7（+signature/+cors 两测）、openapi_surface 5/5、admin typecheck、vitest 30/30（classify 9）、admin build（产出 hash.worker chunk）、schema.gen.ts 重生成。整页渲染/e2e deferred 到 foundation harness。新能力 `web-artifact-upload` + 修改 `storage-and-presign-upload`） |
| add-cli-management-commands | ✅ 归档 `archive/2026-05-29-add-cli-management-commands/`（CLI-only,消费 `add-app-release-artifact` 既有端点零后端/零 api-types 改动:`apps {get,create,update,delete --yes}` + 新 `channels {list,create,set-default,promote,rollback}`（收编并移除 top-level promote/rollback 桩）+ `releases {get,create,update,publish,yank --yes}`；`client.rs` 加 `ApiProblem`/`patch_json`/`delete_no_content`/`post_empty_json`/`emit_one`/`emit_ack`,`main` 改 `dispatch()` + `render_error`（json→stdout 成功 / stderr problem+json / 非零 exit）。gates 全绿:fmt、clippy --workspace --all-targets -D warnings、cargo test --workspace、CLI 单测 5、sea-orm 边界 0；live-server 手动验证 channels/apps get/404 problem 契约通过。CLI-binary e2e deferred（无 harness,bin crate 不可 import；endpoint 由 app_release_smoke 覆盖）。新能力 `cli-management`） |
| add-cli-device-login | ✅ 归档 `archive/2026-06-01-add-cli-device-login/`（47/48 tasks，剩 1 为手动浏览器走查）：RFC 8628 device flow 替换 ROPC `/auth/cli-token`；新 `device_authorization` entity（`UNIQUE(device_code_hash)`）+ `routes/device.rs` 5 endpoint（code/token/lookup/approve/deny，原子 claim）+ api-types `device.rs` + 删 Cli* DTO/`cli_token_smoke` + CLI 重写 `login.rs`（webbrowser 开 /device）+ admin public `routes/device.tsx`；`device_login_smoke` + openapi_surface（`removed_cli_token_endpoint_returns_404`）全绿。新能力 `cli-device-login` |
| add-cli-storage-mail-admin | ✅ 归档 `archive/2026-05-29-add-cli-storage-mail-admin/`（storage CLI `{get,create,update,test,activate,cors}` 零后端改动；**mail DTO 提升到 api-types**（新 `api-types/src/mail.rs` + 3 枚举统一 lowercase〔`MailLogStatus` 一并从 PascalCase 统一,破坏性但用户拍板〕、entity 承双向 `From`、`routes/mail.rs` 改 `api::*` 只留 `LogsQuery`,schema 取舍 A 枚举收紧)；mail CLI `providers{list,create,update,activate,delete --yes,test}` / `templates{list,get,set,preview,restore-defaults}` / `logs` / `status`;密钥三路 `--secret-stdin`>env>明文 flag + update 省略=保留;`client.rs` 加 `put_json`/`resolve_secret`。gates:clippy --workspace -D warnings ✓、admin typecheck ✓(枚举收紧)、schema.gen.ts regen、命令树 --help ✓;全工作区测试进行中。新能力 `storage-cli-admin` + `mail-cli-admin`） |
| add-app-detail-page | ✅ 已 apply（前端重构，待归档）：releases 从顶层菜单收进 **App 详情页** `/apps/:slug`（版本/渠道 tab）；列表行「进入」入口 + 详情页头常驻 app 名/局部面包屑 + 编辑删除上移页头 + 渠道合并（channel CRUD + 发布列车 promote/rollback）+ 产物按 platform 分组 + 删顶层 `/releases`。零后端（复用既有 endpoint）。`routeTree.gen.ts` regen 后无残留 `/releases`；typecheck / biome / admin build 全绿。新能力 `app-detail-navigation` + 改 `apps-page-ui` / `releases-page-ui`） |
| add-artifacts-table-and-guided-upload | ✅ 已 apply（前端，待归档）：产物展示 `ArtifactsDrawer` 分组卡片 → **ProTable 扁平表**（platform `rowSpan` 合并 + 架构友好名 + sha256 截断可复制 + 签名 Tag + 展开行）；上传 `UploadArtifacts` 加**引导式**（选平台→架构→传包，按平台切字段：Tauri target+sig / Android abi+apk）+ 保留拖拽批量（共享抽出的 `uploadItems` 链路）；新增 `lib/upload/artifact-display.ts`（`friendlyArch` + `platformRowSpans`）+ 7 单测。零后端。调研定 table（非 matrix）。typecheck / biome / vitest / admin build 全绿。改 `web-artifact-upload` + `releases-page-ui`） |
| add-release-detail-page | ✅ 已 apply（前端，待归档）：产物从「版本列表点『产物』开 `ArtifactsDrawer`」提升为 **release 详情子页** `/apps/:slug/releases/:version`（版本 tab 内），上传从 Drawer 内嵌改为详情页**居中 Modal**。`releases.tsx` → `releases/` 目录：`index.tsx`（列表，「产物」→ navigate）+ `$version.tsx`（详情：`beforeLoad` 404 兜底 + 元信息 Descriptions + 操作 + `ArtifactsTable` + 上传 Modal）+ 非路由 `-shared.tsx`（共享组件，`ArtifactsDrawer` 拆成纯 `ArtifactsTable`）；`route.tsx` 面包屑正则延伸 version 段。零后端（复用既有 endpoint）。typecheck / biome / admin build 全绿，`routeTree.gen.ts` 含 `$version` + 无残留旧单文件路由，build 产出独立 `-shared` chunk。改 `releases-page-ui` + `web-artifact-upload`） |
| add-cli-publish-polish | ✅ 归档 `archive/2026-06-07-add-cli-publish-polish/`（27/27,CLI-only,零 server/entity/schema）：`init` 双模式(dialoguer 交互 / `--yes`·非 TTY flag 驱动,AI/CI 可无人值守)+ `publish --notes-file/--notes`(changelog,新建塞 create、既有走 PATCH)+ `publish --dry-run`(纯本地零网络)+ `publish/verify --output json` + 进度条 TTY/JSON 守卫;**交互统一 dialoguer 移除 rpassword**;action.yml 加 notes-file、docs/12+06 修正真实嵌套 schema。gates 全绿(clippy -D warnings / test 12 / fmt),对抗审查 0 finding。新能力 `cli-project-init` + 改 `storage-and-presign-upload` |
| add-sdk-android-check | ✅ 归档 `archive/2026-06-07-add-sdk-android-check/`（12/12）：SDK `checkUpdateAndroid` + `normalizeAndroid` 消费 server RN Android 端点 + `ReleaseInfo.kind?` OTA 轻接缝;改能力 `update-sdk-core`。代码 2026-06-04 已落地(commit 51e36a1) |
| add-server-container-and-release | ✅ 归档 `archive/2026-06-09-add-server-container-and-release/`（server/v0.1.0 真实 CI 观测通过：GHCR 双架构镜像 + GitHub Release Linux 二进制）：server `embed-spa` feature + `src/spa.rs`（rust-embed 内嵌 `apps/admin/dist` + SPA fallback）让单二进制同服务 `/api` 与 admin 后台；根 `Dockerfile`（node→rust+cmake→debian-slim 多阶段）+ `.dockerignore`；`.github/workflows/server-release.yml`（`server/v*` tag）出 GHCR `linux/amd64+arm64` 镜像 + GitHub Release Linux x86_64/aarch64 二进制。新能力 `server-spa-embedding` |
