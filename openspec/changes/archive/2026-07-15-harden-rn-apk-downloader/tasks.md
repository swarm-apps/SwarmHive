# Tasks —— harden-rn-apk-downloader

> 1~4 在 SwarmHive 内(registry 上游);5 发布;6 跨仓(下游拉取)。
> 与 `add-download-source-preference` **无依赖**,可并行。

## 1. SDK:补 sizeBytes 归一化缺口

- [x] 1.1 [code] `packages/sdk/src/types.ts`:`ReleaseInfo` 加 `sizeBytes?: number`
      (注释说明来源 = RN wire 的 `size_bytes`,用于下载后尺寸校验)
- [x] 1.2 [code] `packages/sdk/src/check-update-android.ts` `normalizeAndroid`:映射
      `body.size_bytes` → `sizeBytes`
- [x] 1.3 [test] `normalizeAndroid` 单测:`size_bytes` 落到 `sizeBytes`;缺省时 undefined

## 2. 端口:可选期望值

- [x] 2.1 [code] `packages/registry-rn/registry/rn/lib/ports.ts`:新增
      `ApkDownloadExpectation { sizeBytes?: number }`;`ApkDownloader.download` 加**可选**
      第三参 `expected?`(可选 = 既有两参注入实现仍可赋值,向后兼容)
- [x] 2.2 [code] `rn-adapter.ts download()`:候选循环里把
      `{ sizeBytes: release.sizeBytes }` 传给 downloader

## 3. 下载器:回流 + 加固(核心)

- [x] 3.1 [code] `packages/registry-rn/registry/rn/lib/expo-downloader.ts`:从
      SwarmDrop-RN `src/lib/expo-downloader.ts` 回流 `getHeader` / `readTextPreview` /
      `assertApkDownload`(状态 + 非空 + ZIP magic + 失败删文件)
- [x] 3.2 [code] 同文件:`assertApkDownload` 加**尺寸校验**(下游那份也没有)——
      实际尺寸 ≠ `expected.sizeBytes` 则抛错;`expected` 缺省时跳过该层。
      **顺序:尺寸(O(1) getInfoAsync)在 magic(读字节)之前**
- [x] 3.3 [code] 同文件头注释:删掉「镜像 SwarmDrop-RN / SwarmNote-RN 生产下载器」——
      改为声明 registry-rn 是上游、双端从此拉取。**这行注释是漂移的成因,必须一并修**
- [x] 3.4 [code] `rn-adapter.ts:138-141` 的注释:sha256 fallback 那段自认未实现的说明,
      按 design D5 改写为"已撤销该要求 + 三条依据"的指向,不留悬空的 TODO 语气

## 4. 测试:堵住漂移的洞

- [x] 4.1 [test] `packages/registry-rn/test/expo-downloader.test.ts`(新文件):
      `vi.mock("expo-file-system/legacy")` + `vi.mock("react-native")`(Platform.OS="android"),
      驱动 `createExpoApkDownloader`
- [x] 4.2 [test] 用例矩阵:**200 + XML 响应体 → reject**(本次事故正面);非 2xx → reject;
      空文件 → reject;尺寸不符 → reject;合法 ZIP magic + 尺寸相符 → resolve;
      未传 `expected` → 跳过尺寸校验但其余照常;**失败时 deleteAsync 被调用**
- [x] 4.3 [test] `rn-adapter.test.ts` 补一个**真实形态**的 failover 用例:downloader 对主源
      URL 抛 "not an APK"(而非泛化 Error)、对 mirror URL 正常 → 落到 mirror。
      现有用例全是 mock 抛错,**留着但不算数**
- [x] 4.4 [test] **验收锚点**:注释掉 3.1 的 `assertApkDownload` 调用后,4.2 必须变红。
      不满足则说明测试没测到保护本身(design D6)

## 5. registry 发布

- [x] 5.1 [code] `pnpm --filter registry-rn build` → registry JSON 重建;确认新文件进产物
- [x] 5.2 [docs] `openspec/specs/registry-rn` 消费方文档:说明下载器现在会校验投递结果,
      以及注入自定义 downloader 时的契约(要么自行校验,要么复用 `assertApkDownload`)
- [x] 5.3 [code] 发 `@swarm-hive/sdk`(`sizeBytes`)+ 推 registry 到 main —— sdk **0.4.0**
      已上 npm(tag `sdk/v0.4.0`);registry-rn 是 `private: true`,不发 npm,合进 main 即
      分发(consumer 从 `raw.githubusercontent.com/.../main/packages/registry-rn/public/r/`
      拉)

## 6. 跨仓:下游拉取(SwarmDrop-RN)

- [x] 6.1 [code] SwarmDrop-RN:`@swarm-hive/sdk` `^0.1.0` → `^0.4.0`。跨三个 minor 但
      `ReleaseInfo` 只有增量字段,无 break;typecheck 通过
- [x] 6.2 [code] SwarmDrop-RN:从 registry 拉 `rn-adapter` / `expo-downloader` / `ports`。
      **memory 那条「shadcn CLI 在 Node 24 下挂」已过时** —— `npx shadcn@latest add
      @swarmhive-rn/rn-adapter` 在 Node v24.14.0 跑通。新踩的坑:①`--overwrite` 对已存在
      文件仍 **Skip**,要先 `rm`;②GitHub raw CDN 边缘缓存会让 shadcn 拉到旧版(curl 看到
      新的 ≠ shadcn 也看到);③**shadcn 剥掉「首行代码之前的所有注释」**(见 3.3 订正)
- [x] 6.3 [test] SwarmDrop-RN:拉取后与上游 diff —— `rn-adapter.ts` **完全一致**;
      `expo-downloader.ts` 仅差 import 尾逗号;`ports.ts` 少了首个 JSDoc(被 shadcn 剥,
      见上)。**"只允许 import 路径差异"这条验收标准写得过严**:两仓 biome `lineWidth`
      不同(SwarmHive 100 / SwarmDrop-RN 未设 = 80),拉取后跑本地 `biome check --write`
      必然产生换行差异 —— 逐字节一致做不到,判据应是"归一化空白后语义一致"
- [ ] 6.4 [test] SwarmDrop-RN 真机/模拟器验证:主源返回 XML 时,**failover 真正触发**并从
      mirror 完成更新。**未做** —— 且现在更难造:生产已配 GitHub 优先,主源不再失败,要复现
      得手动构造(如把 app 指到 `?source=oss` 入口)。**这仍是 failover 唯一没有被真实故障
      形态验证过的一环**;registry 的 vitest 已用 mock 的 expo-file-system 覆盖了「200+XML
      → 下载器 reject → adapter 落到 mirror」的完整链路,但那不等于真机
- [ ] 6.5 [code] SwarmNote-RN 同步拉取 —— **未做**,该仓是否在维护待确认
