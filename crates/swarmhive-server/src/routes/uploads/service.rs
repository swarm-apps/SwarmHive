//! `uploads` 的业务实现:对象键生成、上传计划、完成校验、artifact 落库、下载入口
//! 拼接。handler（`super`）只做 HTTP 编排,「怎么做」都在这里。

use std::collections::BTreeMap;

use axum::http::StatusCode;
use sea_orm::ActiveValue::Set;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use swarmhive_api_types::{self as api, CompletePart, PresignFile};
use swarmhive_entity::artifact;
use uuid::Uuid;

use crate::auth::Principal;
use crate::error::ApiError;
use crate::routes::apps::principal_actor_type;
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;
use crate::storage::StorageHandle;

/// 持久化进 `upload_session.parts`（JSONB）的逐文件计划;complete 读回它,按
/// `object_key` 重建 artifact 行并校验上传结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlannedPart {
    pub(super) object_key: String,
    pub(super) filename: String,
    pub(super) size: i64,
    pub(super) expected_sha256: String,
    pub(super) expected_md5: String,
    pub(super) platform: api::Platform,
    #[serde(default = "default_artifact_kind")]
    pub(super) kind: api::ArtifactKind,
    pub(super) target: Option<String>,
    pub(super) arch: Option<String>,
    pub(super) abi: Option<String>,
}

fn default_artifact_kind() -> api::ArtifactKind {
    api::ArtifactKind::Universal
}

/// Platform 的 wire 字符串（kebab,如 `tauri-desktop`）,失败回退 `unknown`。
fn platform_str(platform: api::Platform) -> String {
    serde_json::to_value(platform)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

/// 取相对路径的文件名段（末尾 `/` 之后）。
fn filename_of(relative_path: &str) -> &str {
    relative_path.rsplit('/').next().unwrap_or(relative_path)
}

/// 对象键:`apps/{slug}/versions/{version}/{platform}/{variant}/{filename}`。去
/// channel、版本寻址——promote 只移指针,产物零搬运。variant 取 target → abi →
/// arch,都没有则 `any`。
pub(super) fn object_key(slug: &str, version: &str, f: &PresignFile) -> String {
    let variant = f
        .target
        .clone()
        .or_else(|| f.abi.clone())
        .or_else(|| f.arch.clone())
        .unwrap_or_else(|| "any".into());
    format!(
        "apps/{slug}/versions/{version}/{}/{variant}/{}",
        platform_str(f.platform),
        filename_of(&f.relative_path)
    )
}

/// 由 presign 请求里的一个文件构造其上传计划（对象键 + 校验所需字段）。
pub(super) fn plan_part(slug: &str, version: &str, f: &PresignFile) -> PlannedPart {
    PlannedPart {
        object_key: object_key(slug, version, f),
        filename: filename_of(&f.relative_path).to_string(),
        size: f.size,
        expected_sha256: f.expected_sha256.clone(),
        expected_md5: f.expected_md5.clone(),
        platform: f.platform,
        kind: f
            .kind
            .unwrap_or_else(|| api::ArtifactKind::infer(f.platform, filename_of(&f.relative_path))),
        target: f.target.clone(),
        arch: f.arch.clone(),
        abi: f.abi.clone(),
    }
}

/// 上传完整性校验失败的 422（RFC 9457 typed）。
fn checksum_mismatch(object_key: &str) -> ApiError {
    ApiError::typed(
        StatusCode::UNPROCESSABLE_ENTITY,
        "https://swarmhive.dev/errors/upload-checksum-mismatch",
        "Unprocessable Entity",
        format!("uploaded object {object_key} failed checksum/size verification"),
    )
}

/// 写一条 `upload_checksum_mismatch` 审计行后返回 422（篡改 / 损坏留痕）。
async fn audit_and_mismatch(
    state: &AppState,
    principal: &Principal,
    app_id: Uuid,
    object_key: &str,
) -> ApiError {
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: principal_actor_type(principal),
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: Some(app_id),
            action: "upload_checksum_mismatch".into(),
            resource_type: Some("artifact".into()),
            resource_id: Some(object_key.to_string()),
            ip: None,
            user_agent: None,
            metadata: serde_json::json!({ "object_key": object_key }),
        },
    )
    .await;
    checksum_mismatch(object_key)
}

/// 在计划里按 `object_key` 找到对应 `PlannedPart`;找不到说明 complete 报了计划外
/// 的对象 → 422。
pub(super) fn match_planned<'a>(
    plan: &'a [PlannedPart],
    object_key: &str,
) -> Result<&'a PlannedPart, ApiError> {
    plan.iter()
        .find(|p| p.object_key == object_key)
        .ok_or_else(|| checksum_mismatch(object_key))
}

/// 对照对象存储校验单个已上传 part（只 HeadObject,不二次下载）,返回其计划项。
/// 通过条件:size 一致,且「有 sha256 就比 sha256,否则用单段 ETag=MD5 兜底」。
pub(super) async fn verify_part<'a>(
    state: &AppState,
    principal: &Principal,
    app_id: Uuid,
    storage: &StorageHandle,
    plan: &'a [PlannedPart],
    part: &CompletePart,
) -> Result<&'a PlannedPart, ApiError> {
    let planned = match_planned(plan, &part.object_key)?;
    let meta = storage
        .head(&part.object_key)
        .await
        .map_err(|_| checksum_mismatch(&part.object_key))?;
    if meta.size != planned.size {
        return Err(audit_and_mismatch(state, principal, app_id, &part.object_key).await);
    }
    let ok = match &meta.sha256_hex {
        // AWS / MinIO / RustFS:存储侧写入时已自校验 sha256,这里确认回放一致。
        Some(remote_hex) => remote_hex.eq_ignore_ascii_case(&part.sha256),
        // 不回传 sha256 的后端（如阿里云 OSS）:写入时 Content-MD5 已强制,单段 PUT
        // 的 ETag 即 hex MD5,与计划 md5 比对作正向确认;非 MD5 ETag 跳过(靠写时 MD5)。
        None => etag_as_md5(&meta.etag)
            .map(|etag_md5| etag_md5.eq_ignore_ascii_case(&planned.expected_md5))
            .unwrap_or(true),
    };
    if ok {
        Ok(planned)
    } else {
        Err(audit_and_mismatch(state, principal, app_id, &part.object_key).await)
    }
}

/// 单段 PUT 的 ETag 即 hex MD5（带引号;OSS 大写）。归一化为 32 位小写 hex;
/// multipart / SSE 等非 MD5 ETag 返回 `None`(此时靠写入时 Content-MD5 兜底,跳过回读)。
fn etag_as_md5(etag: &Option<String>) -> Option<String> {
    let clean = etag.as_deref()?.trim_matches('"').to_ascii_lowercase();
    (clean.len() == 32 && clean.bytes().all(|b| b.is_ascii_hexdigit())).then_some(clean)
}

/// 一个 artifact 的投递位置:S3 对象(both Some)/ 外部镜像(mirror Some)/ 二者兼有。
/// 至少一个必须存在(invariant enforced by the write path, not the DB —— 跨三个可空列)。
#[derive(Debug, Clone, Default)]
pub(super) struct Delivery {
    pub(super) storage_backend_id: Option<Uuid>,
    pub(super) object_key: Option<String>,
    pub(super) mirror_url: Option<String>,
}

/// 原子 upsert artifact 行,冲突键 `(release_id, platform, target, arch, abi, kind)`。
///
/// 走数据库级 `INSERT ... ON CONFLICT DO UPDATE`(冲突键即 migration 建的
/// `uq_artifact_release_variant_kind`,`NULLS NOT DISTINCT` 让 NULL 列也参与冲突收敛),
/// 替换原先的 SELECT-then-INSERT —— 后者在多 target 并发 complete 时存在写-写竞争。
/// 同 target + kind 重传命中冲突 → 更新内容列;安装包和升级包因 kind 不同可共存。
///
/// 投递位置的重传语义(`add-github-release-source`):
/// - `mirror_url` **无条件**进 update_columns —— 随字节更新,缺失即清空(与字节强绑定,
///   不残留指向旧字节的死链;这与 signature 的"缺失保留"相反)。
/// - `storage_backend_id` / `object_key` **仅在本次是 S3 写入**(`object_key` 为 Some)时
///   才进 update_columns —— 这样一个「仅镜像」的 register 命中既有 S3 行时不会把 S3 位置抹成 null。
///
/// `signature` 非空时(Tauri `.sig` 文本)落进 `signature_metadata`;为空则保持不动
/// (insert 时为 `null`,update 时不把既有签名覆盖成 null)。
pub(super) async fn upsert_artifact<C: sea_orm::ConnectionTrait>(
    txn: &C,
    release_id: Uuid,
    delivery: Delivery,
    planned: &PlannedPart,
    sha256: String,
    signature: Option<String>,
) -> Result<(), ApiError> {
    let sig_value = signature
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::json!({ "tauri_signature": s }));
    let has_signature = sig_value.is_some();
    let is_s3_write = delivery.object_key.is_some();

    let model = artifact::ActiveModel {
        id: Set(Uuid::now_v7()),
        release_id: Set(release_id),
        platform: Set(planned.platform.into()),
        kind: Set(planned.kind.into()),
        target: Set(planned.target.clone()),
        arch: Set(planned.arch.clone()),
        abi: Set(planned.abi.clone()),
        filename: Set(planned.filename.clone()),
        size_bytes: Set(planned.size),
        sha256: Set(sha256),
        storage_backend_id: Set(delivery.storage_backend_id),
        object_key: Set(delivery.object_key.clone()),
        mirror_url: Set(delivery.mirror_url.clone()),
        signature_metadata: Set(sig_value),
        // `on_conflict + exec_*` 跳过 ActiveModelBehavior::before_save,
        // created_at 必须显式 Set,否则 NOT NULL 违例(见 backend.md)。
        created_at: Set(chrono::Utc::now()),
    };

    let mut on_conflict = OnConflict::columns([
        artifact::Column::ReleaseId,
        artifact::Column::Platform,
        artifact::Column::Target,
        artifact::Column::Arch,
        artifact::Column::Abi,
        artifact::Column::Kind,
    ]);
    on_conflict.update_columns([
        artifact::Column::Filename,
        artifact::Column::SizeBytes,
        artifact::Column::Sha256,
        artifact::Column::MirrorUrl,
    ]);
    if is_s3_write {
        on_conflict.update_columns([
            artifact::Column::StorageBackendId,
            artifact::Column::ObjectKey,
        ]);
    }
    // 仅在带签名时覆盖 signature_metadata,避免重传(不带签名)抹掉既有签名。
    if has_signature {
        on_conflict.update_column(artifact::Column::SignatureMetadata);
    }

    artifact::Entity::insert(model)
        .on_conflict(on_conflict)
        .exec_without_returning(txn)
        .await?;
    Ok(())
}

/// release 各平台的下载入口 URL（每平台优先 updater/universal;公开安装包列表走 catalog）。
pub(super) async fn endpoints_for(
    state: &AppState,
    slug: &str,
    version: &str,
    release_id: Uuid,
) -> BTreeMap<String, String> {
    let base = state.config.server.base_url.trim_end_matches('/');
    let mut out = BTreeMap::new();
    if let Ok(mut arts) = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(release_id))
        .all(&state.db)
        .await
    {
        arts.sort_by_key(|a| (endpoint_rank(a.kind), a.filename.clone()));
        for a in arts {
            out.entry(platform_str(api::Platform::from(a.platform)))
                .or_insert_with(|| {
                    crate::routes::download::download_url(base, slug, version, a.id)
                });
        }
    }
    out
}

fn endpoint_rank(kind: artifact::ArtifactKind) -> u8 {
    match kind {
        artifact::ArtifactKind::Updater => 0,
        artifact::ArtifactKind::Universal => 1,
        artifact::ArtifactKind::Installer => 2,
    }
}
