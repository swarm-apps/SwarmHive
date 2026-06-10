# 架构

## 概览

SwarmHive 的顶层设计决策——crate 边界、存储抽象、部署形态、上传链路、SDK / registry 分发等"涉及多文件多 crate"的硬约束。改动这些前必须先看这里。

## Crate 边界

### 5 crate 拓扑（硬约束,2026-06-10 加 migration）

```text
swarmhive-api-types  serde DTO + utoipa::ToSchema     CLI + server 共用
swarmhive-entity     sea-orm Entity + From<api-types>  仅 server 系依赖
swarmhive-migration  sea-orm-migration data migrations 仅 server 依赖
swarmhive-server     lib + bin: 业务/storage/auth/    lib 可被集成测试 import
                     mail/routes/SPA embed
swarmhive-cli        clap + reqwest + indicatif      不依赖 entity / sea-orm
```

**不准依赖**（CI 应有回归测试）：

- `api-types` 不依赖 sea-orm / axum / tokio / reqwest（薄共享层）
- `entity` 不依赖 axum / tokio
- `migration` **不依赖 entity**(migration 是冻结的历史记录,引用持续演进的 Entity 会"实体漂移"——官方 Entity First 文档明确警告;要表结构就 raw SQL / SeaQuery 写死)
- `cli` 不依赖 entity / sea-orm / migration（**关键**：`cargo tree -p swarmhive-cli | grep sea-orm` 必须无输出）
- `api-types` 不反向依赖 entity（避免环）

**Why**：CLI 与 server 业务零重叠；唯一真正共享的是 HTTP DTO，由 api-types 承担。引入 core 类的"业务容器"会拖累 CLI 编译时间。详见 `openspec/changes/add-crate-restructure/`。

**相关文件**：`Cargo.toml`、各 crate `Cargo.toml`、`docs/03-architecture.md` "Rust crate 边界（硬约束）" 段。

### server lib + bin 两 target

`swarmhive-server` 同时声明 `[lib]`（`swarmhive_server::*`）和 `[[bin]]`（`src/bin/server.rs`）。集成测试用 `use swarmhive_server::build_router;`。

**正确做法**：
- 新增业务模块都加到 `src/lib.rs` 的 `pub mod` 列表
- bin 保持瘦身：tokio main + tracing-subscriber + 加载 config + `build_router(state)` + serve
- 业务逻辑、router 装配、middleware 装配都在 lib 里

**相关文件**：`crates/swarmhive-server/src/lib.rs`、`crates/swarmhive-server/src/bin/server.rs`。

### api-types ↔ entity 转换的归属

`impl From<&entity::Model> for api_types::User` 这种转换**写在 entity crate 里**，不在 server 也不在 api-types。

**Why**：转换跟着 entity 字段走，字段改动一处搞定；api-types 不能反向依赖 entity（环）。

**相关文件**：`crates/swarmhive-entity/src/*/mod.rs`。

## 存储抽象

### S3-compatible 是唯一正式存储后端

不提供 local filesystem backend。单服务器场景通过 bundled RustFS（Docker Compose profile 或 `swarmhive storage init rustfs`）解决，RustFS 仍以 S3 API 接入。

**Why**：单一抽象让 RustFS → OSS / R2 / S3 迁移只需改 config；引入 local FS backend 会破坏这个保证，并把 server 拖累成文件服务器。

**正确做法**：
- 所有 storage 操作走 `swarmhive-server::storage` 的 trait（S3 客户端用 `aws-sdk-s3`）
- 上传中转目录只用于临时缓存，不作为产物最终存储

**相关文件**：`crates/swarmhive-server/src/storage/mod.rs`、`docs/07-storage-and-delivery.md`。

### 对象路径规范

**去 channel、按版本寻址**（与发布列车指针模型一致——promote 只移 channel 指针，对象零动）：

```text
{prefix}/apps/{app_slug}/versions/{version}/{platform}/{target}/{filename}
```

例：`apps/swarmdrop/versions/0.4.5/tauri-desktop/x86_64-pc-windows-msvc/SwarmDrop_0.4.5_x64-setup.exe`

`{platform}` 是 `tauri-desktop` / `react-native-android`；`{target}` Tauri 取 target triple、Android 取 abi。

**不要做**：不要把 `channel` 放进对象路径——同一 release 被多个 channel 同时指向时，promote / rollback 会因此重传产物。

**相关文件**：`docs/07-storage-and-delivery.md` 末段。

## CLI 上传链路（presign 直传 + complete 回调）

CLI 不走 server 中转。流程：

1. `POST /api/v1/apps/:slug/releases/:ver/uploads/presign` —— server 校验权限、生成 per-file presigned PUT URL。
2. CLI `PUT <signed-url>` 直传 S3 / RustFS / OSS（带进度条）。
3. `POST /api/v1/apps/:slug/releases/:ver/uploads/:upload_id/complete` —— server HEAD 对象校验 size/etag、写 release / artifact、返回 endpoints。

**正确做法**：
- complete 接口幂等：同 `upload_id` 重复 complete 返回相同 release_id（用 Postgres `ON CONFLICT`）
- presign URL 5–10 min 过期
- 失败重试持有 `upload_id` + `parts[]`，可重发单个 part

**不要做**：
- 不要在 server 端 stream 转发字节（CLI publish 大文件会拖死单 binary）
- 不要二次下载校验 hash（信任 client sha256 报告 + 写 audit 即可）
- **CLI 直传 PUT 必须绑 `Content-Length`**：`reqwest::Body::wrap_stream` 自身长度未知 → reqwest 发 `Transfer-Encoding: chunked`，而 S3 兼容存储的 PUT **不接受 chunked**——rustfs 直接回 `400 UnexpectedContent`（MinIO 部分版本也拒）。`upload_put` 必须先 `file.metadata().await.len()` 拿大小、再 `.header(CONTENT_LENGTH, size)`，流式 + 进度条仍保留。**血泪（2026-06-03）**：这条 CLI 上传路径 e2e 一直 deferred（bin crate 不可 import），首次真实发布（SwarmNote-RN 161MB APK → bundled rustfs）才暴露；诊断时 curl `--data-binary` 自带定长、PUT 200 反而误导，必须用真实 CLI 跑才能复现。**相关**：`crates/swarmhive-cli/src/commands/client.rs` 的 `upload_put`。

**相关文件**：`docs/12-cli.md` "上传形态" 段、`docs/06-cicd.md` publish 段。

## 部署形态

### Server + Admin 单 binary

Admin SPA 构建产物通过 `rust-embed` 嵌入 server binary。Axum 负责 SPA fallback（除 `/api/*` 外都回 index.html）。

**Dev 与 prod 不同**：
- Dev：Vite :5173 代理 `/api` 和 `/healthz` 到 Rust :3030
- Prod：单 binary，Admin SPA 已嵌入

**正确做法**：所有 API 路径放 `/api/...` 下，让 SPA fallback 不会误匹配。registry JSON **不经 server**——走 GitHub raw（见下方「SDK / Registry 分发」），server 无 `/r` 路由。

**相关文件**：`apps/admin/vite.config.ts`、`crates/swarmhive-server/src/lib.rs`。

### Single-server bundled

`docker compose --profile bundled-storage up -d` 同机起：server（嵌入 admin SPA）+ **Postgres** + RustFS + nginx/caddy。Postgres 保存所有结构化数据。

**Why**：用户决定一刀切 Postgres only，不保留 SQLite 路径，避免 SQL 方言双轨维护成本（详见 [backend.md](backend.md) Postgres only 条）。

**相关文件**：`docs/03-architecture.md` "部署方式" 段。

### bundled-storage profile 落地现状（2026-06-02）

`docker-compose.yml`（仓库根）**只覆盖存储层**：`bundled-storage` profile = `rustfs`（S3 `:9000` / console `:9001`）+ 一次性 `rustfs-init`（minio/mc）自动建桶。**刻意不收编 postgres / mailpit**（见下方血泪教训）——它们维持 CLAUDE.md 的手动 `docker run`。

**关键坑**：server 的 storage probe 与 presign 上传**都不自动建桶**（`rg create_bucket` 在 server 无命中）；bucket 不存在时 `swarmhive storage init rustfs` 的 probe 直接失败。建桶职责落在 `rustfs-init`（compose）或用户手动。docs/07 "自动创建 bucket" 仍是未实现的向导目标。

**正确做法**：
- 既有 `docker run` 创建的卷（`swarmhive-rustfs-data` / `swarmhive-pg-data`）在 compose 里用 `volumes.<x>.name:` 显式复用，否则 compose 会加项目前缀（`swarmhive_*`）变成新卷、丢数据。复用既有卷时会有一条 "volume ... not created by Docker Compose" 的良性 warning，仅迁移场景出现。
- `rustfs-init` 用 compose 内部 DNS `http://rustfs:9000` 建桶；宿主 `cargo run` 的 server 连存储走端口映射 `http://localhost:9000`。
- rustfs 默认凭证 `rustfsadmin/rustfsadmin`（env `RUSTFS_ACCESS_KEY` / `RUSTFS_SECRET_KEY`），生产经 `.env` 覆盖（见 `.env.example`）。`init rustfs` 把 `force_path_style` 硬编码为 `true`。

**不要做（血泪教训，2026-06-02）**：
- 不要让 compose 用与手动 `docker run` 容器相同的 `container_name`（`swarmhive-pg` / `swarmhive-mailpit`）去"接管"它们。对该 profile 执行 `docker compose down --remove-orphans` 时，compose 会把这些手动容器当 orphan 删除，**并销毁其 named volume**——实测把 dev 的 `swarmhive-pg-data` 连同整个数据库删没了。pg / mailpit 维持各自的 `docker run`，compose 只管 storage 层（已据此把 `infra` profile 从 compose 移除）。
- 不要在 compose `entrypoint` 的 block scalar 里用 `$$` 做 shell 算术（`i=$$((i+1))`）。compose 不把它转义成单 `$`，字面 `$$`（=PID）传进容器会令计数死循环、init hang。要等待就用最简 `until` 重试（mc 镜像的 sh 本身支持算术，问题出在 compose 转义层）。

**未落地**：完整 single-server app stack（server 镜像[嵌入 admin SPA] + caddy/nginx）——缺 `swarmhive-server` 的 Dockerfile，待补 `app` profile。

**相关文件**：`docker-compose.yml`、`.env.example`、`crates/swarmhive-cli/src/commands/storage.rs`（`init_rustfs`）、`crates/swarmhive-server/src/storage/`。

## 平台主线

**只覆盖 Tauri 桌面 + React Native Android**。iOS / Electron / Flutter / Web 热更新**明确不做**。OTA（Expo Updates / CodePush-compatible）是 provider 扩展层，MVP 不实现，只在 ProviderConfig 留扩展点。

**不要做**：
- 不要把 OTA-specific 假设（runtime_version、bundle、diff package）烤进 core types
- 不要因为某个用户问"能不能加 iOS"就开始改架构

**相关文件**：`docs/04-platform-support.md`、`docs/11-ota-providers.md`。

## SDK / Registry 分发（前端 npm 侧）

SwarmHive 自己**不发 UI**。客户端更新逻辑通过 **1 个 headless npm 包 + 2 套 shadcn registry** 分发，核心是 **ports & adapters**（2026-06 从原 4 包方案修订，见 `add-update-sdk-core`）：

- **`@swarm-hive/sdk`**（唯一 npm 包，`packages/sdk`）：零平台依赖的 headless 核心——`UpdateAdapter`(ports) + `createUpdateEngine`(8 态状态机) + `semverComparator`/`versionCodeComparator` + `inRolloutBucket` + `checkUpdate` + 类型 + `./react` 订阅层。
- **shadcn registry**：`packages/registry-web`(tauriAdapter + useUpdate + 6 UI 组件,**已落地** `add-registry-web-tauri`) / `packages/registry-rn`(rnAdapter + UI,待做)。平台 adapter + 绑定它的 hook + UI 组件**源码**通过 `pnpm dlx shadcn@latest add @swarmhive/<item>` 拉进用户项目。

**ports & adapters 边界（核心）**：`UpdateAdapter`{check, download, install, storage, compare} 是 npm↔registry 唯一契约。平台代码(Tauri plugin-updater 包装 / RN PackageInstaller)**全进 registry**，npm 零平台依赖——因为平台适配本就需用户改源码、且 npm 零依赖最稳、bug 集中修。

**实现要点 / 踩坑**：

- **build 用 tsdown**(rolldown，tsup 继任)；ESM only；`exports` 双子入口 `.` + `./react`；`react` 是 optional peer。
- **状态机用 zustand vanilla**(4 个真实 app 都用 zustand；`zustand/vanilla` 框架无关，`./react` 用 `zustand` 的 `useStore`)。
- **灰度分桶逐位对齐 server**：`@noble/hashes` 的 blake3 + `DataView.getBigUint64(0, true)` 对齐 Rust `u64::from_le_bytes`。server `updates.rs::rollout_buckets_match_sdk_reference` 与 SDK `rollout.test.ts` 的 `SERVER_BUCKETS` 共用同一组锚点(`client-0→2` … `client-9→63`)双向锁定——任一端 blake3 / 字节序漂移都会让两端任一测试失败。已实测 Rust 与 TS 结果完全一致。
- **类型 codegen 复用 admin 链路**(`openapi-typescript` 从 server OpenAPI doc)。⚠️ `cargo run --bin dump-openapi` 首编 dev server lib 较慢；可临时 `cp apps/admin/src/lib/api/schema.gen.ts`(同一 OpenAPI 生成)解 unblock，sdk 的 `codegen` script 仍独立保留供 CI/后续跑。
- **零平台依赖守护**：`scripts/assert-no-platform-deps.mjs`(CI)断言 `dependencies` 无 `@tauri-apps/*` / `expo-*` / `react-native`(同 CLI `cargo tree | grep sea-orm` 范式)。
- 文案 prop 注入，SDK 不依赖 i18n 框架。
- **tauriAdapter（`add-registry-web-tauri`）**：check 走 `@tauri-apps/plugin-updater` 的 `check()`（内置 minisign 验签，**不用** SDK 的 `checkUpdate`——那是 RN 用），从 `update.rawJson.swarmhive` 归一化 `ReleaseInfo`；download/install **拆开**用 `Update` 实例（plugin-updater v2 支持单独 `download` + `install`，非只有 `downloadAndInstall`）；client_id 经 **`X-Client-Id` header** 传（plugin-updater 运行时只能传 header、不能传自定义 query），让灰度在 **server 端**生效（`updates.rs` 取 client_id 改 header→query→IP 三级，回归 `rollout_via_x_client_id_header`）。UI 组件「下载完成→自动 install + relaunch」复刻 SwarmDrop 一体 UX。
- **registry 分发走 GitHub raw，不经 server（2026-06-03 修订）**：`shadcn add` 是**开发时**操作（开发机有外网）、项目开源公开、无私有组件 → 「内网/离线/私有」三理由全不成立，**不做 server `/r` host**（曾写过 `routes/registry.rs` + rust-embed，已移除）。`shadcn build` 产物 `public/r/*.json` 提交进仓库，用户 `components.json` 配 namespace `@swarmhive` 指向 `raw.githubusercontent.com/swarm-apps/swarmhive/<ref>/packages/registry-web/public/r/{name}.json`。vendored `components/ui/*` + `lib/utils.ts` 仅供 registry-web 本地 typecheck（**不列** `registry.json` items，消费者经 `dialog`/`button`/`progress`/`utils` 从 @shadcn 拿 canonical）。`public/r` 已加入 biome ignore（生成物，同 dist）。

**不要做**：

- 不要把平台 adapter / UI / hook 塞进 SDK npm 包(破坏零平台依赖，且用户改不了源码)。
- 不要给 server 加 `/r` registry host(分发走 GitHub raw;装组件是开发时操作,不需跟随 server 内网部署)。
- 不要让 SDK 引入 i18n 框架(让用户自己注 react-i18next / Lingui)。
- ⚠️ **环境坑**：几百个僵尸 `rg`(ripgrep，Explore agent / workflow 搜索残留)会把 cargo 编译拖到**像死锁**(5+ 分钟不动)。cargo 异常慢时先 `pgrep -xc rg` 查、`pkill -x rg` 清，再重试。

**相关文件**：`docs/14-sdk-ui.md`、`packages/sdk/`、`crates/swarmhive-server/src/routes/updates.rs`(rollout reference 锚点 + `in_rollout_bucket`)。

### registry-rn 样式：NativeWind + React Native Reusables（2026-06-05 重构）

registry-rn 的 5 个 UI 组件（prompt / force / progress dialog + release-notes-view + settings-section）从「裸 RN 原语 + StyleSheet + 硬编码 hex（如 `#2563EB`）」重写为 **NativeWind v5 className + React Native Reusables（RNR / `@rn-primitives`）原语**，与 registry-web 的 shadcn token 模型对齐。颜色全部用语义 token（`bg-background`/`bg-muted`/`text-foreground`/`text-muted-foreground`/`text-primary`/`text-destructive`/`border-border`），由 **consumer 自己的 `global.css` 决定**——自动适配各 app 主题（SwarmDrop 蓝 / SwarmNote 琥珀金）且自带暗色，registry **绝不写死颜色**。镜像范本是 `SwarmNote-RN/src/components/update/*`（生产 RNR 更新 UI）。

**分发模型（镜像 web 的 @shadcn 范式）**：UI 组件的 `registryDependencies` 指向 **`@react-native-reusables/*` namespace**（`dialog`/`alert-dialog`/`button`/`progress`/`text`），consumer 在 `components.json` 的 `registries` 注册 `@react-native-reusables` → `https://reactnativereusables.com/r/nativewind/{name}.json`,装 `@swarmhive-rn/*` 时由 shadcn 从 RNR 官方 registry 拉 canonical 原语。SwarmHive **不把 RNR 原语 vendor 进 `registry.json` items**（同 web 经 `@shadcn` 拿 dialog/button/progress 的范式）。`utils`(cn) 仍用裸名（=shadcn canonical 同款 clsx+twMerge）。

**vendored-for-typecheck**：`registry/rn/components/ui/{text,button,icon,native-only-animated-view,alert-dialog,dialog,progress}.tsx` + `lib/utils.ts` + `nativewind-env.d.ts` + `global.css`（neutral 主题契约参考）= **逐字镜像 RNR canonical,仅供本包 `tsc --noEmit` 解析 `@/components/ui/*`,不列 registry.json items**（同 web vendored ui 范式）。`package.json` devDeps 加整套 RNR 栈：`nativewind@5.0.0-preview.3`、`@rn-primitives/{alert-dialog,dialog,progress,slot}@^1.4`、`react-native-reanimated@4`、`react-native-screens`、`lucide-react-native`、`class-variance-authority`、`clsx`、`tailwind-merge`、`tailwindcss@4`。

**关键坑 / 决策**：

- ⚠️ **遮罩必须内联 style**：NativeWind v5 preview 下 `react-native-css` 会**静默丢弃 rgba 透明色**与部分 arbitrary 布局工具类 → Dialog/AlertDialog 的 `Overlay` 布局 + `backgroundColor:"rgba(0,0,0,0.5)"` 必须走内联 `style`,不能 className（vendored alert-dialog/dialog 已照此,头注释标注）。
- **PortalHost 前置**：RNR Dialog/AlertDialog 走 Portal,consumer 根布局必须挂 `PortalHost`,否则弹窗**静默不渲染**(已在组件头注释 + registry.json description 标注)。
- **弹窗原语映射**：prompt → `Dialog`(可关闭,带 Close X + 受控 `open`/`onOpenChange`);force / progress → `AlertDialog`(不可关闭,无 X)。progress **用 AlertDialog 而非 Dialog**——RNR `DialogContent` 总渲染一个 Close X,不适合下载中常驻的进度视图(镜像 SwarmNote 生产)。
- **AlertDialogAction(RNR canonical)`disabled` 不自动变暗**（只有 `Button` 组件加 `opacity-50`）→ 需禁用态视觉反馈时在**调用处**加 `className={busy ? "opacity-50" : undefined}`,**不改 vendored 原语**(改了就偏离 consumer 实际拉到的 canonical)。
- **spinner 用 RN 内置 `ActivityIndicator`**(零依赖)替 web 的 `lucide` Loader2;其余 icon 一律省略(避免引 lucide 直接依赖 + RN 里 icon 颜色需解析 token 而非 className)。
- **prompt 弹窗任意关闭都 `postpone()`(RN + web 一并修,2026-06-07)**:迁到 RNR/shadcn `Dialog` 后,`DialogContent` 自带 Close X、`onOpenChange` 也响应返回键 / Esc / 点遮罩。若把 `onOpenChange` 裸透传给父级,只有「稍后」按钮走 `postpone()`、其余关闭路径漏记 dismiss-TTL → 下次回前台复核(RN 的 AppState `'active'` / web 的 window focus)立刻重弹。修法:组件内拦一层 `handleOpenChange`,`!next && !busy` 时先 `postpone()` 再透传;「稍后」按钮也复用它。**两端同病同治**(web 原样镜像了这个漏洞,按"各平台最优体验"一起改),`busy`(下载中 / ready)关闭只隐藏 UI、不 postpone。**相关**:`packages/registry-{rn,web}/.../prompt-update-dialog.tsx`。
- 测试 `registry-build.test.ts`:放行 `@swarmhive-rn/` + `@react-native-reusables/` 两个 namespace,裸名只许 `utils`,仍拒绝裸 `dialog`/`button`/`progress`/`text`/`alert-dialog`(会被 shadcn 解析成 web @shadcn/Radix)。

**下游未同步**：`apps/docs` 的 RN Snack 预览（`components/demo-rn/*.app.tsx`）是**独立手写的内联样式 demo**(为绕 Snackager 限制做了零依赖,Snackager 装不了 nativewind+RNR)——重构 registry-rn **不影响 docs 构建**,但 demo 仍是旧内联样式,与真实 RNR 组件有视觉偏差,留作后续 polish。

**相关文件**：`packages/registry-rn/registry/rn/components/`、`packages/registry-rn/registry/rn/components/ui/`(vendored)、`packages/registry-rn/{registry.json,components.json,package.json}`、`SwarmNote-RN/src/components/update/`(镜像范本)。

### 文档站 / 组件展示（apps/docs，`add-docs-website` 2026-06-04）

`apps/docs`(`@swarm-hive/docs`)= 官网 + 文档站，展示 registry 组件，是 registry 的**展示层**（不改 GitHub raw 分发链路）。Next.js 16 `output:'export'` 静态导出 + Fumadocs(MDX/Orama) + Tailwind v4 + shadcn(new-york/neutral)，部署 GitHub Pages `swarm-apps.github.io/SwarmHive/`(workflow `Deploy Docs`)。

**踩坑（都修过）**：

- **GitHub Pages 子路径 basePath 必须用仓库名实际大小写 `/SwarmHive`**：Pages 文件大小写敏感，小写让 `_next/*` 文件 404（目录会重定向、文件不会）。经 `PAGES_BASE_PATH` env 注入，再暴露成 `NEXT_PUBLIC_BASE_PATH`(`lib/site.ts`)给客户端拼 `<iframe src>`（basePath 不自动前缀裸 iframe）；OG 图在 `getPageImage` 手动补 basePath，`metadataBase` 用纯 origin。Fumadocs MDX 内链 / Next `<Link>` 会自动带 basePath。
- **mock live preview**：`shadcn add @swarmhive/*` 把真组件装进 docs，浏览器内 mock `UpdateAdapter`(`components/demo/mock-adapter.ts`) + `DemoUpdateProvider`(`createUpdateEngine` 注入与组件同一个 `UpdateEngineContext`)驱动状态机跑各状态，不连后端/不依赖 Tauri——印证 ports & adapters。
- **iframe 隔离预览**：`<ComponentPreview>` 经 iframe 加载 `/preview/[name]` 独立静态页。原因：Radix Dialog 模态遮罩 `fixed inset-0` 相对视口 + modal 模式给 portal 外加 `pointer-events:none`，内联会劫持整页且无法外部关闭；iframe 把遮罩框在预览框内（同 shadcn/Radix 官网方案）。
- **shadcn add 两缺口**：① 漏装 `class-variance-authority`；② 未注入 shadcn 主题 token → 手动在 `app/global.css` 加 new-york/neutral `:root`/`.dark`/`@theme inline`，复用 Fumadocs `base.css` 的 `@variant dark(.dark)`。
- **中文搜索**：Orama 默认 english tokenizer 把中文整句当一个 token → 失效。装 `@orama/tokenizers/mandarin`，**服务端 `createFromSource` 与客户端 `initOrama` 必须用同一 `createTokenizer()`**，否则索引/查询不对齐。

## 单组织 + 完整 RBAC

MVP **不**做多租户。所有核心表预留 `org_id`，但只有默认 Organization。5 角色（Owner / Admin / Release Manager / Developer / Viewer），权限按 verb-scoped permission 颗粒度（`release:publish`、`storage:manage` 等）鉴权。

**相关文件**：`docs/13-rbac.md`。
