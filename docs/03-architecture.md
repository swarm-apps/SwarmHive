# 系统架构

## 总览

SwarmHive 采用控制面与文件分发分离的架构。

```text
Local CLI / CI/CD / Admin
        |
        | publish artifacts / manage releases / configure storage
        v
SwarmHive Server  -- metadata / policy / analytics / telemetry
        |
        | S3 API / presigned URL / public object URL
        v
S3-compatible Object Storage
        |
        | examples
        v
Bundled RustFS / Aliyun OSS / R2 / AWS S3 / MinIO / Garage

Tauri / React Native App
        |
        | check update / report update events
        v
SwarmHive Server
```

服务器负责版本判断、策略计算、埋点采集、统计和后台 API。发布产物统一保存到 S3-compatible object storage。单服务器部署时，SwarmHive 通过官方 Docker Compose profile 或 CLI 引导用户启动 RustFS，RustFS 仍然通过 S3 API 接入。

OTA 能力作为 provider 扩展层接入，不改变 SwarmHive Core 的基础模型。

## 分层

### SwarmHive Core

核心控制面，负责组织、用户、角色、权限、应用、版本、channel、artifact、storage、策略、统计和 API Token。

### Update Providers

不同更新机制的协议适配层：

- Tauri full app update。
- React Native Android APK update。
- Expo Updates OTA（后续）。
- CodePush-compatible OTA（后续）。

### Storage Providers

SwarmHive Core 只维护一套 S3-compatible storage backend。

可接入：

- Bundled RustFS：官方 single-server 模式推荐。
- Aliyun OSS：国内云存储推荐。
- Cloudflare R2。
- AWS S3。
- MinIO。
- Garage。
- 其他兼容 S3 核心 API 的对象存储。

### Entrypoints

SwarmHive 有三个主要入口：

- CLI：本地发布、CI/CD 发布、校验、promote、rollback、存储初始化指引。
- Admin：查看、配置、管理、存储向导和排障。
- SDK / Client API：客户端检查更新和事件上报。

## 组件

### swarmhive-server

Rust + Axum 服务，负责：

- 更新检查 API。
- 管理 API。
- 下载入口与重定向。
- 下载事件记录。
- SDK 埋点上报。
- S3-compatible storage 适配。
- 存储连接测试和健康检查。
- 用户会话鉴权。
- RBAC 权限检查。
- Scoped API Token 鉴权。
- Provider 路由与扩展点。

### Database

第一阶段统一使用 PostgreSQL 作为唯一数据库后端，不再保留 SQLite 路径：

- dev 环境复用现有 coolify-managed Postgres。
- single-server bundled 部署通过 Docker Compose profile 同机起 Postgres。
- 不为 SQLite 做兼容代码，可放心使用 Postgres 特性（JSONB、ILIKE、`ON CONFLICT DO UPDATE`、partial index、`SKIP LOCKED` 等）。

核心实体：

**已落地（add-persistence-foundation, 2026-05-25）**：

- Organization：组织。MVP 只有默认组织（slug = "default"），预留未来多租户边界。
- User：用户（id / org_id / email / display_name / status: active|disabled|invited / 时间戳）。
- IdentityLink：身份来源，`(provider, subject)` UNIQUE。provider ∈ {password, github}；扩 Google/GitLab 只加 provider。
- Role：角色（5 个内建：owner / admin / release-manager / developer / viewer）。
- Permission：权限（21 个 verb-scoped，如 `release:publish` / `storage:manage`）。
- RolePermission：role ↔ permission 复合 PK junction。
- UserRole：用户与角色绑定，`scope_app_id: Option<Uuid>`（NULL = org-level）。
- Session：tower-sessions 后端表。
- AuditLog：关键操作审计日志，`metadata` 为 JSONB。

**已落地（add-app-release-artifact, 2026-05-28）**：

- App：应用。`(org_id, slug)` 唯一，slug 不可变；创建时同事务 seed dev/beta/stable 通道。
- Channel：发布通道（dev/beta/stable）。**命名指针**模型——channel 本身不持版本，当前服务的 release 存在 `channel_release`（channel_id 为 PK，每 channel 至多一行），promote/rollback 只移指针并 append `channel_release_history`，**永不删 release**。
- Release：版本。`(app_id, version)` 唯一，channel 无关；`status` draft/published/yanked；`android_version_code` 供 RN 单调比较。
- Artifact：平台产物。元数据实体已落地（只读）；字节上传 / 创建在 `add-storage-and-presign-upload`。

**待落地**：

- StorageBackend：S3-compatible 存储配置。
- UpdateEvent：更新链路埋点事件。
- DownloadEvent：下载统计事件。
- ProviderConfig：不同 update provider 的配置。
- ApiToken：CI/CD 与客户端访问凭证，支持 app / channel / permission scope。
- AuditLog：关键操作审计日志。

### Storage

存储后端抽象为统一 S3-compatible 接口：

- put object。
- get public URL 或 signed URL。
- delete object。
- check object metadata。
- test upload / test download。

SwarmHive 不提供 local filesystem 作为正式 storage backend。上传中转目录只用于临时缓存，不作为产物最终存储。

### swarmhive-cli

CLI 是本地发布和 CI/CD 的主要入口，负责：

- 登录或读取 API Token。
- 初始化 `swarmhive.toml`。
- 扫描构建产物。
- 校验版本和签名。
- 上传产物并显示进度。
- 创建或更新 release。
- promote / rollback channel。
- 输出 bundled RustFS 部署命令或执行本机部署向导。

### swarmhive-admin

Web 后台用于人工管理：

- 查看应用、版本、产物。
- 配置更新策略。
- 初始化和配置 S3-compatible storage。
- 选择 single-server RustFS / existing S3 / Aliyun OSS。
- 查看下载统计和更新漏斗。
- 管理 API Token。

技术栈：

- Vite + React + TypeScript。
- TanStack Router 提供 file-based 路由与类型安全导航（含 `_auth` pathless layout + `beforeLoad` 鉴权 guard）。
- TanStack Query 管理服务端状态与缓存失效。
- Ant Design 6 + Pro Components（ProTable / ProForm / ProLayout）作为后台 UI 体系，`ConfigProvider` 注入 `locale={zhCN}` + `theme.algorithm` 跟 `useColorMode()` 联动（light/dark/system）。
- @ant-design/charts 渲染 Dashboard 趋势与更新漏斗。
- i18n: Lingui v6（zh-CN MVP，代码 `<Trans>` / `useLingui()` 全包裹，i18n-ready）。
- API client: server 暴露 `/api/openapi.json`（utoipa）→ admin 用 `openapi-typescript` 生成 `schema.gen.ts` types → `openapi-fetch` + `openapi-react-query` 提供类型安全 `$api`，middleware 把 RFC 9457 `application/problem+json` 转 `ApiError` 抛出。CI drift gate `git diff --exit-code` 保护 schema 同步。
- 测试: Vitest unit（jsdom + @testing-library/react）+ Playwright E2E（chromium 单浏览器；global-setup 用 testcontainers Postgres 或 CI services postgres + spawn server binary）。
- 本地 state 不引入 Zustand/Jotai/Redux：URL 状态走 Router search params + zod，跨组件用 Context，服务端走 TanStack Query。
- 通过 `rust-embed` 将构建产物嵌入 server binary，Axum 负责 SPA fallback 与静态服务，部署仍保持单 binary。

## 存储初始化流程

1. 用户启动 SwarmHive Server 和 Admin。
2. Admin 检测到未配置 storage，进入初始化向导。
3. 用户选择一种模式：
   - Existing S3-compatible storage。
   - Aliyun OSS preset。
   - Single-server bundled RustFS。
4. 如果选择 RustFS，Admin 展示官方 Docker Compose profile 或 CLI 命令。
5. 用户启动 RustFS 后，Admin 检测 endpoint 健康状态。
6. Admin 测试创建 bucket / 上传 / 下载。
7. 保存 StorageBackend 配置。
8. 后续 CLI、CI/CD、Admin 上传统一走 S3-compatible backend。

## 更新检查流程

### Tauri

1. 客户端调用 Tauri updater endpoint `GET /api/v1/updates/tauri/:app_slug?current_version&target&arch&channel?&client_id?`。
2. Server 识别 app、current_version、target、arch、channel（updater 注入的是 OS 名 + arch 分离两段，server 解析 artifact 的 Rust target triple 做匹配）。
3. Server 记录 `update_check`。
4. Server 查询对应 channel（缺省取 `is_default`）当前指向的 release，必须 published。
5. Server 判断是否需要更新（semver）、是否强制（`min_version`）、是否在灰度桶（`rollout_percent`）。
6. 如有更新，记录 `update_available`。
7. Server 返回 Tauri updater 兼容 flat JSON（有更新 `200` / 无更新 `204 No Content`）。
8. 客户端使用 Tauri updater 下载并验证签名。

### React Native Android

1. RN SDK 调用 SwarmHive update check API。
2. Server 接收 app、versionName、versionCode、channel、device 信息。
3. Server 记录 `update_check`。
4. Server 判断是否存在更新和策略类型。
5. Server 返回 APK 下载地址、更新日志、策略。
6. RN SDK 下载 APK，展示进度并跳转 PackageInstaller。
7. RN SDK 可上报下载完成、安装器跳转、新版本启动等事件。

## 发布流程

### 本地 CLI 发布

1. 用户本地构建 Tauri 或 RN 产物。
2. `swarmhive verify` 校验版本、签名和平台信息。
3. `swarmhive publish` 上传产物和 metadata。
4. Server 将文件保存到 S3-compatible storage。
5. Server 创建 release 和 artifact 记录。
6. 客户端下一次检查更新时拿到新版本。

### CI/CD 发布

CI/CD 使用同一套 CLI 或官方 GitHub Action：

1. CI 构建 Tauri 或 RN 产物。
2. Action 调用 `swarmhive-cli verify`。
3. Action 调用 `swarmhive-cli publish`。
4. 可选执行 `promote` 将 beta 提升到 stable。

## 部署方式

### Single-server bundled RustFS

适合个人项目、小团队和私有部署。

- 一台服务器通过 Docker Compose 同机起：swarmhive-server（嵌入 admin SPA）+ Postgres + RustFS + nginx/caddy。官方镜像 `ghcr.io/swarm-apps/swarmhive-server`（`linux/amd64` + `linux/arm64`，`server/v*` tag 触发 `server-release.yml` 构建，`--features embed-spa` 内嵌 SPA）；复制即用的生产示例见仓库 `deploy/docker-compose.yml`。不想用容器也有 GitHub Release 上的 Linux 单文件二进制。
- Postgres 保存元数据、会话、审计、更新事件等所有结构化数据。
- RustFS 保存产物，通过 S3 API 暴露给 server。
- Nginx / Caddy 负责 HTTPS 反向代理。

### Existing S3-compatible storage

适合已有对象存储的用户。

- SwarmHive Server / Admin 独立部署。
- 产物保存到已有 RustFS、MinIO、Garage、R2、AWS S3 等。

### Aliyun OSS

适合国内公开分发。

- SwarmHive 连接阿里云 OSS 的 S3-compatible endpoint。
- 下载 URL 可走 OSS 自身域名或绑定 CDN 域名。

后续可演进为：

- 多实例 server（Postgres 已支持，需补 session 共享与 leader-elected 后台任务）。
- 独立后台前端（解开 rust-embed，前后端独立部署）。
- 异步任务队列处理统计聚合（in-process tokio-cron-scheduler → apalis + Postgres `SKIP LOCKED`）。
- OTA provider 单独拆包或插件化。

## OpenAPI 暴露面

Server 用 `utoipa` 系列在编译期收集所有 handler 的请求/响应类型，生成机器可读的 OpenAPI 3.1 文档。Admin SPA、CLI 和外部 onboarding 都通过它消费 API 契约。

**Endpoint**：

- `GET /api/openapi.json` —— 完整 OpenAPI 3.1 JSON 文档（公开，无 auth）。
- `GET /api/docs` —— Redoc UI（公开，无 auth）。

**为什么公开**：SwarmHive 是 self-hosted 内部部署，OpenAPI doc 列的是 path / schema / 错误码，不含敏感数据（类比 GitHub 公开 swagger）。Admin SPA 在 CI 中跑客户端类型生成时无需先 login 拿 cookie，dev 时反复刷 Redoc 也不会被 rate-limit 拦截（这两个 endpoint 装在 root，session_layer 与 tower-governor 都管不到它们）。

**Tag 划分**（drives Redoc 左侧导航）：

| Tag | endpoint |
| --- | --- |
| `health` | `/healthz` |
| `version` | `/api/v1/version` |
| `auth` | `/api/v1/auth/{login,logout,me}` |
| `setup` | `/api/v1/setup{,info}` |
| `internal` | `/api/v1/_demo/*`（标 description 说明被哪个 proposal 移除） |

未来 `add-app-release-artifact` 加 `apps` / `releases` / `artifacts` tag；`add-pat-and-api-token` 加 `tokens` tag 与 `securitySchemes`（bearer + cookieAuth 并存）。

**错误响应统一**：`ApiError` 类型实现 `utoipa::IntoResponses`，handler 注解只需 `responses(..., ApiErrorResponses)` 一行就把 401/403/404/409/410/422/500 全套响应注入 OpenAPI doc，每个 status 引用同一个 `Problem` schema（RFC 9457 `application/problem+json` body）。**注意**：utoipa 5 的 IntoResponses derive 生成 `$ref` 但不自动注册 schema，所以 ApiDoc 用 `components(schemas(Problem))` 显式带入。

**Router 收集**：`utoipa_axum::router::OpenApiRouter` 替代 `axum::Router`，handler 通过 `routes!(handler)` 宏注册时自动加入 OpenAPI doc，无需手列 paths。同 path 不同 method 的 handler 才放进同一个 `routes!()`，path 不同必须分开多次 `.routes()`。

**版本号**：`info.version` 跟 `CARGO_PKG_VERSION`（也即 `/api/v1/version` 返回值），所以 server binary 升级即文档版本升级。

**相关文件**：`crates/swarmhive-server/src/{openapi.rs,lib.rs,error.rs,routes/*.rs}`、`openspec/changes/archive/2026-05-26-add-openapi-and-admin-client/`（admin client 生成 + CI drift gate + CLI Rust client 是 Non-goals，留给后续 proposal）。

## 仓库组织

SwarmHive 采用单体仓库（monorepo），同时托管 Rust 服务端、CLI、Web 后台、SDK 与 shadcn registry。Cargo workspace 管理 Rust crates，pnpm workspace 管理 npm packages 和 apps。

```text
swarmhive/
├── Cargo.toml                       # Rust workspace 根，members = ["crates/*"]
├── pnpm-workspace.yaml              # pnpm workspace 配置
├── package.json                     # 根 package.json，承载 scripts 与 devDependencies
├── rust-toolchain.toml              # 锁定 Rust 工具链版本
├── biome.json                       # Biome 配置（lint + format）
├── lefthook.yml                     # Git hooks（pre-commit / commit-msg）
├── cliff.toml                       # git-cliff changelog 生成配置
├── commitlint.config.js             # Conventional Commits 校验
├── tsconfig.base.json               # 共享 TS 编译选项
├── .editorconfig                    # 编辑器统一约定
├── .gitignore
├── README.md
├── docs/                            # 早期设计文档（本目录）
├── crates/
│   ├── swarmhive-api-types/       # 共享 HTTP DTO（serde + utoipa::ToSchema），CLI/server/SDK 共用，零 ORM/HTTP/IO 依赖
│   ├── swarmhive-entity/          # sea-orm 实体 + From<&Model> for api-types（仅 server 系依赖）
│   ├── swarmhive-server/          # Axum HTTP server（lib + bin），承载控制面与嵌入式 admin SPA
│   └── swarmhive-cli/             # clap CLI，本地发布与 CI/CD 共用
├── apps/
│   └── admin/                       # Vite + React + AntD 后台，build 后由 rust-embed 嵌入 server
├── packages/
│   ├── sdk-core/                    # @swarm-hive/sdk-core，状态机 + HTTP 客户端 + react 子入口
│   ├── tauri/                       # @swarm-hive/tauri，Tauri 平台适配
│   ├── react-native/                # @swarm-hive/react-native，RN 平台适配
│   ├── registry-web/                # shadcn registry 源码（Tailwind v4 + Radix）
│   └── registry-rn/                 # shadcn registry 源码（NativeWind 4 + @rn-primitives）
├── examples/                        # Tauri / RN / Web 接入示例（后续补充）
├── xtask/                           # 自动化任务 crate（registry 构建、release 流程等）
└── .github/
    └── workflows/                   # CI、发布、registry 同步等流水线
```

工程化约定：

- **包管理**：根 pnpm workspace 统一管理 `apps/*`、`packages/*`，根 Cargo workspace 统一管理 `crates/*` 与 `xtask`。
- **Rust crate 边界（硬约束）**：
  - `swarmhive-api-types` 禁止依赖 sea-orm / axum / tokio / reqwest（仅 serde + utoipa + chrono + uuid + garde）。
  - `swarmhive-entity` 依赖 sea-orm + api-types；不依赖 axum / tokio。
  - `swarmhive-cli` 不依赖 entity / sea-orm；只通过 api-types 解析 server 响应。
  - `swarmhive-server` 同时拥有 lib（`swarmhive_server::*`）与 bin（`swarmhive-server`）target，集成测试可直接 `use swarmhive_server::build_router`。
  - schema 演进仅用 sea-orm `schema-sync`，**不引入** `sea-orm-migration` crate。
- **代码规范**：Biome 负责 JS/TS 的 lint + format，`cargo fmt` + `cargo clippy` 负责 Rust。
- **Git hooks**：lefthook 接入 pre-commit（Biome check、cargo fmt --check）与 commit-msg（commitlint）。
- **提交规范**：Conventional Commits，配合 git-cliff 自动生成 `CHANGELOG.md`。
- **CI**：GitHub Actions 跑 lint、test、build；release tag 触发 server / cli / sdk / registry 的产物发布。
- **MVP 推进顺序**：先 `crates/swarmhive-server` + `crates/swarmhive-cli` + `apps/admin`，registry 与 SDK 包预留目录但延后实现。
