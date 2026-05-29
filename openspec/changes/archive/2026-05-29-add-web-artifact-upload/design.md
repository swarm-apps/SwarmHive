# design — add-web-artifact-upload

跨 crate(api-types + server + admin),按项目约定画数据流图。聚焦传输方式、客户端 hash、`.sig` 落库、CORS 端点四个决策。

## Context

- presign / complete 契约与 `Storage` trait 已由 `add-storage-and-presign-upload` 落地,CLI publish 在用。浏览器要做的是"当一个非 CLI 的 client"复用同一契约。
- 与 CLI 的两个本质差异:① 浏览器没有本地 hash 工具,要在浏览器内算 `Content-MD5` 需要的 MD5 + 可选 SHA256;② 浏览器跨域 PUT 到对象存储,桶必须配 CORS(代码库当前完全没有)。
- `artifact.signature_metadata` 列已存在但 `upsert_artifact` 恒写 `Set(None)`([service.rs:212](../../../crates/swarmhive-server/src/routes/uploads/service.rs#L212))——Tauri `.sig` 无处落地。
- 宿主:`ArtifactsDrawer`(releases 页,只读)+ storage 页(backend CRUD)。

## Goals / Non-Goals

**Goals:**
- Admin 网页完成"建/选 draft → 传产物 → 发布 → promote"全链路,与 CLI 同源(复用 presign/complete,零字节中转)。
- 大文件(数百 MB)算 hash 不卡 UI;逐文件进度。
- Tauri `.sig` 写入 `signature_metadata`,为后续 updater manifest 备好数据。
- 一键给当前 backend 配 CORS,RustFS / 标准 S3 可用;OSS 给明确手动回退。

**Non-Goals:**
- 断点续传 / multipart 分片;独立 wizard 页;改 download / manifest;改 CLI;OSS 原生 CORS 自动化。

## 数据流(浏览器 ↔ server ↔ 对象存储)

```text
  Admin: ArtifactsDrawer "上传产物"
  ┌──────────────────────────────────────────────────────────────────────┐
  │ 1. 拖拽多文件 → 自动分类(platform/target/abi) + .sig 与 bundle 配对    │
  │ 2. hash.worker.ts (hash-wasm 流式)：每文件 → {md5_hex, sha256_hex}      │
  │    .sig 文件不进上传集；读其文本，挂到对应 bundle 的 signature          │
  └───────────────┬──────────────────────────────────────────────────────┘
                  │ POST presign  { files:[PresignFile{md5,sha256,platform,...}] }
                  ▼
  server routes/uploads::presign ── plan_part → presign_put(Content-MD5[+sha256])
                  │ 200 PresignResponse { upload_id, parts:[{url, headers}] }
                  ▼
  ┌──────────────────────────────────────────────────────────────────────┐
  │ 3. 逐文件 PUT <presigned_url>  body=File  headers=part.headers(原样回放)│  ──直传──▶  S3 / RustFS / OSS
  │    需桶 CORS 放行 admin 源(见下"CORS 端点")；进度条用 XHR upload.onprogress│
  └───────────────┬──────────────────────────────────────────────────────┘
                  │ POST complete  { parts:[CompletePart{object_key,sha256,signature?}], publish }
                  ▼
  server routes/uploads::complete ── HeadObject 校验 size/checksum
                  │  upsert_artifact 写 signature_metadata ← part.signature
                  │  publish=true 且持 release:publish → 发布
                  │ 200 CompleteResponse { release_id, status, endpoints }
                  ▼
  ┌──────────────────────────────────────────────────────────────────────┐
  │ 4. 可选 promote：POST /channels/{name}/promote { version }(release:promote)│
  └──────────────────────────────────────────────────────────────────────┘

  旁路(storage 页一次性):
  Admin "配置 CORS" ──POST /storage/backends/{id}/cors {allowed_origins:[window.origin]}──▶
        server::put_cors → aws-sdk-s3 put_bucket_cors  → {ok, detail}（OSS 失败=ok:false+手动指引）
```

## Decisions

### D1. 传输:复用 presign 直传(不走 server 中转)

浏览器 PUT 直传对象存储,复用既有 presign/complete。**Why over server 中转**:① 零后端上传逻辑改动(中转要新写流式 endpoint);② 与 CLI 同源;③ server 不进数据路径,契合架构硬约束「不中转字节」。代价(客户端 hash + CORS)由 D2/D4 解。

### D2. 客户端 hash:`hash-wasm` + Web Worker 流式

presign 要求**先**算好 hex MD5(`Content-MD5` 必绑)+ SHA256(后端支持时绑)。WebCrypto 无 MD5 且 `SubtleCrypto.digest` 不支持流式(需整文件进内存)。选 `hash-wasm`(WASM,`createMD5()`/`createSHA256()` 增量 update),在 Web Worker 里按 ~8MB `Blob.slice` 分块喂,主线程不阻塞。**备选**:`spark-md5`(MD5)+ WebCrypto(SHA256)——两套 API、SHA256 仍非流式,弃。

### D3. `.sig` 内联进 complete,不作独立对象

Tauri `.sig` 是一小段 base64 文本。浏览器读其文本,挂到同名 bundle 的 `CompletePart.signature`;server `upsert_artifact` 写 `signature_metadata = Some(json!({ "tauri_signature": <sig> }))`。**Why**:① `.sig` 不该作为可下载产物占一行 artifact;② 与 Tauri `latest.json` 内联签名的形态一致;③ 不影响对象键 / presign,只在 complete 末端落库。**配对规则**:`Foo.app.tar.gz` ↔ `Foo.app.tar.gz.sig`(bundle 名 + `.sig`)。孤立 `.sig`(无对应 bundle)→ 前端校验报错,不上传。

### D4. CORS 端点:origin 由客户端给,`put_bucket_cors` 写桶

新增 `POST /api/v1/storage/backends/{id}/cors`(`storage:manage`),body `{ allowed_origins: string[] }`。前端按钮传 `[window.location.origin]`——浏览器自己的源正是要放行的。server 构建该 backend 的 `S3Storage` 调 `put_cors`:`AllowedMethods=[PUT,GET,HEAD]`、`AllowedHeaders=["*"]`、`ExposeHeaders=["ETag"]`、`AllowedOrigins=req`。**Why 让客户端给 origin**:嵌入式部署 server 不一定知道公网回源域名;客户端 origin 最权威。返回 `{ ok, detail }`。

### D5. 平台自动分类(纯函数,可单测)

`classifyArtifact(filename) → { platform, abi?, target? }`:`.apk`→`react-native-android`(从 `arm64-v8a`/`armeabi-v7a`/`x86_64` 子串取 abi);`.msi/.exe/.dmg/.app.tar.gz/.AppImage/.deb/.rpm/.nsis.zip`→`tauri-desktop`(target triple 留空,用户可填);`.sig`→标记为签名(配对,不上传)。未知扩展名→默认 `tauri-desktop` + 标黄让用户确认。结果在表格里可改。

## Risks / Trade-offs

- [OSS S3-兼容层不支持 `PutBucketCors`] → D4 返回 `ok:false` + detail 指向控制台手动配;docs/07 给 OSS CORS 规则范例。直传本身不受影响(只要桶配好)。
- [大文件浏览器内存] → 用 `Blob.slice` 分块 hash + PUT body 直接给 `File`(浏览器流式发送,不整体读进内存);不做 multipart,单 PUT 上限受对象存储单 PUT 大小限制(S3 5GB),够覆盖桌面/APK 包。
- [presign headers 含 `Content-MD5`,fetch 跨域触发 preflight] → CORS `AllowedHeaders=["*"]` 放行;用 `XMLHttpRequest` 拿 `upload.onprogress`(fetch 无上传进度)。
- [客户端 hash 可被篡改] → 不是信任边界:server complete 时 HeadObject 复核 size + 存储侧校验和(写入时 `Content-MD5` 已强制),与 CLI 同等保证。
- [.sig 配对靠文件名约定] → 命名不符时前端显式报错而非静默丢签名。

## Migration Plan

纯增量,无 DB schema 变更(`signature_metadata` 列已在)。`CompletePart.signature` 用 `#[serde(default)]` 向后兼容——CLI 不传该字段,行为不变。CORS 端点 + 按钮新增,不影响既有路径。回滚 = 撤掉端点/前端入口,已写入的 `signature_metadata` 无害保留。

## Open Questions

- 无阻塞性问题。`signature_metadata` 的 JSON 形态先定 `{ "tauri_signature": <sig> }`,待 `add-update-check-tauri` 消费时若需更多字段再扩(加键,不破坏)。
