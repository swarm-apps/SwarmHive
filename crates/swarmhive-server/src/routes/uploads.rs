//! `/api/v1/apps/:slug/releases/:ver/uploads/{presign,complete}` —— 预签名直传 +
//! 完成回调链路。
//!
//! presign 为每个文件签发一个绑定校验和的 PUT 并记录 `upload_session`;complete
//! 对每个对象 HeadObject 确认 size + 校验和(不二次下载),原子 upsert artifact 行,
//! 并标记 upload session 完成。幂等:重复 complete 已完成的 session 直接返回当前
//! release 状态。
//!
//! **发布与上传已解耦**(`harden-publish-flow` D2):`complete` 不再触发发布,发布走
//! 独立的幂等 `POST .../finalize` 端点(`releases::finalize_release`)。
//! `complete(publish=true)` 仍被接受但**已 deprecated**,仅为过渡期兼容尚未升级的下游
//! 客户端——此时内部委托给同一个 `releases::finalize_publish`(带 release 级锁、幂等、
//! 校验 artifact ≥ 1),待下游升级后移除。具体业务实现见 `service` 子模块。

mod service;

use axum::Json;
use axum::extract::{Path, State};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, EntityTrait, TransactionTrait};
use swarmhive_api_types::{
    self as api, CompleteRequest, CompleteResponse, PermissionName, PresignPart, PresignRequest,
    PresignResponse, RegisterArtifactRequest,
};
use swarmhive_entity::upload_session;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::require_permission;
use crate::routes::apps::{find_app, principal_actor_type};
use crate::routes::releases::{finalize_publish, find_release};
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
        .routes(routes!(register_artifact))
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
    responses((status = 200, body = CompleteResponse, description = "Artifacts written and upload session marked complete. Publishing is decoupled — use POST /api/v1/apps/{slug}/releases/{version}/finalize. (`publish=true` is still accepted but deprecated.)"), ApiErrorResponses),
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

    // 写 artifact(原子 upsert)+ 标记 session 完成,放进一个事务。**不再加锁** ——
    // 并发安全已由 `ON CONFLICT DO UPDATE` + `uq_artifact_release_variant`
    // (NULLS NOT DISTINCT)唯一索引在 DB 层保证,各 target 各写各行,无写-写竞争。
    // 若某 part 携带 mirror_url,先做 store-time allowlist 校验(host=github.com +
    // app 配置的 owner/repo),不合格直接拒绝——公开目录那个无验签的镜像按钮靠这层兜。
    for part in &req.parts {
        if let Some(mirror) = part.mirror_url.as_deref().filter(|s| !s.is_empty()) {
            crate::services::mirror::validate_mirror_url(&state.db, app.id, mirror).await?;
        }
    }

    let txn = state.db.begin().await?;
    for &(part, planned) in &verified {
        service::upsert_artifact(
            &txn,
            rel.id,
            service::Delivery {
                storage_backend_id: Some(backend.id),
                object_key: Some(planned.object_key.clone()),
                mirror_url: part.mirror_url.clone().filter(|s| !s.is_empty()),
            },
            planned,
            part.sha256.clone(),
            part.signature.clone(),
        )
        .await?;
    }
    let mut sm: upload_session::ActiveModel = session.into();
    sm.status = Set(upload_session::UploadStatus::Completed);
    sm.update(&txn).await?;
    txn.commit().await?;

    // 过渡兼容(DEPRECATED):新流程下 complete 只上传,发布走独立的幂等 finalize 端点。
    // 尚未升级的旧客户端仍可能发 publish=true —— 此时委托给 finalize_publish(release
    // 级排他锁 + 幂等 + 校验 artifact ≥ 1),与 finalize 端点是同一条发布路径。artifact
    // 已先行提交,故即便发布因权限 / 校验失败也不回滚已传产物(直接消除「上传前撞 403 →
    // 0 产物」的旧失败模式)。待下游全部升级后整段移除。
    let mut final_status = rel.status;
    if req.publish {
        tracing::warn!(
            release_id = %rel.id,
            "complete(publish=true) is deprecated; upload to draft then call POST .../finalize"
        );
        let ftxn = state.db.begin().await?;
        let outcome = match finalize_publish(&ftxn, app.id, &slug, rel.id).await {
            Ok(outcome) => outcome,
            Err(e) => {
                ftxn.rollback().await?;
                return Err(e);
            }
        };
        ftxn.commit().await?;
        final_status = outcome.release.status;
        if outcome.newly_published {
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
    }

    Ok(Json(CompleteResponse {
        release_id: rel.id,
        status: final_status.into(),
        endpoints: service::endpoints_for(&state, &slug, &version, rel.id).await,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases/{version}/uploads/register",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    request_body = RegisterArtifactRequest,
    responses((status = 200, body = CompleteResponse, description = "Externally-hosted (GitHub Release) artifact registered without an S3 upload. Publishing stays decoupled — use POST .../finalize."), ApiErrorResponses),
    tag = "uploads",
)]
async fn register_artifact(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
    Json(req): Json<RegisterArtifactRequest>,
) -> Result<Json<CompleteResponse>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ArtifactUpload,
        Scope::App(app.id)
    )?;
    let rel = find_release(&state.db, app.id, &version).await?;

    // 字节托管在外部源:server 不 Head、不持有对象,信任客户端声明的 sha256/size;
    // 真伪由客户端 minisign/keystore + 读侧 liveness/digest 兜底。mirror_url 必过 allowlist。
    crate::services::mirror::validate_mirror_url(&state.db, app.id, &req.mirror_url).await?;

    let planned = PlannedPart {
        object_key: String::new(), // 无 S3 对象
        filename: req.filename.clone(),
        size: req.size,
        expected_sha256: req.sha256.clone(),
        expected_md5: String::new(),
        platform: req.platform,
        kind: req
            .kind
            .unwrap_or_else(|| api::ArtifactKind::infer(req.platform, &req.filename)),
        target: req.target.clone(),
        arch: req.arch.clone(),
        abi: req.abi.clone(),
    };

    let txn = state.db.begin().await?;
    service::upsert_artifact(
        &txn,
        rel.id,
        service::Delivery {
            storage_backend_id: None,
            object_key: None,
            mirror_url: Some(req.mirror_url.clone()),
        },
        &planned,
        req.sha256.clone(),
        req.signature.clone(),
    )
    .await?;
    txn.commit().await?;

    Ok(Json(CompleteResponse {
        release_id: rel.id,
        status: rel.status.into(),
        endpoints: service::endpoints_for(&state, &slug, &version, rel.id).await,
    }))
}
