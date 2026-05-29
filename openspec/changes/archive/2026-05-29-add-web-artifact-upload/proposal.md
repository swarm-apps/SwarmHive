# add-web-artifact-upload

## Why

CLI 是一等发布入口,但 Admin 至今只能**查看**产物(`ArtifactsDrawer` 只读)。不想接 CI、只想点几下网页发版的用户(尤其首次试用、手动出包的小团队)被迫装 CLI。本提案把 `swarmhive publish` 的核心链路搬进 Admin:浏览器直传产物到对象存储,复用既有 presign / complete 契约,做到与 CLI 同源、零字节中转。

## What Changes

- **浏览器 presign 直传**:Admin `ArtifactsDrawer` 增加"上传产物"入口——拖拽多文件 → Web Worker 用 `hash-wasm` 流式算 MD5 + SHA256(大文件不卡 UI)→ `presign` → 逐文件 `PUT` 直传 S3/RustFS/OSS(回放 `Content-MD5` 头 + 进度)→ `complete`(可勾选发布)→ 可选 promote 到某 channel。
- **自动平台分类**:从文件名 / 扩展名推断 `platform` / `target` / `abi`(`.apk`→Android、`.msi/.dmg/.AppImage/...`→Tauri),用户可改。
- **Tauri `.sig` 签名**:`.sig` 与同名 bundle 配对——客户端读 `.sig` 文本,在 `complete` 时随对应 part 上送,server 写入 artifact 的 `signature_metadata`(当前恒为 `null`,是真缺口)。`.sig` 本身不作为独立对象上传。
- **一键 CORS 配置**:storage 页加"配置 CORS"按钮 → 新端点 `POST /storage/backends/{id}/cors` 调 `put_bucket_cors` 把 admin 源写进桶 CORS(直传的前置)。OSS S3-兼容层不支持时回退手动文档。

## Capabilities

### New Capabilities
- `web-artifact-upload`: Admin 浏览器端产物上传流程——拖拽 + 客户端 hash + presign 直传 + complete + 自动平台分类 + `.sig` 配对 + 上传后 promote channel,以及 storage 页的 CORS 配置入口。

### Modified Capabilities
- `storage-and-presign-upload`: `complete` 的 `CompletePart` 增加可选 `signature` 字段并落库到 artifact `signature_metadata`;Storage trait 增加 `put_cors` 能力 + 新增 `POST /api/v1/storage/backends/{id}/cors` 端点(`storage:manage`)。

## Impact

- **api-types**:`CompletePart` 加 `signature: Option<String>`;新增 `CorsConfigRequest` / `CorsConfigResult` DTO。
- **server**:`Storage` trait + `S3Storage` 加 `put_cors`;`routes/storage.rs` 加 cors 端点;`routes/uploads/service.rs::upsert_artifact` 写 `signature_metadata`;openapi 注解同步。
- **entity**:无 schema 变更(`artifact.signature_metadata` 列已存在,仅从未写过)。
- **admin**:`ArtifactsDrawer` 升级为可上传;新增 hash worker + `lib/api/uploads.ts`;storage 页加 CORS 按钮 + `storage.ts` helper;新依赖 `hash-wasm`。
- **docs**:`docs/07-storage-and-delivery.md` 补 CORS 段;`docs/12-cli.md` 标注网页发布与 CLI 平价。
- **测试**:hash/分类/`.sig` 配对纯函数单测;`put_cors` + 签名落库 server 测试;整页渲染 / e2e 沿用 foundation harness gap 暂缓。

## Non-goals

- **不做断点续传 / multipart 分片**:单 PUT 直传(与 CLI 现状一致);超大文件 multipart 留后续提案。
- **不改下载 / updater manifest 生成**:本提案只写入 `signature_metadata`,不动 `routes/download` 或 manifest 拼装(Tauri/RN updater 链路是 `add-update-check-*` 的范围)。
- **不做独立"发布向导"页**:增量嵌进现有 `ArtifactsDrawer`,不另起 wizard 路由。
- **不改 CLI**:CLI publish 链路不动。
- **不替 OSS 兜底自动配 CORS**:OSS 原生 CORS API 不在范围,仅给出文档回退。

## Depends on

- `add-storage-and-presign-upload`(已归档)——presign / complete / Storage trait / `artifact.signature_metadata` 列。
- `add-app-release-artifact`(已归档)——Release / Artifact / Channel promote。
- `add-releases-page-ui`(apply 完成待归档)——`ArtifactsDrawer` 宿主 + `promote` helper。
- `add-storage-wizard-page`(apply 完成待归档)——storage 页宿主(挂 CORS 按钮)。

## Maps to docs

- `docs/03-architecture.md` 上传链路(presign 直传,不中转字节)。
- `docs/07-storage-and-delivery.md` 存储与下发(CORS 新增段)。
- `docs/12-cli.md` publish(网页发布与 CLI 同源)。
