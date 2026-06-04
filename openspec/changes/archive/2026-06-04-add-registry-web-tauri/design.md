# design

## Context

`add-update-sdk-core` 给了 `@swarm-hive/sdk`:`UpdateAdapter` ports + `createUpdateEngine`(8 态状态机) + `useUpdateEngine` + `semverComparator` / `inRolloutBucket` / `checkUpdate`。本 change 在 registry 侧实现 **Tauri 平台 adapter + UI**,并通过 GitHub raw 分发 registry。

调研(三路)定下的事实:

- **shadcn registry 自托管 = 纯静态 JSON**;`shadcn build` 把 `registry/` 源码编译进 `public/r/*.json`;`registryDependencies` 串联(装一个组件自动带 hook + adapter);CLI `add` 是 Node 端 fetch,**不需 CORS**。
- **`@tauri-apps/plugin-updater`(经 plugin-updater 源码核对修正)**:① `Update` 对象暴露**单独的 `download(onEvent?)` 与 `install()`**(`downloadAndInstall` 只是二者的便捷封装)——本 change **拆开用**,download 走 engine 的 downloading 态、install 后 `relaunch()`;② `check()` 内置 minisign 验签且支持运行时 `headers`,返回的 `Update` 是 download 的前置(download 必须用它),`headers` 可带 `X-Client-Id` 让 server 灰度生效。
- **分发走 GitHub raw**:`shadcn add` 是开发时操作、项目开源公开,故**不做 server `/r` host**;build 产物随仓库走 GitHub raw(见 D5)。

## Goals / Non-Goals

**Goals:**

- tauriAdapter 实现 `UpdateAdapter`,把 plugin-updater 的 check/download/install/relaunch 桥接到 SDK engine
- registry-web 通过 shadcn registry 分发 adapter + hook + UI,`registryDependencies` 串联
- GitHub raw 分发(build 产物随仓库提交,免 server host)
- 让 Tauri 桌面端到端可用(单测 + registry build + GitHub raw 装齐验收;真实 app 迁移后续)

**Non-Goals:**

- 不做 RN / registry-rn;不迁移真实 app;不改 SDK `UpdateAdapter` 接口;不做 server `/r` host / 独立 registry 仓库

## 数据流

```text
  SwarmDrop/SwarmNote 桌面(用户项目,shadcn add 拉源码进来)
    import { useUpdate } from "@/components/swarmhive/use-update"
        │
        ▼  registry-web 源码(复制进项目):
  ┌──────────────────────────────────────────────────────────────────┐
  │ useUpdate() = useUpdateEngine(createUpdateEngine(tauriAdapter))     │
  │ <PromptUpdateDialog/> <ForceUpdateDialog/> <UpdateProgressDialog/>  │
  │ tauriAdapter: UpdateAdapter                                         │
  │   check(ctx):                                                       │
  │     plugin-updater.check()  ──配 tauri.conf endpoints──▶ SwarmHive  │
  │        → Update{version, body, rawJson{...,swarmhive}} (已 minisign 验签)│
  │     缓存 Update 进闭包(_pending);转 ReleaseInfo(取 rawJson.swarmhive)│
  │     ▲ check({headers:{'X-Client-Id':ctx.clientId}}) 让 server 灰度生效│ (D3)
  │   download(rel, onProgress):                                        │
  │     _pending.download(onEvent) ─Started/Progress/Finished─▶ onProgress│ (D2)
  │   install(): _pending.install() + relaunch() (@tauri-apps/plugin-process)│
  │   storage: plugin-store 包装 KeyValueStorage                        │
  │   compare: semverComparator (来自 @swarm-hive/sdk)                  │
  └──────────────────────────────────┬───────────────────────────────┘
                                     │ 依赖(npm): @swarm-hive/sdk, @tauri-apps/*, @radix-ui, lucide-react
                                     ▼
                           @swarm-hive/sdk (engine 状态机 + 纯算法)

  分发(GitHub raw,免 server):
    packages/registry-web/registry/tauri/<item>/  ──shadcn build──▶ public/r/*.json
        │  (提交进仓库 swarm-apps/swarmhive)
        ▼
    GitHub raw:  raw.githubusercontent.com/.../public/r/<name>.json ──▶ JSON
        ▲
    components.json 配 @swarmhive→raw URL;pnpm dlx shadcn add @swarmhive/prompt-update-dialog
```

## Decisions

### D1. tauriAdapter.check —— 用 plugin-updater check(验签),不用 SDK checkUpdate

**张力**:SDK 的 `checkUpdate`(fetch SwarmHive endpoint)和 plugin-updater 的 `check()`(也 fetch + 验签)二选一。

**决策**:Tauri 用 **plugin-updater `check()`**。理由:

- plugin-updater check 内置 **minisign 验签**(用 `tauri.conf.json` 的 pubkey 验 `signature`)——这是 Tauri 更新的安全底线,不能丢。SDK checkUpdate 纯 fetch、不验签。
- plugin-updater 的 `download` / `install` **必须**用它自己 `check()` 返回的 `Update` 对象(下载 + 安装逻辑挂在该实例上)。若 check 走 SDK checkUpdate(返回纯数据 `ReleaseInfo`),download 时还得再调一次 plugin-updater check(重复 fetch)。

`SDK.checkUpdate` 因此**专属 RN**(RN 无 plugin-updater,自己 fetch + 校验 sha256)。这正是 ports/adapter 的价值:`check` 抽象,两平台各自实现。

**实现**:plugin-updater `check()` → `Update` → 缓存进 adapter 闭包(`_pendingUpdate`,复刻 SwarmDrop 的 `_pendingDesktopUpdate`)+ 从 `update.rawJson.swarmhive` 取 `{upgrade_type, min_version, rollout_percent, channel}` 转成 SDK `ReleaseInfo`(`version`/`url`/`signature`/`notes`=`update.body` 等)。**`UpdateAdapter` 接口不变**——平台 `Update` 由 adapter 内部缓存,不进 SDK 接口(印证 add-update-sdk-core 的 design R1:接口够用)。

### D2. download / install —— 直接拆(plugin-updater **支持**单独 download/install)

**核实修正**:plugin-updater v2 的 `Update` 对象有**独立的** `download(onEvent?)` 和 `install()`(JS 源码确认,非只有 `downloadAndInstall` 一体)。与 SDK engine 的 `download() → ready → install()` 分离**天然对齐**,无需 workaround。

**决策**:

- `download(rel, onProgress)` = 缓存的 `_pendingUpdate.download(onEvent)`。`DownloadEvent`(`Started{contentLength}` / `Progress{chunkLength}` / `Finished`)经 `DownloadSpeedTracker`(500ms 节流,来自 SwarmDrop)转成 SDK `Progress{downloaded,total,percent,speed}`。完成 → engine `ready`。
- `install()` = `_pendingUpdate.install()` + `relaunch()`(@tauri-apps/plugin-process)重启生效。

`DownloadSpeedTracker` 放 **registry adapter**(平台特定 UI 关注点),**不进 sdk-core**(sdk 零平台 + 不该管速度展示)。

### D3. 灰度 —— 走 header 让 server 端生效(核实后改进)

**核实修正**:plugin-updater 的 `check(opts)` 支持运行时 **`headers`**(JS 源码确认 `CheckOptions.headers?: HeadersInit`,Rust 端原样带进对 endpoint 的请求);但**不支持运行时自定义 query / endpoint 模板变量**(endpoint 是 `tauri.conf.json` 静态 URL,只替换 `current_version`/`target`/`arch`/`bundle_type`)。

**决策**:**tauriAdapter.check 调 `check({ headers: { 'X-Client-Id': ctx.clientId } })`,server 的 `/updates/tauri` endpoint 从 `X-Client-Id` header 读 client_id(fallback query),灰度在 server 端统一生效。** 策略(版本/force/min_version/灰度)全部归 server,adapter 只读响应。

- **配套 server 改动(本 change 范围)**:`routes/updates.rs` 的 `tauri` handler 取 client_id 改为 **header `X-Client-Id` → query `client_id` → IP** 三级(原是 query → IP 两级)。Tauri 走 header、RN 走 query(RN 自己 fetch 能拼 query),两端都让 server 灰度生效。
- 客户端 `inRolloutBucket` 退化为**可选 defense-in-depth**(server 已灰度,adapter 默认不再算;SDK 仍导出它供无 header 通道的平台 / 离线兜底)。两端 `blake3` 逐位一致(add-update-sdk-core 已实测),所以即便客户端兜底也与 server 等价。
- `ctx.clientId` 由 engine 从 `ensureClientId(adapter.storage)` 注入(storage = plugin-store)。
- **channel**:走 plugin-updater 的静态 endpoint URL(`tauri.conf.json` 配 `&channel=stable`);只有运行时动态的 client_id 走 header。
- **rawJson 透传**(核实确认):server 的 `swarmhive:{...}` 自定义字段原样出现在 `update.rawJson` —— `upgrade_type`/`min_version`/`rollout_percent`/`channel` 都从 `rawJson.swarmhive` 取(D1)。

### D4. registry-web 组织 + registryDependencies 图

```text
packages/registry-web/
  registry.json                 # $schema + name "swarmhive" + items[]
  registry/tauri/
    tauri-adapter/...            # registry:lib   dep: @tauri-apps/{plugin-updater,plugin-process,plugin-store}, @swarm-hive/sdk
    use-update/...               # registry:hook  registryDep: tauri-adapter        dep: @swarm-hive/sdk, react
    update-provider/...          # registry:component  registryDep: use-update
    prompt-update-dialog/...     # registry:component  registryDep: use-update      dep: @radix-ui/react-dialog, lucide-react
    force-update-dialog/...      # registry:component  registryDep: use-update
    update-progress-dialog/...   # registry:component  registryDep: use-update      dep: @radix-ui/react-progress
    update-settings-section/...  # registry:component  registryDep: use-update
    release-notes-view/...       # registry:component  (releaseNotesRenderer slot)
    lib/utils.ts                 # registry:lib   cn() helper(如组件需要)
  package.json                   # devDep shadcn;script build:registry = "shadcn build"
  public/r/*.json                # build 产物
```

`registryDependencies` 用 **namespace 形式 `@swarmhive/<name>`**(**不**硬编码 host)——让 registry 源码与分发**解耦**:用户在 `components.json` 的 `registries` 配 `@swarmhive` 指向 GitHub raw URL,CLI 按该 namespace 解析(D5)。装 `prompt-update-dialog` → 传递带出 `use-update` → `tauri-adapter`;npm deps 各 item 声明、CLI 自动 `pnpm install`。Tailwind v4 token 走 `cssVars.theme` / `css`(非 v3 的 `tailwind` 字段)。

### D5. 分发 —— GitHub raw(registry-web 留 monorepo,免 server)

**关键澄清**:`shadcn add` 装组件是**开发时**操作(开发者开发 SwarmDrop/SwarmNote 时把更新 UI 源码拉进项目),在**开发机**(有外网)跑——与 SwarmHive **server** 的内网/离线**运行时**部署无关(那是终端用户 app 检查更新)。加上项目**开源、GitHub 公开、无私有组件**,所以"内网/离线/私有"三个理由**全不成立** → **不做 server `/r` host**(已移除),直接 GitHub 分发。

**registry-web 留在 monorepo**(`packages/registry-web`),`shadcn build` 产 `public/r/*.json` **提交进仓库**(`swarm-apps/swarmhive`)。用户 `components.json` 配 namespace 指向 GitHub raw:

```jsonc
"registries": {
  "@swarmhive": "https://raw.githubusercontent.com/swarm-apps/swarmhive/main/packages/registry-web/public/r/{name}.json"
}
```

`shadcn add @swarmhive/prompt-update-dialog` → raw URL → build 产物 JSON → 递归带出 `use-update` + `tauri-adapter`(registryDependencies 的 `@swarmhive/<name>` 同 namespace 解析)。**版本锁定**:把 URL 的 `main` 换成 tag/commit(`.../v1.0.0/...`)。

**零 server 负担、零 rust-embed、零运行时依赖**——registry 是开发期 dev-dependency,不进 SwarmHive server 的运行时职责。

> 注:shadcn 短形式 `owner/repo/<item>` 要求 `registry.json` 在**仓库根**;monorepo 根是 workspace 配置、不放 registry.json,故用 **raw URL 模板**(指向 `packages/registry-web/public/r/`)而非短形式。若以后想要更优雅的短形式体验(`swarm-apps/swarmhive-registry/<item>#tag`),可另开一个根放 registry.json 的公开 registry 仓库,但 MVP 不必。

### D6. UI 组件(SwarmDrop 蓝本)

来自 SwarmDrop `src/components/upgrade/*` + about-section,搬进 registry 并参数化:

| 组件 | type | Radix | lucide | 关键 props / slot |
|---|---|---|---|---|
| UpdateProvider | component | — | — | `app` / `channel` / `baseUrl` → 建 engine + context |
| PromptUpdateDialog | component | Dialog | Loader2/Download | `open`/`onOpenChange`,`releaseNotesRenderer?` |
| ForceUpdateDialog | component | Dialog(trap) | Loader2 | 禁 ESC / outside-click(不可关) |
| UpdateProgressDialog | component | Progress | — | 进度 + 速度(`Progress` from engine) |
| UpdateSettingsSection | component | Progress | RefreshCw/Download | 设置页"检查更新"区块 |
| ReleaseNotesView | component | — | FileText | `releaseNotesRenderer?`(Markdown vs 纯文本) |

文案 prop 注入(默认 en/zh-CN);强制更新阻塞、稍后提醒(engine.postpone)、错误重试(engine.retry)直接用 engine actions。

## Risks

- **R1 install/relaunch 时机**:`Update.install()` 完成后是否自动退出取决于 Tauri 版本;`install()` 末尾的 `relaunch()`(plugin-process)兜底。按 `@tauri-apps/plugin-updater` v2 实际行为对齐(单测 mock,真机验证留 app 集成)。
- **R2 namespace registryDependencies 解析**:registry item 里写 `@swarmhive/<name>`,依赖用户 `components.json` 配了 `@swarmhive`。docs 必须给两套模板(server / GitHub),否则用户 `shadcn add` 解析不到 namespace。
- **R3 build 产物提交**:`shadcn build` 产 `public/r/*.json` 须**提交进仓库**(GitHub raw 装的就是它);源码改了要记得 rebuild。可加 CI(release 时 rebuild + 提交)或 pre-commit 钩子兜底。无 server 同步问题(已不做 server `/r`)。
- **R4 adapter 单测覆盖**:adapter 依赖 @tauri-apps(平台),单测靠 mock plugin-updater;真实下载/安装/重启只能真机(SwarmDrop/SwarmNote 集成)验证——本 change 验收以"转换逻辑单测 + registry build + server serve"为准,端到端真机留 app 迁移。
