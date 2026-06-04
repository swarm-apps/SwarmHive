# tasks

> 实施顺序:registry-web scaffold(1) → tauri-adapter(2) → use-update(3) → UI 组件(4) → registry.json + build(5) → server header client_id(6) → 测试(7) → docs(8)。
> registry-web 是 shadcn registry **源码包**(给用户 `shadcn add` 复制进项目);自身只需 typecheck + `shadcn build` 产 JSON,不需运行。
> 平台依赖(@tauri-apps/*, @radix-ui/*, lucide-react, @swarm-hive/sdk)作 registry item 的 `dependencies`(用户装)+ registry-web 的 devDep(本地 typecheck)。

## 1. packages/registry-web scaffold

- [x] 1.1 [code] `packages/registry-web/package.json`:name `@swarm-hive/registry-web`、`private: true`、devDep `shadcn` / `typescript` / `@types/react` / `react` / `@swarm-hive/sdk`(workspace) / `@tauri-apps/{api,plugin-updater,plugin-process,plugin-store}` / `@radix-ui/{react-dialog,react-progress}` / `lucide-react`;script `build:registry: shadcn build`、`typecheck: tsc --noEmit`
- [x] 1.2 [code] `tsconfig.json`(继承 `../../tsconfig.base.json`,`paths` 镜像消费者 alias `@/lib`/`@/hooks`/`@/components` → registry 源码树,本地 typecheck 可解析跨 item import,include `registry`)
- [x] 1.3 [code] `components.json`(shadcn 配置:style、aliases `@/components`/`@/lib`/`@/hooks`、`registries.@swarmhive` 指向 GitHub raw)
- [x] 1.4 [code] `registry.json` 骨架:`$schema`、`name: "swarmhive"`、`homepage`、`items: []`(后续填)

## 2. tauri-adapter(registry:lib,D1/D2/D3)

- [x] 2.1 [code] `registry/tauri/lib/tauri-adapter.ts`:`createTauriAdapter(opts?: {storeFileName?, checkOptions?}) -> UpdateAdapter`(endpoint 在 `tauri.conf.json` 配,adapter 不收 baseUrl/appSlug)
- [x] 2.2 [code] **check**:`check({ headers: { 'X-Client-Id': ctx.clientId } })`(验签 + header 传 client_id 让 server 灰度生效)→ 缓存 `Update` 进闭包 → 从 `update.rawJson.swarmhive` 取 `{upgrade_type,min_version,rollout_percent,channel}` + `update.{version,body,date}` 转 `ReleaseInfo`
- [x] 2.3 [code] **download**:cached `Update.download(onEvent)`(plugin-updater **支持单独** download);`DownloadSpeedTracker`(500ms 节流,**本文件**,搬自 SwarmDrop)把 `Started{contentLength}`/`Progress{chunkLength}`/`Finished` 转 SDK `Progress`
- [x] 2.4 [code] **install**:`Update.install()` + `relaunch()`(@tauri-apps/plugin-process);**storage**:`@tauri-apps/plugin-store`(LazyStore)包装成 `KeyValueStorage`;**compare**:`semverComparator`(来自 @swarm-hive/sdk)
- [x] 2.5 ~~`getTarget()`/`getArch()`~~ **N/A**:用 plugin-updater `check()`(从 `tauri.conf.json` endpoint 的 `{{target}}`/`{{arch}}`/`{{current_version}}` 占位自拼 URL),无需 adapter 提供 target/arch;当前版本由 `createSwarmHiveEngine` 经 `@tauri-apps/api/app` `getVersion()` 取(见 3.1)

## 3. use-update(registry:hook,D4)

- [x] 3.1 [code] `registry/tauri/hooks/use-update.ts`:`UpdateEngineContext` + `useUpdate()`(从 context 取 engine,返回 `useUpdateEngine(engine)`,缺 provider 则 throw);`createSwarmHiveEngine(opts?)` = `createUpdateEngine(createTauriAdapter(opts), {currentVersion: getVersion(), clientId: ensureClientId(storage)})`(异步装配)

## 4. UI 组件(registry:component,D6;Tailwind v4 + @radix-ui + lucide-react)

- [x] 4.1 [code] `update-provider`:`createSwarmHiveEngine()` 异步装配 + React context;`<UpdateProvider fallback checkOnMount recheckOnFocus>`(endpoint 在 tauri.conf,无需 app/channel/baseUrl props)
- [x] 4.2 [code] `prompt-update-dialog`:Dialog + Loader2/Download/FileText;`open`/`onOpenChange`/`releaseNotesRenderer?`/`currentVersion?`;稍后(`postpone`)/立即更新(`download`)+ ready 自动 install
- [x] 4.3 [code] `force-update-dialog`:Dialog trap(`onPointerDownOutside`/`onEscapeKeyDown` preventDefault,不可关);status `force-required` + ready 自动 install
- [x] 4.4 [code] `update-progress-dialog`:Progress + 百分比(`percent*100`)+ 速度(MB/s);缺省按 status 自动显示
- [x] 4.5 [code] `update-settings-section`:检查/下载按钮(`check(true)`/`download`)+ 状态描述 + 进度 banner + error 重试(`retry`)
- [x] 4.6 [code] `release-notes-view`:`renderer?` slot 吸收 Markdown / 纯文本(缺省纯文本 + whitespace-pre-wrap)
- [x] 4.7 [code] 文案 prop 注入:`lib/update-texts.ts` 的 `resolveUpdateTexts(locale, overrides)`,en / zh-CN 预设,组件 `locale?`/`texts?` 注入

## 5. registry.json + build(D4)

- [x] 5.1 [code] `registry.json` 9 items:每项 `name`/`type`/`title`/`description`/`files[].{path,type}`/`registryDependencies`(namespace `@swarmhive/<name>` + canonical `dialog`/`button`/`progress`/`utils`)/`dependencies`(npm)。vendored `ui/*` + `lib/utils.ts` **不列 item**(消费者拿 canonical)
- [x] 5.2 [code] registryDependencies 图:UI 组件 → `use-update` → `tauri-adapter`,组件 → `release-notes-view`/`update-texts`;npm deps 按 item 分配(adapter 带 @tauri-apps/*+sdk、hook 带 sdk+@tauri-apps/api、组件带 lucide-react)
- [x] 5.3 [code] `pnpm --filter @swarm-hive/registry-web build:registry` 跑通 → `public/r/registry.json` + 9 item JSON;`files[0].content` 已 inline(prompt-update-dialog 3759 chars 验证)

## 6. server endpoint 读 header client_id(D3)

> 分发改 GitHub raw(design D5),**不做 server `/r` host** —— 本节只剩 endpoint 的 header 改动。

- [x] 6.1 [code] **`routes/updates.rs` 的 `tauri` handler 取 client_id 改三级**:header `X-Client-Id` → query `client_id` → IP(`forwarded_ip`)。让 Tauri(plugin-updater 运行时只能传 header)的灰度在 server 端生效;不动响应格式
- [x] 6.2 [test] 回归:`update_check_tauri_smoke::rollout_via_x_client_id_header`(header client-0→bucket 2 命中 200、client-9→bucket 63 未命中 204;header==query 同语义;header 优先于 query 双向验证)—— 1 passed

## 7. 测试

- [x] 7.1 [test] tauriAdapter 单测(vitest,mock `@tauri-apps/plugin-updater`/`plugin-process`/`plugin-store`):check 从 `rawJson.swarmhive` 转 `ReleaseInfo`(force/未知→prompt)+ X-Client-Id header、无更新→null、download 的 `DownloadEvent`→`Progress`(末值 percent 1)、install 调 `install()`+`relaunch`、compare=semverComparator —— 6 passed
- [x] 7.2 [test] registry build 验证(`test/registry-build.test.ts`):读 `public/r`,断言 9 items + `prompt-update-dialog.content` 非空 + `registryDependencies` 含 `@swarmhive/use-update`/`dialog` + `use-update`→`@swarmhive/tauri-adapter`
- [x] 7.3 [test] no-sdk-duplication 守护:`grep -rnE "createStore|blake3|semver\.(gt|valid)|zustand" packages/registry-web/registry` 为空(状态机/comparator/rollout 全 import 自 @swarm-hive/sdk)

## 8. docs / openspec 同步

- [x] 8.1 [docs] `docs/14-sdk-ui.md`:接入流程换 namespace `@swarmhive`(GitHub raw + `components.json` 模板,UpdateProvider 去掉 app/channel/baseUrl)、组件清单对齐 6 真实组件 + props/slot、Tauri/RN 差异表改 download/install 拆开、Registry host 段改 GitHub raw 分发
- [x] 8.2 [docs] `dev-notes/knowledge/architecture.md`:补 tauriAdapter 职责(plugin-updater check 验签 + rawJson.swarmhive 归一化 + header X-Client-Id 灰度 + download/install 拆开 + ready 自动 install)、registry 分发(GitHub raw,vendored ui 不分发,public/r 进 biome ignore);删 server `/r` fallback 说法
- [x] 8.3 [docs] `openspec/changes/README.md`:阶段 6 + SDK 层链标注 add-update-sdk-core ✅ / add-registry-web-tauri ✅,分发改 GitHub raw
- [x] 8.4 [code] 质量门全绿:`pnpm lint`(132 files clean)+ registry-web `typecheck` + `test`(9 passed)+ `build:registry`(幂等)+ `cargo fmt --check` + `cargo check -p swarmhive-server` + `cargo test ...rollout_via_x_client_id_header`(1 passed)

## 跨 proposal 联动

- [x] 9.1 标注 `add-registry-rn`(后续)将复刻本结构:rnAdapter(APK 下载 + PackageInstaller + sha256)+ registry-rn(NativeWind + @rn-primitives),消费同一 `@swarm-hive/sdk` engine。**实现 tauriAdapter 未发现 `UpdateAdapter` 接口缺字段**(check/download/install/storage/compare 五件套够用,`DownloadHandle.payload` 缓存平台 Update 实例;RN 复刻时若需调整再改 SDK ports)
