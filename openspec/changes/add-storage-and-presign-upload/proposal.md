# add-storage-and-presign-upload

## Why

docs/07 把存储抽象统一为 S3-compatible；docs/12 把 CLI publish 上传形态定为 **presign 直传 + complete 回调**。本 proposal 落地这两块：StorageBackend 配置实体 + 后台 storage 初始化向导 + presign / complete 双端点 + CLI publish 命令。

## What

### 1. 实体

- `storage_backend`：id、name、kind (`s3`)、active、endpoint、bucket、region、access_key_id、`access_key_secret_encrypted`、`force_path_style`、`prefix`、`public_base_url`、`url_mode` (`public` | `signed`)、`signed_url_ttl_secs`、connectivity_status_jsonb、created_at。
- `upload_session`：id、release_id、created_by_principal、parts JSONB (each: object_key, sha256, etag, size, completed_at)、status (`pending`/`completed`/`expired`)、expires_at、created_at。

### 2. Server endpoints

存储管理（按 docs/08 Storage 页）：

```
GET    /api/v1/storage/backends                  storage:manage
POST   /api/v1/storage/backends                  storage:manage
PATCH  /api/v1/storage/backends/:id              storage:manage
POST   /api/v1/storage/backends/:id/test         storage:manage
POST   /api/v1/storage/backends/:id/activate     storage:manage
```

测试动作：list bucket、put / get / delete 一个 `.swarmhive-probe` 对象。

上传：

```
POST   /api/v1/apps/:slug/releases/:ver/uploads/presign   artifact:upload
       req:  { files: [{ relative_path, size, expected_sha256, platform, target, arch?, abi? }] }
       resp: { upload_id, parts: [{ object_key, presigned_url, headers }] }

POST   /api/v1/apps/:slug/releases/:ver/uploads/:upload_id/complete
       req:  { parts: [{ object_key, sha256, etag }] }
       resp: { release_id, endpoints: { tauri: …, android: … } }
```

complete 处理：

- server HEAD 对象拿 size + etag 做 sanity check（不再次下载校验 hash，信任 client 报的 sha256 但写 audit）。
- 一致性满足后写 `artifact` + 标记 `upload_session=completed` + 更新 `release.status=published`（若 `auto_publish=true`）。
- 幂等：同 upload_id 重复 complete 返回相同 release_id。

### 3. Admin Setup Wizard

按 docs/03 / docs/08 步骤：

1. 检测无 active storage → 引导。
2. 三选一：Existing S3 / Aliyun OSS preset / Single-server bundled RustFS。
3. RustFS 选项展示 `docker compose --profile bundled-storage up -d` 与 health-check 状态（不主动执行 docker 命令）。
4. 测试 put / get / delete → 保存 backend。

### 4. CLI

- `swarmhive verify tauri|android`：本地校验产物 + 算 sha256。
- `swarmhive publish tauri|android`：扫描 → presign → 直传（reqwest stream + indicatif progress bar）→ complete → 输出 endpoint。
- `swarmhive storage init rustfs`：输出 RustFS compose profile 命令、检测健康、调 server 创建 storage_backend。
- 错误重试：单 part 失败可单独 retry，不重传成功的 part。

## Acceptance

- Admin 能完成存储初始化（包含 RustFS 路径 + 真实测试上传/下载）。
- CLI 发布 100 MB Tauri 安装包：能看到进度条；失败重发只重传失败 part；成功后 Admin 能看到 release + artifact。
- 同 upload_id 重复 complete 返回相同 release_id（幂等性测试）。
- presign URL 5 分钟后过期，复用旧 URL 上传返回 SignatureDoesNotMatch（mock S3 测试）。
- 错误 sha256 → complete 接口拒绝 + 写 audit。
- `force_path_style` 切换正确（OSS=false，RustFS=true）。

## Non-goals

- 不做 S3 multipart upload 的客户端分片（先支持单 PUT；future 改进留 hook）。
- 不做 CDN URL 重写 / 智能调度。
- 不做 storage 后端定期清理（orphan object）。
- 不做 RustFS 进程托管，只展示命令。

## Depends on

- `add-app-release-artifact`
- `add-pat-and-api-token`（CLI publish 需要 token）

## Maps to docs

- [docs/07-storage-and-delivery.md](../../../docs/07-storage-and-delivery.md) 全文。
- [docs/12-cli.md](../../../docs/12-cli.md) "上传形态：presign 直传 + complete 回调"。
- [docs/06-cicd.md](../../../docs/06-cicd.md) publish 一节。
- [docs/09-mvp-roadmap.md](../../../docs/09-mvp-roadmap.md) 阶段 3 + 4 + 5。
