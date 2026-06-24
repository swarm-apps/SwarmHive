//! `/api/v1/apps/:slug/releases` — release lifecycle + channel promote/rollback
//! + read-only artifact listing.
//!
//! Release-train model: a release is channel-independent. `publish` flips
//! `draft → published` without touching any channel; `promote` / `rollback`
//! move a channel's `channel_release` pointer and append a
//! `channel_release_history` row in one transaction. Releases are never
//! deleted. Artifact creation is the upload `complete` callback's job
//! (`add-storage-and-presign-upload`); here artifacts are read-only.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use swarmhive_api_types::{
    self as api, CreateReleaseRequest, PermissionName, PromoteRequest, RollbackRequest,
    UpdateReleaseRequest,
};
use swarmhive_entity::{artifact, channel, channel_release, channel_release_history, release};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::notify::emit::{self, NewEvent};
use crate::require_permission;
use crate::routes::apps::{find_app, principal_actor_type};
use crate::services::audit::{self, AuditEntry};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_releases, create_release))
        .routes(routes!(get_release, update_release))
        .routes(routes!(publish_release))
        .routes(routes!(finalize_release))
        .routes(routes!(yank_release))
        .routes(routes!(promote))
        .routes(routes!(rollback))
        .routes(routes!(list_artifacts))
        .routes(routes!(get_channel_release))
}

// ---- helpers --------------------------------------------------------------

pub(crate) async fn find_release<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    version: &str,
) -> Result<release::Model, ApiError> {
    release::Entity::find()
        .filter(release::Column::AppId.eq(app_id))
        .filter(release::Column::Version.eq(version))
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)
}

/// 把 Draft release 翻成 Published（盖上 published_at）。幂等：非 Draft 原样返回。
/// 发布这一领域不变量的唯一来源——`publish_release` 与 `uploads::complete` 两条
/// 写路径共用，避免状态机复制后漂移。调用方负责事务边界与提交后的审计。
pub(crate) async fn mark_published<C: ConnectionTrait>(
    txn: &C,
    rel: release::Model,
) -> Result<release::Model, ApiError> {
    if rel.status != release::ReleaseStatus::Draft {
        return Ok(rel);
    }
    let mut am: release::ActiveModel = rel.into();
    am.status = Set(release::ReleaseStatus::Published);
    am.published_at = Set(Some(chrono::Utc::now()));
    Ok(am.update(txn).await?)
}

/// `finalize_publish` 的结果。
pub(crate) struct FinalizeOutcome {
    pub release: release::Model,
    /// 本次调用是否真的发生了 Draft→Published —— 用于决定是否写审计。幂等重复调用
    /// (已 published)为 `false`,不重复 emit / 审计。
    pub newly_published: bool,
}

/// 「发布」副作用的**唯一来源**:对 release 行加排他锁 → 幂等判定 → 校验 artifact ≥ 1
/// → `mark_published` → emit `ReleasePublished`。`finalize` 端点与过渡期的
/// `uploads::complete(publish=true)` 共用,避免发布语义在多处复制后漂移。
///
/// 这里的锁是 **release 级单次操作**(非 per-target):artifact 写入的并发安全已由原子
/// `ON CONFLICT` upsert + `uq_artifact_release_variant` 唯一索引保证,本锁只为「发布」
/// 这一终态转换防并发双发布 / 双通知。它取代了 `complete` 内把所有 artifact 写入串行
/// 化的临时悲观锁(`harden-publish-flow` D2)。
///
/// 调用方负责:在一个已 `begin` 的事务内调用(`lock_exclusive` 才生效)、按返回的
/// `newly_published` 决定提交后审计、出错时 rollback。
pub(crate) async fn finalize_publish<C: ConnectionTrait>(
    txn: &C,
    app_id: Uuid,
    app_slug: &str,
    release_id: Uuid,
) -> Result<FinalizeOutcome, ApiError> {
    let locked = release::Entity::find_by_id(release_id)
        .lock_exclusive()
        .one(txn)
        .await?
        .ok_or(ApiError::NotFound)?;
    match locked.status {
        // 幂等:已发布原样返回,不重复 emit / 审计。
        release::ReleaseStatus::Published => {
            return Ok(FinalizeOutcome {
                release: locked,
                newly_published: false,
            });
        }
        release::ReleaseStatus::Yanked => {
            return Err(ApiError::Conflict {
                detail: "cannot finalize a yanked release".into(),
            });
        }
        release::ReleaseStatus::Draft => {}
    }

    // finalize 前置:release 至少有一个 artifact,否则拒绝(不改发布状态)。
    let artifact_count = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(release_id))
        .count(txn)
        .await?;
    if artifact_count == 0 {
        return Err(ApiError::Validation {
            detail: "cannot finalize a release with no artifacts".into(),
        });
    }

    let updated = mark_published(txn, locked).await?;
    emit::emit(
        txn,
        NewEvent {
            event_type: api::NotificationEventType::ReleasePublished,
            app_id,
            data: notification_payload(app_slug, &updated, None),
        },
    )
    .await?;
    Ok(FinalizeOutcome {
        release: updated,
        newly_published: true,
    })
}

async fn find_channel<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    name: &str,
) -> Result<channel::Model, ApiError> {
    channel::Entity::find()
        .filter(channel::Column::AppId.eq(app_id))
        .filter(channel::Column::Name.eq(name))
        .one(db)
        .await?
        .ok_or(ApiError::NotFound)
}

fn nothing_to_rollback() -> ApiError {
    ApiError::typed(
        StatusCode::UNPROCESSABLE_ENTITY,
        "https://swarmhive.dev/errors/nothing-to-rollback",
        "Unprocessable Entity",
        "channel has no prior release to roll back to",
    )
}

pub(crate) fn notification_payload(
    app_slug: &str,
    rel: &release::Model,
    channel: Option<&str>,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "app_slug": app_slug,
        "version": rel.version,
        "notes": rel.release_notes,
    });
    if let Some(channel) = channel {
        payload["channel"] = serde_json::json!(channel);
    }
    payload
}

/// Move a channel's current-release pointer and append history, inside `txn`.
async fn apply_pointer_move<C: ConnectionTrait>(
    txn: &C,
    channel_id: Uuid,
    release_id: Uuid,
    actor_id: Uuid,
    action: channel_release_history::ChannelAction,
    reason: Option<String>,
) -> Result<(), ApiError> {
    match channel_release::Entity::find_by_id(channel_id)
        .one(txn)
        .await?
    {
        Some(existing) => {
            let mut am: channel_release::ActiveModel = existing.into();
            am.release_id = Set(release_id);
            am.updated_by = Set(actor_id);
            am.update(txn).await?;
        }
        None => {
            channel_release::ActiveModel {
                channel_id: Set(channel_id),
                release_id: Set(release_id),
                updated_at: NotSet,
                updated_by: Set(actor_id),
            }
            .insert(txn)
            .await?;
        }
    }
    channel_release_history::ActiveModel {
        id: Set(Uuid::now_v7()),
        channel_id: Set(channel_id),
        release_id: Set(release_id),
        action: Set(action),
        reason: Set(reason),
        actor_id: Set(actor_id),
        created_at: NotSet,
    }
    .insert(txn)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn audit_release(
    state: &AppState,
    principal: &Principal,
    app_id: Uuid,
    action: &str,
    release_id: Uuid,
    metadata: serde_json::Value,
) {
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: principal_actor_type(principal),
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: Some(app_id),
            action: action.into(),
            resource_type: Some("release".into()),
            resource_id: Some(release_id.to_string()),
            ip: None,
            user_agent: None,
            metadata,
        },
    )
    .await;
}

// ---- Release CRUD ---------------------------------------------------------

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}/releases",
    params(("slug" = String, Path, description = "App slug.")),
    responses((status = 200, body = Vec<api::Release>, description = "Releases of the app."), ApiErrorResponses),
    tag = "releases",
)]
async fn list_releases(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Vec<api::Release>>, ApiError> {
    require_permission!(principal, PermissionName::ReleaseRead, Scope::None)?;
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    let releases = release::Entity::find()
        .filter(release::Column::AppId.eq(app.id))
        .order_by_desc(release::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(releases.iter().map(api::Release::from).collect()))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases",
    params(("slug" = String, Path, description = "App slug.")),
    request_body = CreateReleaseRequest,
    responses((status = 201, body = api::Release, description = "Draft release created."), ApiErrorResponses),
    tag = "releases",
)]
async fn create_release(
    principal: Principal,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<CreateReleaseRequest>,
) -> Result<(StatusCode, Json<api::Release>), ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::ReleaseCreate, Scope::App(app.id))?;

    // 版本号必须是合法 semver(容忍单个前导 v),否则 Tauri 更新检查端点会解析失败并
    // 静默跳过该 release(永不分发)。从创建处杜绝坏值,而非分发时才 warn。
    if semver::Version::parse(req.version.strip_prefix('v').unwrap_or(&req.version)).is_err() {
        return Err(ApiError::typed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "https://swarmhive.dev/errors/invalid-release-version",
            "Unprocessable Entity",
            format!("release version '{}' is not valid SemVer", req.version),
        ));
    }

    if release::Entity::find()
        .filter(release::Column::AppId.eq(app.id))
        .filter(release::Column::Version.eq(&req.version))
        .one(&state.db)
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict {
            detail: format!("release version '{}' already exists", req.version),
        });
    }

    let model = release::ActiveModel {
        id: Set(Uuid::now_v7()),
        app_id: Set(app.id),
        version: Set(req.version.clone()),
        android_version_code: Set(req.android_version_code),
        android_min_version_code: Set(req.android_min_version_code),
        status: Set(release::ReleaseStatus::Draft),
        release_notes: Set(req.release_notes.clone()),
        published_at: Set(None),
        min_version: Set(None),
        rollout_percent: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(api::Release::from(&model))))
}

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}/releases/{version}",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    responses((status = 200, body = api::Release, description = "Release detail."), ApiErrorResponses),
    tag = "releases",
)]
async fn get_release(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<api::Release>, ApiError> {
    require_permission!(principal, PermissionName::ReleaseRead, Scope::None)?;
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    let rel = find_release(&state.db, app.id, &version).await?;
    Ok(Json(api::Release::from(&rel)))
}

#[utoipa::path(
    patch, path = "/api/v1/apps/{slug}/releases/{version}",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    request_body = UpdateReleaseRequest,
    responses((status = 200, body = api::Release, description = "Release updated."), ApiErrorResponses),
    tag = "releases",
)]
async fn update_release(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
    Json(req): Json<UpdateReleaseRequest>,
) -> Result<Json<api::Release>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::ReleaseUpdate, Scope::App(app.id))?;
    let rel = find_release(&state.db, app.id, &version).await?;

    let mut am: release::ActiveModel = rel.into();
    if let Some(code) = req.android_version_code {
        am.android_version_code = Set(Some(code));
    }
    // RN 强更下限(整数);Some 设值,absent/null 不改。调高即 kill switch。
    if let Some(code) = req.android_min_version_code {
        am.android_min_version_code = Set(Some(code));
    }
    if let Some(notes) = req.release_notes {
        am.release_notes = Set(Some(notes));
    }
    // 灰度百分比限 1..=100(0/越界 422)。单层 Option:absent/null 都视作不改,
    // 不支持 PATCH 回 NULL(清空走边界值 100,见 design D3)。
    if let Some(percent) = req.rollout_percent {
        if !(1..=100).contains(&percent) {
            return Err(ApiError::typed(
                StatusCode::UNPROCESSABLE_ENTITY,
                "https://swarmhive.dev/errors/invalid-rollout-percent",
                "Unprocessable Entity",
                format!("rollout_percent must be in 1..=100, got {percent}"),
            ));
        }
        am.rollout_percent = Set(Some(percent));
    }
    // min_version 非空时必须合法 semver(容忍单个前导 v);清空走 "0.0.0"。
    if let Some(mv) = req.min_version {
        if semver::Version::parse(mv.strip_prefix('v').unwrap_or(&mv)).is_err() {
            return Err(ApiError::typed(
                StatusCode::UNPROCESSABLE_ENTITY,
                "https://swarmhive.dev/errors/invalid-min-version",
                "Unprocessable Entity",
                format!("min_version '{mv}' is not valid SemVer"),
            ));
        }
        am.min_version = Set(Some(mv));
    }
    let updated = am.update(&state.db).await?;
    Ok(Json(api::Release::from(&updated)))
}

// ---- Lifecycle ------------------------------------------------------------

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases/{version}/publish",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    responses((status = 200, body = api::Release, description = "Release published."), ApiErrorResponses),
    tag = "releases",
)]
async fn publish_release(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<api::Release>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ReleasePublish,
        Scope::App(app.id)
    )?;
    let rel = find_release(&state.db, app.id, &version).await?;

    match rel.status {
        release::ReleaseStatus::Published => Ok(Json(api::Release::from(&rel))),
        release::ReleaseStatus::Yanked => Err(ApiError::Conflict {
            detail: "cannot publish a yanked release".into(),
        }),
        release::ReleaseStatus::Draft => {
            let txn = state.db.begin().await?;
            let updated = mark_published(&txn, rel).await?;
            emit::emit(
                &txn,
                NewEvent {
                    event_type: api::NotificationEventType::ReleasePublished,
                    app_id: app.id,
                    data: notification_payload(&slug, &updated, None),
                },
            )
            .await?;
            txn.commit().await?;
            audit_release(
                &state,
                &principal,
                app.id,
                "release_published",
                updated.id,
                serde_json::json!({ "version": version }),
            )
            .await;
            Ok(Json(api::Release::from(&updated)))
        }
    }
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases/{version}/finalize",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    responses((status = 200, body = api::Release, description = "Release finalized (published). Idempotent: re-finalizing an already-published release returns 200 unchanged. Rejected if the release has no artifacts."), ApiErrorResponses),
    tag = "releases",
)]
async fn finalize_release(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<api::Release>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ReleasePublish,
        Scope::App(app.id)
    )?;
    // 解析 version → release_id;真正的锁 + 幂等判定在 finalize_publish 的事务内完成。
    let rel = find_release(&state.db, app.id, &version).await?;

    let txn = state.db.begin().await?;
    let outcome = match finalize_publish(&txn, app.id, &slug, rel.id).await {
        Ok(outcome) => outcome,
        Err(e) => {
            txn.rollback().await?;
            return Err(e);
        }
    };
    txn.commit().await?;

    if outcome.newly_published {
        audit_release(
            &state,
            &principal,
            app.id,
            "release_published",
            outcome.release.id,
            serde_json::json!({ "version": version, "via": "finalize" }),
        )
        .await;
    }
    Ok(Json(api::Release::from(&outcome.release)))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/releases/{version}/yank",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    responses((status = 200, body = api::Release, description = "Release yanked."), ApiErrorResponses),
    tag = "releases",
)]
async fn yank_release(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<api::Release>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(principal, PermissionName::ReleaseYank, Scope::App(app.id))?;
    let rel = find_release(&state.db, app.id, &version).await?;

    match rel.status {
        release::ReleaseStatus::Yanked => Ok(Json(api::Release::from(&rel))),
        release::ReleaseStatus::Draft => Err(ApiError::Conflict {
            detail: "cannot yank a draft release".into(),
        }),
        release::ReleaseStatus::Published => {
            let release_id = rel.id;
            let mut am: release::ActiveModel = rel.into();
            am.status = Set(release::ReleaseStatus::Yanked);
            let updated = am.update(&state.db).await?;
            audit_release(
                &state,
                &principal,
                app.id,
                "release_yanked",
                release_id,
                serde_json::json!({ "version": version }),
            )
            .await;
            Ok(Json(api::Release::from(&updated)))
        }
    }
}

// ---- Channel pointer moves ------------------------------------------------

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/channels/{name}/promote",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("name" = String, Path, description = "Channel name."),
    ),
    request_body = PromoteRequest,
    responses((status = 200, body = api::Release, description = "Channel now serves the release."), ApiErrorResponses),
    tag = "releases",
)]
async fn promote(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, name)): Path<(String, String)>,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<api::Release>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ReleasePromote,
        Scope::App(app.id)
    )?;
    let chan = find_channel(&state.db, app.id, &name).await?;
    let rel = find_release(&state.db, app.id, &req.version).await?;
    if rel.status != release::ReleaseStatus::Published {
        return Err(ApiError::Conflict {
            detail: "only a published release can be promoted".into(),
        });
    }

    let txn = state.db.begin().await?;
    apply_pointer_move(
        &txn,
        chan.id,
        rel.id,
        principal.user_id,
        channel_release_history::ChannelAction::Promote,
        None,
    )
    .await?;
    emit::emit(
        &txn,
        NewEvent {
            event_type: api::NotificationEventType::ChannelPromoted,
            app_id: app.id,
            data: notification_payload(&slug, &rel, Some(&name)),
        },
    )
    .await?;
    txn.commit().await?;

    audit_release(
        &state,
        &principal,
        app.id,
        "release_promoted",
        rel.id,
        serde_json::json!({ "version": req.version, "channel": name }),
    )
    .await;
    Ok(Json(api::Release::from(&rel)))
}

#[utoipa::path(
    post, path = "/api/v1/apps/{slug}/channels/{name}/rollback",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("name" = String, Path, description = "Channel name."),
    ),
    request_body = RollbackRequest,
    responses((status = 200, body = api::Release, description = "Channel rolled back."), ApiErrorResponses),
    tag = "releases",
)]
async fn rollback(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, name)): Path<(String, String)>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<api::Release>, ApiError> {
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    require_permission!(
        principal,
        PermissionName::ReleaseRollback,
        Scope::App(app.id)
    )?;
    let chan = find_channel(&state.db, app.id, &name).await?;

    let target = match req.version {
        Some(version) => find_release(&state.db, app.id, &version).await?,
        None => {
            let current = channel_release::Entity::find_by_id(chan.id)
                .one(&state.db)
                .await?
                .ok_or_else(nothing_to_rollback)?;
            // 让 DB 只回一行：排除当前指向的 release，按时间倒序取最近一条历史。
            // created_at 相同的极快连续 move 用 id 兜底排序，保证选取确定。
            let prev = channel_release_history::Entity::find()
                .filter(channel_release_history::Column::ChannelId.eq(chan.id))
                .filter(channel_release_history::Column::ReleaseId.ne(current.release_id))
                .order_by_desc(channel_release_history::Column::CreatedAt)
                .order_by_desc(channel_release_history::Column::Id)
                .one(&state.db)
                .await?
                .ok_or_else(nothing_to_rollback)?;
            release::Entity::find_by_id(prev.release_id)
                .one(&state.db)
                .await?
                .ok_or(ApiError::NotFound)?
        }
    };

    let txn = state.db.begin().await?;
    apply_pointer_move(
        &txn,
        chan.id,
        target.id,
        principal.user_id,
        channel_release_history::ChannelAction::Rollback,
        None,
    )
    .await?;
    emit::emit(
        &txn,
        NewEvent {
            event_type: api::NotificationEventType::ChannelRolledBack,
            app_id: app.id,
            data: notification_payload(&slug, &target, Some(&name)),
        },
    )
    .await?;
    txn.commit().await?;

    audit_release(
        &state,
        &principal,
        app.id,
        "release_rolled_back",
        target.id,
        serde_json::json!({ "version": target.version, "channel": name }),
    )
    .await;
    Ok(Json(api::Release::from(&target)))
}

// ---- Reads ----------------------------------------------------------------

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}/channels/{name}/release",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("name" = String, Path, description = "Channel name."),
    ),
    responses((status = 200, body = Option<api::Release>, description = "Release the channel currently serves, or null."), ApiErrorResponses),
    tag = "releases",
)]
async fn get_channel_release(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, name)): Path<(String, String)>,
) -> Result<Json<Option<api::Release>>, ApiError> {
    require_permission!(principal, PermissionName::ReleaseRead, Scope::None)?;
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    let chan = find_channel(&state.db, app.id, &name).await?;

    let pointer = channel_release::Entity::find_by_id(chan.id)
        .one(&state.db)
        .await?;
    let Some(pointer) = pointer else {
        return Ok(Json(None));
    };
    let rel = release::Entity::find_by_id(pointer.release_id)
        .one(&state.db)
        .await?;
    Ok(Json(rel.as_ref().map(api::Release::from)))
}

#[utoipa::path(
    get, path = "/api/v1/apps/{slug}/releases/{version}/artifacts",
    params(
        ("slug" = String, Path, description = "App slug."),
        ("version" = String, Path, description = "Release version."),
    ),
    responses((status = 200, body = Vec<api::Artifact>, description = "Artifacts of the release."), ApiErrorResponses),
    tag = "releases",
)]
async fn list_artifacts(
    principal: Principal,
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> Result<Json<Vec<api::Artifact>>, ApiError> {
    require_permission!(principal, PermissionName::ArtifactRead, Scope::None)?;
    let app = find_app(&state.db, principal.org_id, &slug).await?;
    let rel = find_release(&state.db, app.id, &version).await?;
    let artifacts = artifact::Entity::find()
        .filter(artifact::Column::ReleaseId.eq(rel.id))
        .order_by_asc(artifact::Column::Filename)
        .all(&state.db)
        .await?;
    Ok(Json(artifacts.iter().map(api::Artifact::from).collect()))
}
