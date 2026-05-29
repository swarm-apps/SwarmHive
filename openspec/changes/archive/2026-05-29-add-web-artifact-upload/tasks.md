# tasks — add-web-artifact-upload

按依赖排序:api-types → server(签名落库 + CORS)→ admin(hash worker + 上传 UI + CORS 按钮)→ docs/memory。`[code]`/`[test]`/`[docs]` 便于并行。

## 1. api-types(共享 DTO)

- [x] 1.1 [code] `upload.rs`:`CompletePart` 加 `#[serde(default)] pub signature: Option<String>`(向后兼容,CLI 不传)。
- [x] 1.2 [code] `storage.rs`:新增 `CorsConfigRequest { allowed_origins: Vec<String> }` + `CorsConfigResult { ok: bool, detail: String }`,都 derive `Serialize/Deserialize/ToSchema`;`lib.rs` 导出。
- [x] 1.3 [test] `cargo build -p swarmhive-api-types` 通过;确认 api-types 仍零 ORM/HTTP 依赖。

## 2. server — 签名落库

- [x] 2.1 [code] `routes/uploads/service.rs::upsert_artifact` 增加 `signature: Option<String>` 形参,写 `signature_metadata = signature.filter(非空).map(|s| json!({"tauri_signature": s}))`(insert 与 update 两分支一致;update 时仅在有签名时覆盖)。
- [x] 2.2 [code] `routes/uploads.rs::complete` 把 `part.signature.clone()` 透传给 `upsert_artifact`。
- [x] 2.3 [test] server 测试:complete 带 signature → artifact `signature_metadata == {"tauri_signature": ...}`;不带 → `null`(扩 `storage_smoke` 或新增 `upload_signature_smoke`)。

## 3. server — CORS 端点

- [x] 3.1 [code] `storage/mod.rs`:`Storage` trait 加 `async fn put_cors(&self, allowed_origins: &[String]) -> Result<(), StorageError>`。
- [x] 3.2 [code] `storage/s3.rs`:实现 `put_cors` —— 构造 `CORSRule`(`AllowedMethods=[PUT,GET,HEAD]`、`AllowedHeaders=["*"]`、`ExposeHeaders=["ETag"]`、`AllowedOrigins=入参`),调 `put_bucket_cors`;失败映射 `StorageError::Object`。
- [x] 3.3 [code] `routes/storage.rs`:加 `#[utoipa::path] async fn configure_cors`(`POST /storage/backends/{id}/cors`,`storage:manage`)——按 id 查 backend(404)→ build `S3Storage` → `put_cors`;`Ok` → `CorsConfigResult{ok:true}`,`Err` → `{ok:false, detail: <手动配置指引 + 错误>}`(不 5xx);`routes!(configure_cors)` 注册。
- [x] 3.4 [test] server 测试:`configure_cors` 对 MinIO backend → `ok:true`(`storage_smoke` 扩);无 `storage:manage` → 403。
- [x] 3.5 [code] `cargo run -p swarmhive-server --bin dump-openapi`(或既有方式)刷新 openapi,确认新端点 + `signature`/CORS DTO 出现在 schema。

## 4. admin — 客户端 hash + 上传 API 模块

- [x] 4.1 [code] `pnpm --filter @swarm-hive/admin add hash-wasm`。
- [x] 4.2 [code] `src/lib/upload/hash.worker.ts`:Web Worker 用 hash-wasm `createMD5()`/`createSHA256()`,按 ~8MB `Blob.slice` 分块 update,postMessage 进度 + 最终 `{md5_hex, sha256_hex}`。
- [x] 4.3 [code] `src/lib/upload/classify.ts`(纯函数):`classifyArtifact(filename) → {platform, target?, abi?, isSignature}`;`pairSignatures(files)` 把 `.sig` 配对到同名 bundle、检出孤立 `.sig`。
- [x] 4.4 [code] `src/lib/api/uploads.ts`:`presignUpload(slug,version,files)`、`putToStorage(part, file, onProgress)`(用 `XMLHttpRequest` 拿 `upload.onprogress`,回放 `part.headers`)、`completeUpload(slug,version,uploadId,parts,publish)`;类型从 `schema.gen.ts` 派生。
- [x] 4.5 [code] `src/lib/api/storage.ts`:加 `CORS_PATH` + `configureCors(id, origins)` helper + `CorsConfigResult` 类型。
- [x] 4.6 [test] Vitest:`classify.ts`(apk/abi、桌面扩展名、未知扩展名、`.sig` 配对、孤立 `.sig`)纯函数单测。

## 5. admin — ArtifactsDrawer 上传 UI

- [x] 5.1 [code] `routes/_auth/releases.tsx` `ArtifactsDrawer`:加"上传产物"区——`Upload.Dragger` 多选 → 文件表(可改 platform/target/abi)→ 逐文件 hash 进度 → 上传进度。
- [x] 5.2 [code] 编排:hash(worker)→ `presignUpload` → 逐文件 `putToStorage` → `completeUpload`(publish 开关)→ 成功 `invalidate artifactsQueryOptions`/`releasesQueryOptions`。
- [x] 5.3 [code] 上传后可选 promote:持 `release:promote` 时显示 channel 选择(`channelsQueryOptions`),发布成功后调既有 `promote()`;无权限隐藏(hide-not-disable)。
- [x] 5.4 [code] 权限门控:无 `artifact:upload` 隐藏上传入口(`usePermissions().has`)。
- [x] 5.5 [code] 错误处理:`.sig` 孤立 / hash 失败 / PUT 失败 / `complete` 422 `upload_checksum_mismatch` → `isApiError` 分支 + notification;失败保留 `upload_id` 可重试单文件。

## 6. admin — storage 页 CORS 按钮

- [x] 6.1 [code] `routes/_auth/settings/storage.tsx`:backend 行加"配置 CORS"操作 → `configureCors(id, [window.location.origin])`;`ok` success / `ok:false` 展示 `detail`(手动指引);`storage:manage` 才显示。

## 7. 校验 + docs/memory 同步

- [x] 7.1 [test] gates:`cargo fmt --all` / `cargo clippy --workspace --all-targets -D warnings` / `cargo test --workspace` / `pnpm lint` / `pnpm --filter @swarm-hive/admin typecheck` / `pnpm --filter @swarm-hive/admin test` / `pnpm admin:build` 全绿;`schema.gen.ts` 重新生成后纳入提交。
- [x] 7.2 [docs] `docs/07-storage-and-delivery.md` 补"浏览器直传需配 CORS"段 + OSS 手动 CORS 规则范例;`docs/12-cli.md` 标注网页发布与 CLI 平价。
- [x] 7.3 [docs] `dev-notes/knowledge/admin-spa.md` 加:hash-wasm Web Worker 流式 hash 范式、XHR 上传进度、`.sig` 配对、CORS 一键配置;`backend.md` 加 `put_cors` + signature_metadata 落库。
- [x] 7.4 [docs] `openspec/changes/README.md` 依赖图 + 进度表加 `add-web-artifact-upload`。
- [x] 7.5 [docs] 联动扫描:`grep -rn "signature_metadata\|put_cors\|ArtifactsDrawer 只读" openspec/changes docs` 清残留描述(如 releases 页"只读"措辞)。
