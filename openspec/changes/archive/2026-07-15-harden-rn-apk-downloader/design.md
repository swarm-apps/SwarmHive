# Design —— harden-rn-apk-downloader

## D1. 分发拓扑:把倒置的上下游正过来

### 现状(漂移的成因)

```text
   SwarmDrop-RN/src/lib/expo-downloader.ts     ← 事实上的 source of truth
     assertApkDownload ✅                         (校验只长在这里)
              │
              │  「镜像」——— 手工、单向、无回流机制
              ▼
   registry-rn/registry/rn/lib/expo-downloader.ts
     assertApkDownload ❌                       ← 上游反而缺保护
              │
              │  shadcn registry 分发
              ▼
        任何新装配的 app  ← 拿到的是缺保护的版本:XML 静默写盘 → 喂给 PackageInstaller
```

头注释「镜像 SwarmDrop-RN / SwarmNote-RN 生产下载器」把箭头指反了 —— 上游声称自己是下游的
镜像。于是下游的加固**没有义务**回流,上游的缺失**没有人**负责。

### 目标

```text
   registry-rn/registry/rn/lib/expo-downloader.ts   ← 唯一 source of truth
     assertApkDownload + 尺寸校验 ✅
     vitest 覆盖(mock expo-file-system)✅          ← 漂移的防线
              │
              │  shadcn registry 分发(推 main,双端拉)
              ├──────────────────────┬──────────────────────┐
              ▼                      ▼                      ▼
       SwarmDrop-RN           SwarmNote-RN            任何新 app
    src/lib/expo-downloader.ts    (同)                   (同)
       与上游无实质 diff
```

**回流一次,分发多次。** 下游不再各自演化下载器 —— 要改就改上游,再拉。

## D2. 校验链:每一层拦的是什么

```text
   downloadAsync() resolve
            │
            ▼
   ┌─────────────────────┐
   │ status ∈ [200,300)? │──否──▶ throw(带 status + content-type + 响应体前 160 字符)
   └──────────┬──────────┘         ↳ 拦:403/404 错误响应
              ▼
   ┌─────────────────────┐
   │ 文件存在且 ≥ 4 字节? │──否──▶ throw
   └──────────┬──────────┘         ↳ 拦:空响应
              ▼
   ┌─────────────────────┐
   │ 尺寸 == sizeBytes?  │──否──▶ throw        ★ 本 change 新增
   └──────────┬──────────┘         ↳ 拦:截断下载(ZIP magic 仍合法,只有尺寸能发现)
              │  (expected 缺省时跳过)
              ▼
   ┌─────────────────────┐
   │ 首 4 字节 == "UEs"? │──否──▶ throw        ★ 本次生产事故的正面
   └──────────┬──────────┘         ↳ 拦:200 + XML/HTML 错误页(阿里云 OSS 匿名 APK 受限)
              ▼
        resolve(uri)
                                    任一 throw 前:先 deleteAsync(uri) —— 不留毒化缓存
                                    任一 throw 后:rn-adapter 捕获 → 试下一个候选源
```

**为什么 200 + XML 是最危险的一层**:它同时骗过了 `downloadAsync`(不抛)与 status 检查
(是 2xx),只有内容检查能识破。而这恰恰是阿里云 OSS 的实际行为 —— 也是 registry 版本
唯一缺的那一层。

**尺寸校验放在 magic 之前**:尺寸是 O(1) 的 `getInfoAsync`,magic 要读文件;先便宜后贵。

## D3. 端口边界:校验归下载器,不归 adapter

```text
   rn-adapter.download(release, onProgress)          纯逻辑,可 vitest 直测
     │  candidates = [release.url, ...mirrorUrls]
     │  for url of candidates:
     │     try { downloader.download(url, cb, { sizeBytes: release.sizeBytes }) }
     │     catch { lastErr = e; continue }           ← failover 在这里
     ▼
   ApkDownloader(注入端口)
     └─ createExpoApkDownloader   ← expo-file-system / react-native 只在这一侧
          └─ assertApkDownload    ← 校验在这里
```

`ports.ts` 的既有约束原文:「让 adapter 本体可纯逻辑单测(不碰 expo-* 真实实现)」。
校验要读文件字节 —— 那是 expo-* 的活。放进 adapter 会把 expo 依赖拖进纯逻辑层,毁掉这条
约束;放进下载器则天然内聚:**下载器的职责本就是"产出一个可用的本地 APK",而不是
"产出一个文件"**。

期望值经参数传入(而非下载器自己去查),因为只有 adapter 手上有 `ReleaseInfo`。

## D4. SDK:`sizeBytes` 的归一化缺口

wire 有、`ReleaseInfo` 没有:

```ts
// AndroidUpdateResponse(wire)        →  ReleaseInfo(SDK 契约)
size_bytes: Some(art.size_bytes)      →  ❌ 丢失            ★ 本 change 补
sha256: Some(art.sha256)              →  signature ✅(RN 用 sha256 占 signature 槽)
mirror_urls                           →  mirrorUrls ✅
```

补 `sizeBytes?: number` + `normalizeAndroid` 映射 `body.size_bytes`。可选字段,非破坏性。

## D5. sha256 的取舍(修订既有 spec)

归档 spec 要求「post-download sha256 mismatch SHALL 触发 fall-through」。本 change 撤销该
要求,依据三条:

1. **无廉价实现路径**(已验证):`expo-file-system` 只有 md5(原生流式);`expo-crypto` 的
   `digestStringAsync` 要把整个 APK 当 JS 字符串传入(50MB → ~67MB base64,OOM 风险);
   原生流式哈希库违背 registry-rn「no native code」原则。
2. **与服务端 gate 冗余**:`github-release-source` spec 已要求服务端在**暴露前**校验镜像
   digest 与 `artifact.sha256` 一致(一次、缓存、单飞)。digest 不符的镜像根本不会成为候选。
   客户端重算 = 在最贵的地方重做一遍更弱的检查。
3. **残余风险已被覆盖**:传输损坏 → 尺寸/magic 拦下;APK 真伪 → Android PackageInstaller
   验签(`ReleaseInfo.signature` 注释原文:「APK 真伪由 Android 安装器验签兜底」)。

被撤销要求所举的场景「same-repo but wrong-bytes asset」,正是服务端 digest gate 的靶心 ——
它在 D5.2 的意义上**已经被实现了**,只是实现在服务端。撤销的是"在客户端再做一遍",不是
"不做"。

保留一个字符串比较是可行的(`signature` 槽已有 sha256),但没有字节可比 —— 我们算不出本地
文件的 sha256,这才是死结。

## D6. 测试策略:堵住漂移长出来的那个洞

漂移能发生,是因为 `expo-downloader.ts` 头注释写着「本仓 vitest 不跑它」—— 一个没有测试的
文件,和一个声称自己是下游镜像的注释,合起来就是无人负责。

本 change 让它可测:vitest 里 `vi.mock("expo-file-system/legacy")` + `vi.mock("react-native")`,
直接驱动 `createExpoApkDownloader`。

**验收锚点**:新增测试必须能让「删掉 `assertApkDownload` 调用」的改动**变红**。这是判断
测试是否真的在防漂移(而非只是覆盖行数)的唯一标准 —— 现有那套 mock-downloader-抛错的
failover 测试就通不过这个锚点。
