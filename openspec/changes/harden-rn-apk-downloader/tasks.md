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
- [ ] 5.3 [code] 发 `@swarm-hive/sdk`(`sizeBytes`)+ 推 registry 到 main

## 6. 跨仓:下游拉取(SwarmDrop-RN)

- [ ] 6.1 [code] SwarmDrop-RN:`@swarm-hive/sdk` `^0.1.0` → 最新(拿到 `mirrorUrls` +
      `sizeBytes`)。**注意 0.1.0 → 0.3.x 是跨两个 minor,需核对 ReleaseInfo 其余字段有无
      break**
- [ ] 6.2 [code] SwarmDrop-RN:从 registry 拉 `rn-adapter` / `expo-downloader` / `ports`。
      **memory 记录 shadcn CLI 在 Node 24 下挂 → 失败则 cp registry 源兜底**
- [ ] 6.3 [test] SwarmDrop-RN:拉取后 `src/lib/expo-downloader.ts` 与上游 diff 应无实质差异
      (只允许 import 路径差异)—— 这是"下游是镜像"的可验证形式
- [ ] 6.4 [test] SwarmDrop-RN 真机/模拟器验证:主源返回 XML 时,**failover 真正触发**并从
      mirror 完成更新。**这是 failover 第一次在真实故障形态下被端到端验证** —— 此前
      registry 的 mirror failover 从未在生产链路上真正跑通过
- [ ] 6.5 [code] SwarmNote-RN 同步拉取(若在维护中)
