# tasks

## 1. Entity (`swarmhive-entity/src/`)

- [ ] 1.1 `storage_backend.rs`：Model（id/name/kind/active/endpoint/bucket/region/access_key_id/access_key_secret_encrypted/force_path_style/prefix/public_base_url/url_mode `UrlMode`/signed_url_ttl_secs/supports_sha256_checksum/connectivity_status `Option<Json>`/created_at/updated_at）+ `UrlMode` DeriveActiveEnum（`#[serde(rename_all="lowercase")]` public/signed）+ `From<&Model> for api::StorageBackendView`（不含 secret，带 `secret_set`）
- [ ] 1.2 `upload_session.rs`：Model（id/release_id/created_by/parts `Json`/status `UploadStatus`/expires_at/created_at）+ `UploadStatus` DeriveActiveEnum（pending/completed/expired）+ `belongs_to release`
- [ ] 1.3 `artifact.rs`：补 `#[sea_orm(belongs_to, from="storage_backend_id", to="id")] storage_backend`（`add-app-release-artifact` 留的裸列）
- [ ] 1.4 `lib.rs` 注册 module；enum serde round-trip 单测

## 2. api-types

- [ ] 2.1 `storage.rs`：`StorageBackendView`（secret_set: bool）+ `CreateStorageBackendRequest` + `UpdateStorageBackendRequest` + `UrlMode` + `TestResult`
- [ ] 2.2 `upload.rs`：`PresignRequest { files: [PresignFile] }` + `PresignResponse { upload_id, parts: [PresignPart] }` + `CompleteRequest { parts: [CompletePart], publish?: bool }` + `CompleteResponse { release_id, status, endpoints }`
- [ ] 2.3 `lib.rs` re-export

## 3. Storage 抽象 (`swarmhive-server/src/storage/`)

- [ ] 3.1 `mod.rs`：`Storage` trait（`presign_put` / `head` / `public_url` / `signed_url` / `delete` / `probe`）+ `PresignedPut` / `ObjectMeta` 类型 + `StorageError`
- [ ] 3.2 `s3.rs`：`S3Storage`（aws-sdk-s3）；presign `PutObject().checksum_sha256(b64).presigned()`；`head` 读 ChecksumSHA256 + ContentLength；`force_path_style` 适配；`from_backend(row, &SecretKey)` 解密 secret 构造
- [ ] 3.3 `AppState.storage: Arc<RwLock<Option<StorageHandle>>>`；启动 `wire_active_storage()`；`refresh_storage()` hot swap（参考 mail `refresh_mailer`）
- [ ] 3.4 workspace + server Cargo.toml 加 `aws-sdk-s3`（+ aws-config，rustls）

## 4. Server — `routes/storage.rs`（backend 管理）

- [ ] 4.1 `GET/POST /storage/backends`（`storage:manage`）；secret 加密落盘（crypto::SecretKey），GET 返 `secret_set`
- [ ] 4.2 `PATCH /storage/backends/:id`（空 secret = 不改）
- [ ] 4.3 `POST /storage/backends/:id/test`：put/get/delete `.swarmhive-probe` + 探测 checksum 支持 → 写 `supports_sha256_checksum` + `connectivity_status`
- [ ] 4.4 `POST /storage/backends/:id/activate`：TX 置单 active + `refresh_storage()`
- [ ] 4.5 utoipa::path + tag `storage`；挂 `openapi_router()` + `build_router()`

## 5. Server — `routes/uploads.rs`（presign / complete）

- [ ] 5.1 `object_key` 生成 helper：`{prefix}/apps/{slug}/versions/{version}/{platform}/{target}/{filename}`（无 channel）
- [ ] 5.2 `POST .../uploads/presign`（`artifact:upload`）：release 须存在；无 active backend → `409 storage_not_configured`；每文件签 PUT + 绑 `x-amz-checksum-sha256`（5–10min）；建 `upload_session=pending`
- [ ] 5.3 `POST .../uploads/:upload_id/complete`（`artifact:upload`）：TX 内每 part `head` 确认 checksum+size（不符 `422 upload_checksum_mismatch` + audit）→ upsert artifact（ON CONFLICT）→ 标 session completed
- [ ] 5.4 complete `publish=true`：额外校验 `release:publish`（缺 403）+ release ≥1 artifact + 置 published + published_at + publish audit
- [ ] 5.5 幂等：同 upload_id 重复 complete 返相同 release_id
- [ ] 5.6 utoipa::path + tag `uploads`

## 6. Server — `routes/download.rs`（分发）

- [ ] 6.1 `GET /download/:app/:version/:artifact_id`（公开）：查 artifact；yanked release → 404
- [ ] 6.2 记 `download_intent`（最小表或 structured log，见 design Open Question）→ 按 url_mode 生成 public/signed URL → `302`
- [ ] 6.3 错误类型 `storage_not_configured`(409) / `upload_checksum_mismatch`(422) 加到 `error.rs`

## 7. CLI（`swarmhive-cli`）

- [ ] 7.1 `swarmhive.toml` 解析：`config.rs`（server / [app] / [app.tauri] / [app.android]）；`--app` 覆盖
- [ ] 7.2 `commands/verify.rs`：tauri（latest.json 解析 + 产物存在 + sha256）/ android（apk 存在 + sha256 + 信任 flag）+ 查 server 重复版本（`--dry-run` 跳过）
- [ ] 7.3 `commands/publish.rs`：ensure draft release → 算 sha256（sha2 流式）→ presign → 直传（reqwest stream + indicatif + `x-amz-checksum-sha256` 头 + backon 重试，单文件失败只重该文件）→ complete（publish=true）→ 输出 endpoint
- [ ] 7.4 `commands/storage.rs`：`storage init rustfs` 输出 compose profile 指引 + health-check + 调 server 建/激活 backend
- [ ] 7.5 Tauri version 自动读 `tauri.conf.json`；Android `--version`/`--version-code` 显式
- [ ] 7.6 reqwest features 切 `rustls-tls-native-roots`；`--ca-cert`/`SWARMHIVE_CA_CERT`；CLI Cargo.toml 加 `sha2` `backon`（workspace pin）
- [ ] 7.7 clap 挂 `verify` / `publish` / `storage` 子命令

## 8. 分发（cargo-dist）

- [ ] 8.1 `dist-workspace.toml`：targets（win/macOS/linux gnu+musl）+ installers（shell/powershell/npm `@swarmhive/cli`）
- [ ] 8.2 release workflow（GH Actions）：tag → dist build → 上传 GH Releases + 发 npm
- [ ] 8.3 官方 GitHub Action（composite `action.yml`）：inputs（server/token/app/version/channel/artifacts/notes/dry-run）→ `npx @swarmhive/cli` → flags
- [ ] 8.4 `cargo tree -p swarmhive-cli | grep -E 'aws-sdk|sea-orm'` 回归守护：无输出

## 9. OpenAPI / 测试

- [ ] 9.1 `openapi_surface.rs` 加 storage / uploads / download paths + schemas
- [ ] 9.2 `pnpm --filter @swarmhive/admin openapi` 重生成 schema.gen.ts
- [ ] 9.3 集成测试（testcontainers Postgres + MinIO）：configure backend + `/test` probe；presign → 真实 PUT（含 checksum）→ complete（publish）→ artifacts 可见 + release published
- [ ] 9.4 幂等：重复 complete 同 release_id；checksum 不符 422 + audit；过期 URL `SignatureDoesNotMatch`
- [ ] 9.5 RBAC：developer complete publish=true → 403；download 302；yanked → 404
- [ ] 9.6 `force_path_style` OSS=false / RustFS=true 行为

## 10. 文档 / 知识库

- [ ] 10.1 `docs/07-storage-and-delivery.md` 文件路径规范段去 channel（与本 proposal 一起 commit 过 diff gate）
- [ ] 10.2 `dev-notes/knowledge/backend.md` 加「Storage trait + presign checksum + complete 幂等 + hot-swap backend」段；`docs/12` publish 流程对齐
- [ ] 10.3 `openspec/changes/README.md` 状态行更新；探索档 explore-summary 落地后可删
