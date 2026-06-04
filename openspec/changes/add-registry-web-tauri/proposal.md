# add-registry-web-tauri

## Why

`add-update-sdk-core` 落地了 `@swarm-hive/sdk` 的 headless 核心(ports + engine),但客户端**还用不起来**——缺平台 adapter(实现 `UpdateAdapter`)+ UI 组件 + 分发机制。本 change 做 `packages/registry-web`(tauriAdapter + useUpdate hook + UI 组件,通过 shadcn registry 源码分发,build 产物随仓库走 GitHub raw),让 Tauri 桌面(SwarmDrop / SwarmNote)从第三方 ToolSetLink 切到自托管 SwarmHive,**端到端跑通整条链路**。

## What Changes

### 1. packages/registry-web(shadcn registry 源码包)

`registry/tauri/<item>/` 源码 + `registry.json`,`shadcn build` → `public/r/*.json`。`registryDependencies` 串联(装一个组件自动带出 hook + adapter),npm deps 自动装。

### 2. tauriAdapter(实现 SDK 的 `UpdateAdapter`,registry:lib)

- **check**:`@tauri-apps/plugin-updater` 的 `check({ headers: { 'X-Client-Id': clientId } })`(配 SwarmHive endpoint,内置 minisign 验签;header 带 client_id 让 server 灰度生效)→ `Update` 缓存进闭包 + 从 `rawJson.swarmhive` 转成 SDK `ReleaseInfo`。**不用** SDK 的 `checkUpdate`(那是 RN 适配用的:RN 无 plugin-updater)。
- **download**:cached `Update.download(onProgress)`(plugin-updater **支持单独** download/install)→ `DownloadSpeedTracker` 进度转 SDK `Progress`。
- **install**:`Update.install()` + `relaunch()`(@tauri-apps/plugin-process)。
- **storage**:`@tauri-apps/plugin-store` 包装成 `KeyValueStorage`。
- **compare**:SDK 的 `semverComparator`。

### 3. use-update hook(registry:hook)

`useUpdate()` = `useUpdateEngine(createUpdateEngine(tauriAdapter))`,薄封装,业务也可直接用 SDK 的 engine。

### 4. UI 组件(registry:component,Tailwind v4 + @radix-ui + lucide-react)

`UpdateProvider` / `PromptUpdateDialog` / `ForceUpdateDialog`(不可关) / `UpdateProgressDialog` / `UpdateSettingsSection` / `ReleaseNotesView`(`releaseNotesRenderer` slot 吸收 Markdown / 纯文本差异)。文案 prop 注入(en / zh-CN 默认),不依赖 i18n 框架。蓝本来自 SwarmDrop 现有 `prompt-update-dialog` / `force-update-dialog` / about-section。

### 5. GitHub raw 分发(免 server host)

`shadcn add` 是**开发时**操作(开发机有外网),加上项目开源、GitHub 公开、无私有组件 → **不做 server `/r` host**。registry-web 留 monorepo,`shadcn build` 产物 `public/r/*.json` 提交进仓库;用户 `components.json` 配 namespace `@swarmhive` 指向 GitHub raw URL,`shadcn add @swarmhive/<item>` 直接装。详见 design D5。

### 6. server endpoint 读 header client_id(配套 D3)

`/api/v1/updates/tauri/:app_slug`(add-update-check-tauri)的 client_id 取值改为 **header `X-Client-Id` → query `client_id` → IP** 三级。因 plugin-updater 运行时**只能传 header**(不能传自定义 query),这让 Tauri 的灰度在 **server 端**统一生效。小改,不动响应格式。

## Acceptance

- `pnpm --filter @swarm-hive/registry-web build:registry` 产出 `public/r/registry.json` + 各 item JSON。
- `components.json` 配 `@swarmhive` namespace 指向 GitHub raw 后,`pnpm dlx shadcn@latest add @swarmhive/prompt-update-dialog` 能装齐 hook + adapter + 组件。
- tauriAdapter 单测(mock @tauri-apps/plugin-updater):check 从 `rawJson.swarmhive` 转 `ReleaseInfo`、download 的 DownloadEvent 转 `Progress`、install 调 `relaunch`。
- registryDependencies 图正确:装一个 UI 组件传递带出 `use-update` + `tauri-adapter`;npm `dependencies`(@tauri-apps/*, @swarm-hive/sdk, @radix-ui/*, lucide-react)声明完整。
- registry-web **不复制** sdk-core 逻辑(状态机 / comparator / rollout 全来自 `@swarm-hive/sdk`);UI 只消费 `useUpdate`。
- `pnpm lint` + `pnpm --filter @swarm-hive/registry-web typecheck` 全绿;`cargo check`(registry route)绿。

## Non-goals

- 不做 RN(`registry-rn` + `rnAdapter` 是后续 change)。
- 不迁移任何真实 app(SwarmDrop / SwarmNote 切 SwarmHive 是各自仓库的后续集成步骤)。
- 不改 SDK 的 `UpdateAdapter` 接口(plugin-updater 的 `Update` 对象依赖由 adapter **内部闭包缓存**解决,接口够用)。
- 不做 registry 鉴权(MVP 公开组件;私有 registry 的 header token 留后续)。
- 不做独立 registry 仓库 / shadcn 短形式(monorepo + GitHub raw 够用;短形式需根目录 `registry.json`,留后续)。

## Depends on

- `add-update-sdk-core`(`UpdateAdapter` ports + `createUpdateEngine` + `useUpdateEngine` + `semverComparator`)
- `add-update-check-tauri`(`/api/v1/updates/tauri/:app_slug` endpoint)

## Maps to docs

- [docs/14-sdk-ui.md](../../../docs/14-sdk-ui.md) —— registry 接入流程、组件清单、Tauri 差异、registry host。
- [dev-notes/knowledge/architecture.md](../../../dev-notes/knowledge/architecture.md) —— SDK / Registry 分发(GitHub raw,registry-web 留 monorepo)。
