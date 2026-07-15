# harden-rn-apk-downloader

> 把 APK 下载校验从下游 app 回流进 registry-rn,修正上下游倒置导致的漂移;并诚实修订
> `add-github-release-source` 里一条**从未实现、且在 Expo 中无廉价实现路径**的 sha256 要求。

## Why

### 1. registry 的下载器缺了保护,而下游 app 有 —— 上下游反了

`packages/registry-rn/registry/rn/lib/expo-downloader.ts` 只做:

```ts
const result = await resumable.downloadAsync();
if (!result?.uri) throw new Error("Download produced no file");
return result.uri;
```

**它不检查 HTTP 状态、不检查文件内容。** `createDownloadResumable` 对 4xx/5xx **不抛错** ——
它会把错误响应体(阿里云 OSS 的 XML)照常写进 `swarmhive-update.apk` 并正常 resolve。于是
按 registry 装配的 app 会把一个 XML 文件交给系统 PackageInstaller,用户看到「解析包时出现
问题」—— **既没有报错线索,也不会触发 rn-adapter 的 mirror failover**(failover 只在下载器
抛错时才走)。

而下游的 SwarmDrop-RN `src/lib/expo-downloader.ts` **有** `assertApkDownload`:查 HTTP 状态、
查文件非空、查首 4 字节 ZIP magic(`UEs` = `PK`),失败则删文件并抛错。

`add-github-release-source` 的 proposal / tasks 反复把它称作「**既有** `assertApkDownload`」
并据此声称 failover 的换源触发点已覆盖错误页 —— 但那个函数**从来只存在于下游**,registry
里一行都没有。tasks 8.2 因此是在一个不成立的前提上被勾掉的。

**根因是上下游倒置**:registry 那个文件的头注释写着「镜像 SwarmDrop-RN / SwarmNote-RN
生产下载器」—— 把下游 app 当成了 source of truth。正确方向是 **registry-rn 是上游,双端从
它拉取**。倒置的注释既是漂移的成因,也是它没被发现的原因。

### 2. 测试测的是不会发生的场景

`test/rn-adapter.test.ts` 的 failover 用例全部 mock 一个**主动抛错**的 downloader:

```ts
it("主源抛错 → 回退到 mirrorUrls 备用源", ...)
```

而真实的 OSS 失败**恰恰不抛错**(见上)。所以这套测试全绿、却对真实故障零覆盖 ——
它验证的是 adapter 的分支逻辑,而漏掉的保护在被完全 mock 掉的 downloader 里。
`expo-downloader.ts` 的头注释自认「本仓 vitest 不跑它」,漂移正是从这个测试盲区长出来的。

### 3. 一条无法兑现的 spec 要求

`add-github-release-source` 归档的 `update-sdk-core` spec 要求:

> A "download failure" that triggers fall-through SHALL include ... AND a post-download
> `sha256` mismatch against the expected value

`rn-adapter.ts:140` 的注释自认没做。**这不是遗漏,是 Expo 里没有廉价实现路径**(已验证):

| 途径 | 结论 |
| --- | --- |
| `expo-file-system` `getInfoAsync({ md5: true })` | 原生流式,但**只有 md5,没有 sha256** |
| `expo-crypto` `digestStringAsync(SHA256, str)` | 一次性 API,要把整个文件当 JS 字符串传入 —— 50MB APK ≈ 67MB base64 字符串堆在 JS 堆里,OOM 风险 |
| `react-native-quick-crypto` 等原生流式哈希 | 违背 registry-rn 既有原则「install via ACTION_VIEW **with no native code**」 |

更关键的是:**这条要求与一个已存在的服务端 gate 冗余**。`github-release-source` spec 已经
要求「Server SHALL verify a GitHub mirror's liveness and digest before exposing it」——
digest 不符的 GitHub 镜像**根本不会被暴露成候选**。客户端再算一次 sha256,是在设备上、
用最贵的方式、重做一遍服务端已经做过且做得更好(一次、缓存、暴露前)的事。

而它没覆盖到的残余风险(OSS 字节错误、传输中损坏),由**尺寸 + ZIP magic + Android 安装器
验签**兜住 —— 后者才是 APK 真伪的真正闸门(`ReleaseInfo.signature` 的注释早已写明
「APK 真伪由 Android 安装器验签兜底」)。

因此本 change **诚实修订该要求**,而不是继续挂着一条勾了 `[x]` 却没实现的 spec。

## What Changes

### 1. `assertApkDownload` 回流进 registry-rn,并补尺寸校验

把下游那份校验搬进 `packages/registry-rn/registry/rn/lib/expo-downloader.ts`,并加一层
**期望尺寸**校验(下游那份也没有):

- HTTP 状态非 2xx → 抛错(带状态码 + content-type + 响应体前 160 字符,便于定位)
- 文件不存在 / < 4 字节 → 抛错
- **实际尺寸与期望 `sizeBytes` 不符 → 抛错**(拦截截断下载:连接中断后 `downloadAsync`
  会 resolve 出一个残缺文件,ZIP magic 仍然合法,只有尺寸能发现)
- 首 4 字节非 ZIP magic(`UEs`)→ 抛错(拦截 XML/HTML 错误页 —— **本次生产事故的正面**)
- 任一失败:删除残留文件后再抛(不留毒化缓存给下次 resume)

抛错即触发 `rn-adapter` 的既有 mirror failover —— **保护与 failover 由此第一次真正接上**。

### 2. `ApkDownloader` 端口加可选期望值

```ts
export interface ApkDownloadExpectation {
  /** 期望字节数(来自 update 响应 size_bytes)。用于拦截截断下载。 */
  sizeBytes?: number;
}
export interface ApkDownloader {
  download(url, onProgress, expected?: ApkDownloadExpectation): Promise<string>;
}
```

参数可选 → 既有注入实现(两参)在 TS 下仍可赋值,**向后兼容**。

校验放**下载器**而非 adapter,是为守住 `ports.ts` 立的架构约束:「让 adapter 本体可纯逻辑
单测(不碰 expo-* 真实实现)」。文件读取/哈希是 expo-* 的活,adapter 保持纯逻辑。

### 3. SDK:`ReleaseInfo` 补 `sizeBytes`

wire 的 `AndroidUpdateResponse.size_bytes` 当前**没有**被 `normalizeAndroid` 归一化进
`ReleaseInfo`(`signature` 槽拿了 `sha256`,尺寸被丢了)。补上,adapter 才能把期望尺寸
喂给下载器。

### 4. 修正上下游方向

- 删掉 registry `expo-downloader.ts` 头注释里「镜像 SwarmDrop-RN / SwarmNote-RN 生产下载器」
  的表述,改为声明 **registry-rn 是上游,双端从此拉取**。
- 下游 SwarmDrop-RN / SwarmNote-RN 改为从 registry 拉取该组件(跨仓,见 tasks 6)。

### 5. 补上能抓住这次漂移的测试

vitest 里 mock `expo-file-system/legacy` + `react-native`,直接测 `createExpoApkDownloader`:
200 + XML → reject、非 2xx → reject、尺寸不符 → reject、合法 ZIP → resolve、失败时删文件。
**这是本 change 里唯一能防止漂移复发的东西** —— 缺了它,回流的代码下次照样会被改漂。

## Capabilities

| 能力 | 变更 |
| --- | --- |
| `registry-rn` | MODIFIED —— 下载器 SHALL 在返回前校验投递结果;registry 为上游 |
| `update-sdk-core` | MODIFIED —— `ReleaseInfo` 携带 `sizeBytes`;**修订** sha256-mismatch 触发要求 |

## Impact

- **修的是"未来的坑"**:本 change **不修复当前生产事故** —— SwarmDrop-RN 装的 SDK 0.1.0
  连 failover 都没有,且它自己那份 `assertApkDownload` 已经在正常报错。生产的止血由
  `add-download-source-preference` 完成(服务端翻转缺省源,存量客户端零改动)。本 change 的
  价值是:**任何按 registry 装配的新 app 不再静默把 XML 喂给安装器**,以及双端拿到真正
  接上的 failover 韧性。
- **registry 消费方需重新拉取**才能受益(shadcn registry 分发模型固有)。
- **无破坏性变更**:端口新增参数可选;`ReleaseInfo` 新增字段可选。

## Non-goals

- **客户端 sha256 校验**。见 Why 3 —— 无廉价路径、与服务端 digest gate 冗余、真伪由 Android
  验签兜底。若将来 Expo 提供原生流式 SHA-256(或 `getInfoAsync` 支持 sha256),可另提 change
  重新引入。
- **给 OSS 也加服务端 digest gate**。当前 gate 只覆盖 GitHub 镜像;OSS 字节是自己上传的,
  信任模型不同。要做另提。
- **服务端源顺序配置**。见 `add-download-source-preference`,独立 change,无依赖。
- **registry-web-tauri 的对应加固**。Tauri 侧下载/校验走 minisign,链路不同,不在本 change。

## Depends on

- `archive/2026-07-12-add-github-release-source` —— 本 change 修正其 tasks 8.2 的不成立前提
  (「既有 `assertApkDownload`」在 registry 中并不存在),并修订其 `update-sdk-core` spec 中
  一条无法兑现的要求。

## Acceptance

1. registry 的 `createExpoApkDownloader` 拿到 200 + XML 响应体 → **reject**,且残留文件被删除。
2. 同上,`rn-adapter` 因此 fall through 到 `mirrorUrls` 候选并成功 —— failover 首次在
   **真实**故障形态(而非 mock 抛错)下被验证。
3. 尺寸与 `sizeBytes` 不符的截断文件 → reject。
4. 合法 APK(ZIP magic + 尺寸相符)→ resolve 出本地路径,行为与本 change 前一致。
5. 未传 `expected` 时不做尺寸校验,其余校验照常(向后兼容)。
6. SwarmDrop-RN 从 registry 拉取后,`src/lib/expo-downloader.ts` 与上游**无实质 diff**。
