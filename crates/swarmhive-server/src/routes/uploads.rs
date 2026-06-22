//! `/api/v1/apps/:slug/releases/:ver/uploads/{presign,complete}` —— 预签名直传 +
//! 完成回调链路。
//!
//! presign 为每个文件签发一个绑定校验和的 PUT 并记录 `upload_session`;complete
//! 对每个对象 HeadObject 确认 size + 校验和(不二次下载),写 artifact 行,并在
//! `publish=true`、调用者持 `release:publish`、release 至少 1 个 artifact 时发布
//! release。幂等:重复 complete 已完成的 session 直接返回当前 release 状态。
//! 具体业务实现见 `service` 子模块。

mod service;

use axum::Json;
use axum::extract::{Path, State};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, TransactionTrait,
};
use swarmhive_api_types::{
    self as api, CompleteRequest, CompleteResponse, PermissionName, PresignPart, PresignRequest,
    PresignResponse,
};
use swarmhive_entity::{artifact, release, upload_session};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::notify::emit::{self, NewEvent};
use crate::require_permission;
use crate::routes::apps::{find_app, principal_actor_type};
use crate::routes::releases::{find_release, notification_payload};
use crate::services::audit::{self, AuditEntry};
use crate::services::storage::{active_backend, handle};
use crate::state::AppState;

use service::PlannedPart;

/// 预签名 URL 的有效期(秒);upload_session 的过期时间也用它。
const PRESIGN_TTL_SECS: u64 = 600;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(presign))
        .routes(routes!(complete))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases/{version}/uploads/presign",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    request_body = PresignRequest,
    responses((status = 200, body = PresignResponse, description = "Per-file presigned PUTs + upload_id."), ApiErrorResponses),
    tag = "uploads",
)]
async fn presign(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
    Json(req): Json<PresignRequest>,
) -> Result<Json<PresignResponse>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ArtifactUpload,
        Scope::App(app.id)
    )?;
    let rel = find_release(&state.db, app.id, &version).await?;

    let storage = handle(&state)?;
    let backend = active_backend(&state).await?;

    // 逐文件:算上传计划 → 签发一个绑定 Content-MD5(+ 可选 sha256)的 PUT。
    let mut parts = Vec::with_capacity(req.files.len());
    let mut plan: Vec<PlannedPart> = Vec::with_capacity(req.files.len());
    for f in &req.files {
        let planned = service::plan_part(&slug, &version, f);
        let presigned = storage
            .presign_put(
                &planned.object_key,
                &f.expected_sha256,
                &f.expected_md5,
                PRESIGN_TTL_SECS,
                backend.supports_sha256_checksum,
            )
            .await
            .map_err(|e| ApiError::Validation {
                detail: e.to_string(),
            })?;
        parts.push(PresignPart {
            object_key: planned.object_key.clone(),
            presigned_url: presigned.url,
            headers: presigned.headers,
        });
        plan.push(planned);
    }

    let upload_id = Uuid::now_v7();
    upload_session::ActiveModel {
        id: Set(upload_id),
        release_id: Set(rel.id),
        created_by: Set(principal.user_id),
        // plan 是 Vec<PlannedPart>，序列化 infallible —— expect 写明不变量，
        // 真出错时 fail-fast 而非静默写空数组。
        parts: Set(serde_json::to_value(&plan).expect("upload plan serializes")),
        status: Set(upload_session::UploadStatus::Pending),
        expires_at: Set(chrono::Utc::now() + chrono::Duration::seconds(PRESIGN_TTL_SECS as i64)),
        created_at: NotSet,
    }
    .insert(&state.db)
    .await?;

    Ok(Json(PresignResponse { upload_id, parts }))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases/{version}/uploads/{upload_id}/complete",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
        ("upload_id" = Uuid, Path, description = "Upload session id."),
    ),
    request_body = CompleteRequest,
    responses((status = 200, body = CompleteResponse, description = "Artifacts written; release optionally published."), ApiErrorResponses),
    tag = "uploads",
)]
async fn complete(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version, upload_id)): Path<(String, String, Uuid)>,
    Json(req): Json<CompleteRequest>,
) -> Result<Json<CompleteResponse>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ArtifactUpload,
        Scope::App(app.id)
    )?;
    let rel = find_release(&state.db, app.id, &version).await?;
    if req.publish {
        require_permission!(
            principal,
            PermissionName::ReleasePublish,
            Scope::App(app.id)
        )?;
    }

    let session = upload_session::Entity::find_by_id(upload_id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    // 幂等:已完成的 session 直接回报当前 release 状态。
    if session.status == upload_session::UploadStatus::Completed {
        return Ok(Json(CompleteResponse {
            release_id: rel.id,
            status: rel.status.into(),
            endpoints: service::endpoints_for(&state, &slug, &version, rel.id).await,
        }));
    }

    let plan: Vec<PlannedPart> = serde_json::from_value(session.parts.clone()).unwrap_or_default();
    let storage = handle(&state)?;
    let backend = active_backend(&state).await?;

    // 逐 part 对照对象存储校验(size + 校验和,不二次下载),收集已校验的计划项。
    let mut verified = Vec::with_capacity(req.parts.len());
    for part in &req.parts {
        let planned =
            service::verify_part(&state, &principal, app.id, &storage, &plan, part).await?;
        verified.push((part, planned));
    }

    // 写 artifact + 标记 session 完成 +(可选)发布,放进一个事务。
    let txn = state.db.begin().await?;
    for &(part, planned) in &verified {
        service::upsert_artifact(
            &txn,
            rel.id,
            backend.id,
            planned,
            part.sha256.clone(),
            part.signature.clone(),
        )
        .await?;
    }
    let mut sm: upload_session::ActiveModel = session.into();
    sm.status = Set(upload_session::UploadStatus::Completed);
    sm.update(&txn).await?;

    let mut final_status = rel.status;
    let should_emit_published = req.publish && rel.status == release::ReleaseStatus::Draft;
    if req.publish {
        let count = artifact::Entity::find()
            .filter(artifact::Column::ReleaseId.eq(rel.id))
            .count(&txn)
            .await?;
        if count == 0 {
            txn.rollback().await?;
            return Err(ApiError::Validation {
                detail: "cannot publish a release with no artifacts".into(),
            });
        }
        // mark_published 幂等：Draft → Published 并盖 published_at，非 Draft 原样返回。
        let updated = crate::routes::releases::mark_published(&txn, rel.clone()).await?;
        final_status = updated.status;
        if should_emit_published && final_status == release::ReleaseStatus::Published {
            emit::emit(
                &txn,
                NewEvent {
                    event_type: api::NotificationEventType::ReleasePublished,
                    app_id: app.id,
                    data: notification_payload(&slug, &updated, None),
                },
            )
            .await?;
        }
    }
    txn.commit().await?;

    // 发布成功后补一条审计(失败不影响主流程)。
    if req.publish && final_status == release::ReleaseStatus::Published {
        audit::write_swallowing(
            &state.db,
            AuditEntry {
                actor_type: principal_actor_type(&principal),
                actor_id: Some(principal.user_id),
                org_id: principal.org_id,
                app_id: Some(app.id),
                action: "release_published".into(),
                resource_type: Some("release".into()),
                resource_id: Some(rel.id.to_string()),
                ip: None,
                user_agent: None,
                metadata: serde_json::json!({ "version": version, "via": "upload_complete" }),
            },
        )
        .await;
    }

    Ok(Json(CompleteResponse {
        release_id: rel.id,
        status: final_status.into(),
        endpoints: service::endpoints_for(&state, &slug, &version, rel.id).await,
    }))
}
