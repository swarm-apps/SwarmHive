//! `/api/v1/auth/device/*` — RFC 8628 OAuth 2.0 Device Authorization Grant。
//!
//! `swarmhive login` 用它替换旧的 ROPC `cli-token`：CLI 拿 `device_code` +
//! 用户可读 `user_code`，用户在浏览器 `/device` 页登录后批准，CLI 轮询
//! `/device/token` 换取 PAT。认证步骤复用 Web `/login`，故 OAuth-only 用户也能用 CLI。
//!
//! 设计要点（见 `openspec/changes/add-cli-device-login/design.md`）：
//! - **token 端点破例走 RFC 8628 wire 格式** `400 { "error": <code> }`，不走仓库的
//!   RFC 9457 problem+json——让标准 device-flow 客户端可互操作。infra 故障（DB）仍
//!   走 `ApiError` 的 500 problem+json。
//! - **approved→completed 原子 claim**：铸 PAT 前条件 UPDATE `WHERE status=approved`，
//!   `rows_affected==1` 才铸，防并发轮询双铸。
//! - **approve/deny/lookup 仅接受 Session**：拒 Bearer PAT 自批（钓鱼缓解）。
//! - **bootstrap window 排除**：user 表空时 `/device/code` 返 410。

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use chrono::Utc;
use rand::rngs::OsRng;
use rand::{Rng, RngCore};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde::Deserialize;
use swarmhive_api_types::{
    ApiTokenKind, CreateTokenRequest, DEVICE_GRANT_TYPE, DeviceAuthorizationView,
    DeviceCodeRequest, DeviceCodeResponse, DeviceTokenError, DeviceTokenErrorResponse,
    DeviceTokenRequest, DeviceTokenResponse, DeviceVerifyRequest,
};
use swarmhive_entity::device_authorization::{self, DeviceGrantStatus};
use swarmhive_entity::{audit_log, user};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::bootstrap;
use crate::auth::principal::{AuthMethod, Scope};
use crate::auth::service::{self, RequestCtx};
use crate::error::{ApiError, ApiErrorResponses};
use crate::services::audit::{self, AuditEntry};
use crate::services::token as token_service;
use crate::state::AppState;

/// 一等公民 public client，无 client_secret。
const CLIENT_ID: &str = "swarmhive-cli";
/// device_code 有效期。
const DEVICE_CODE_TTL_MINUTES: i64 = 15;
/// 轮询建议间隔（秒）。
const DEVICE_POLL_INTERVAL_SECS: i32 = 5;
/// RFC 8628 推荐的 base-20 辅音字母表（去掉易混元音 / 数字）。
const USER_CODE_ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
/// 活跃 user_code 碰撞时的重试上限（活跃集合极小，几乎不会触发）。
const USER_CODE_MAX_ATTEMPTS: usize = 8;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(device_code))
        .routes(routes!(device_token))
        .routes(routes!(device_lookup))
        .routes(routes!(device_approve))
        .routes(routes!(device_deny))
}

/// 构造 RFC 8628 token 端点错误响应（`400 { "error": <code> }`，非 problem+json）。
fn device_err(error: DeviceTokenError) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(DeviceTokenErrorResponse { error }),
    )
        .into_response()
}

/// 生成 32 字节高熵 device_code（base64url-no-pad）+ 其 blake3 hex。明文只回 CLI 一次。
fn gen_device_code() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let code = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let hash = blake3::hash(code.as_bytes()).to_hex().to_string();
    (code, hash)
}

/// 生成 8×base20 的用户码，格式 `WDJB-MJHT`。
fn gen_user_code() -> String {
    let raw: String = (0..8)
        .map(|_| USER_CODE_ALPHABET[OsRng.gen_range(0..USER_CODE_ALPHABET.len())] as char)
        .collect();
    format!("{}-{}", &raw[..4], &raw[4..])
}

/// approve / deny / lookup 仅接受浏览器 session 来源的 Principal，拒 Bearer PAT
/// 脱离浏览器自批（钓鱼缓解，见 design Decision 8）。
fn require_session(principal: &Principal) -> Result<(), ApiError> {
    if matches!(principal.auth_method, AuthMethod::Session { .. }) {
        Ok(())
    } else {
        Err(ApiError::typed(
            StatusCode::FORBIDDEN,
            "https://swarmhive.dev/errors/session-required",
            "Forbidden",
            "Device approval requires an interactive browser session, not an API token.",
        ))
    }
}

#[utoipa::path(
    post, path = "/api/v1/auth/device/code",
    request_body = DeviceCodeRequest,
    responses(
        (status = 200, body = DeviceCodeResponse, description = "Device + user codes issued."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn device_code(
    State(state): State<AppState>,
    Json(req): Json<DeviceCodeRequest>,
) -> Result<Json<DeviceCodeResponse>, ApiError> {
    // bootstrap window 期间没有任何已存在用户能进批准页（批准页 → /login → 空 DB
    // 全路径跳 /setup），发码只会让 CLI 静默轮询到超时。即时报错引导先建 Owner。
    let st = bootstrap::bootstrap_state(&state.db, &state.bootstrap).await?;
    if st.needs_bootstrap {
        return Err(ApiError::typed(
            StatusCode::GONE,
            "https://swarmhive.dev/errors/device-not-available-during-bootstrap",
            "Gone",
            "Device login is unavailable until the first owner is created. Complete setup in the web UI first.",
        ));
    }

    let now = Utc::now();
    // lazy 清理：带 1h grace，保住刚过期的行仍能返回 expired_token（而非误删成
    // not-found → invalid_grant）。只删早已过期的行。
    let cutoff = now - chrono::Duration::hours(1);
    device_authorization::Entity::delete_many()
        .filter(device_authorization::Column::ExpiresAt.lt(cutoff))
        .exec(&state.db)
        .await?;

    let (device_code, device_code_hash) = gen_device_code();

    // user_code 活跃集合内唯一：生成时校验 + 重试（不装 partial unique index）。
    let mut user_code = None;
    for _ in 0..USER_CODE_MAX_ATTEMPTS {
        let candidate = gen_user_code();
        let active = device_authorization::Entity::find()
            .filter(device_authorization::Column::UserCode.eq(candidate.clone()))
            .filter(device_authorization::Column::Status.eq(DeviceGrantStatus::Pending))
            .filter(device_authorization::Column::ExpiresAt.gt(now))
            .count(&state.db)
            .await?;
        if active == 0 {
            user_code = Some(candidate);
            break;
        }
    }
    let user_code = user_code.ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("could not allocate a unique user_code"))
    })?;

    let token_name = req
        .token_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| CLIENT_ID.to_string());

    device_authorization::ActiveModel {
        id: Set(Uuid::now_v7()),
        device_code_hash: Set(device_code_hash),
        user_code: Set(user_code.clone()),
        client_id: Set(req.client_id.clone()),
        client_name: Set(Some(token_name.clone())),
        token_name: Set(token_name),
        scope: Set(req.scope.clone()),
        status: Set(DeviceGrantStatus::Pending),
        user_id: Set(None),
        interval_secs: Set(DEVICE_POLL_INTERVAL_SECS),
        last_polled_at: Set(None),
        approved_at: Set(None),
        expires_at: Set(now + chrono::Duration::minutes(DEVICE_CODE_TTL_MINUTES)),
        created_at: NotSet,
    }
    .insert(&state.db)
    .await?;

    let base = state.config.server.base_url.trim_end_matches('/');
    Ok(Json(DeviceCodeResponse {
        verification_uri: format!("{base}/device"),
        verification_uri_complete: format!("{base}/device?user_code={user_code}"),
        device_code,
        user_code,
        expires_in: DEVICE_CODE_TTL_MINUTES * 60,
        interval: DEVICE_POLL_INTERVAL_SECS as i64,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/auth/device/token",
    request_body = DeviceTokenRequest,
    responses(
        (status = 200, body = DeviceTokenResponse, description = "Grant approved — PAT minted (plaintext returned once)."),
        (status = 400, body = DeviceTokenErrorResponse, description = "RFC 8628 OAuth error (authorization_pending / slow_down / access_denied / expired_token / invalid_grant / unsupported_grant_type / invalid_request)."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn device_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceTokenRequest>,
) -> Result<Response, ApiError> {
    // 请求校验。
    if req.grant_type != DEVICE_GRANT_TYPE {
        return Ok(device_err(DeviceTokenError::UnsupportedGrantType));
    }
    if req.client_id.is_empty() || req.client_id != CLIENT_ID {
        return Ok(device_err(DeviceTokenError::InvalidRequest));
    }

    let hash = blake3::hash(req.device_code.as_bytes())
        .to_hex()
        .to_string();
    // not-found（含已被 lazy 清理的行）必须先于任何 status 分支 → invalid_grant。
    let Some(row) = device_authorization::Entity::find()
        .filter(device_authorization::Column::DeviceCodeHash.eq(hash))
        .one(&state.db)
        .await?
    else {
        return Ok(device_err(DeviceTokenError::InvalidGrant));
    };

    let now = Utc::now();
    if row.expires_at < now {
        return Ok(device_err(DeviceTokenError::ExpiredToken));
    }

    // slow_down 闸门：仅当此前轮询过（last_polled_at 非 null）且间隔过短才拒，
    // 且**不刷新** last_polled_at（避免边界抖动活锁）。
    if let Some(last) = row.last_polled_at
        && (now - last) < chrono::Duration::seconds(row.interval_secs as i64)
    {
        return Ok(device_err(DeviceTokenError::SlowDown));
    }

    match row.status {
        DeviceGrantStatus::Denied => Ok(device_err(DeviceTokenError::AccessDenied)),
        DeviceGrantStatus::Completed => Ok(device_err(DeviceTokenError::InvalidGrant)),
        DeviceGrantStatus::Pending => {
            // 通过 slow_down 闸门 → 记录本次轮询时刻，供下次判定。
            device_authorization::Entity::update_many()
                .col_expr(
                    device_authorization::Column::LastPolledAt,
                    Expr::value(Some(now)),
                )
                .filter(device_authorization::Column::Id.eq(row.id))
                .exec(&state.db)
                .await?;
            Ok(device_err(DeviceTokenError::AuthorizationPending))
        }
        DeviceGrantStatus::Approved => {
            // 原子 claim：只有仍是 approved 才抢到铸造权，rows_affected==1 才继续。
            let claimed = device_authorization::Entity::update_many()
                .col_expr(
                    device_authorization::Column::Status,
                    Expr::value(DeviceGrantStatus::Completed),
                )
                .filter(device_authorization::Column::Id.eq(row.id))
                .filter(device_authorization::Column::Status.eq(DeviceGrantStatus::Approved))
                .exec(&state.db)
                .await?;
            if claimed.rows_affected != 1 {
                return Ok(device_err(DeviceTokenError::InvalidGrant));
            }

            let Some(user_id) = row.user_id else {
                return Ok(device_err(DeviceTokenError::InvalidGrant));
            };
            let Some(user_row) = user::Entity::find_by_id(user_id).one(&state.db).await? else {
                return Ok(device_err(DeviceTokenError::InvalidGrant));
            };

            // 临时 Principal（同 token 创建路径）：PAT 继承 owner 实时权限。
            let perms: HashSet<_> = service::load_user_permissions(&state.db, user_row.id).await?;
            let creator = Principal {
                user_id: user_row.id,
                org_id: user_row.org_id,
                scope: Scope::None,
                permissions: perms,
                auth_method: AuthMethod::Session {
                    session_id: Uuid::nil(),
                },
            };
            let ctx = RequestCtx::from_headers(&headers);
            let created = token_service::create(
                &state.db,
                &creator,
                CreateTokenRequest {
                    kind: ApiTokenKind::Pat,
                    name: row.token_name.clone(),
                    permissions: None,
                    expires_at: None,
                },
                &ctx,
            )
            .await?;

            Ok((
                StatusCode::OK,
                Json(DeviceTokenResponse {
                    token: created.plaintext,
                    name: created.api_token.name,
                    kind: created.api_token.kind,
                    created_at: created.api_token.created_at,
                }),
            )
                .into_response())
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct LookupQuery {
    /// 用户码，形如 `WDJB-MJHT`。
    user_code: String,
}

#[utoipa::path(
    get, path = "/api/v1/auth/device/lookup",
    params(LookupQuery),
    responses(
        (status = 200, body = DeviceAuthorizationView, description = "Pending grant details for the approval page."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn device_lookup(
    principal: Principal,
    State(state): State<AppState>,
    Query(q): Query<LookupQuery>,
) -> Result<Json<DeviceAuthorizationView>, ApiError> {
    require_session(&principal)?;
    let row = find_active(&state, &q.user_code)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(DeviceAuthorizationView {
        user_code: row.user_code,
        client_id: row.client_id,
        client_name: row.client_name,
        created_at: row.created_at,
        expires_at: row.expires_at,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/auth/device/approve",
    request_body = DeviceVerifyRequest,
    responses(
        (status = 204, description = "Grant approved."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn device_approve(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceVerifyRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&principal)?;
    let row = find_active(&state, &req.user_code)
        .await?
        .ok_or(ApiError::NotFound)?;

    match row.status {
        // 幂等：已被本人/任意人批准则当成功（双击）。
        DeviceGrantStatus::Approved => return Ok(StatusCode::NO_CONTENT),
        DeviceGrantStatus::Denied | DeviceGrantStatus::Completed => {
            return Err(ApiError::Conflict {
                detail: "device grant already resolved".into(),
            });
        }
        DeviceGrantStatus::Pending => {}
    }

    let now = Utc::now();
    // 仅 clone 审计要用的 user_code，再用 row 构造 ActiveModel（省一次整行 Model clone）。
    let user_code = row.user_code.clone();
    let mut am: device_authorization::ActiveModel = row.into();
    am.status = Set(DeviceGrantStatus::Approved);
    am.user_id = Set(Some(principal.user_id));
    am.approved_at = Set(Some(now));
    am.update(&state.db).await?;

    audit_device(
        &state,
        &principal,
        &headers,
        "auth:device_authorized",
        &user_code,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/v1/auth/device/deny",
    request_body = DeviceVerifyRequest,
    responses(
        (status = 204, description = "Grant denied."),
        ApiErrorResponses,
    ),
    tag = "auth",
)]
async fn device_deny(
    principal: Principal,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceVerifyRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&principal)?;
    let row = find_active(&state, &req.user_code)
        .await?
        .ok_or(ApiError::NotFound)?;

    match row.status {
        DeviceGrantStatus::Denied => return Ok(StatusCode::NO_CONTENT),
        DeviceGrantStatus::Approved | DeviceGrantStatus::Completed => {
            return Err(ApiError::Conflict {
                detail: "device grant already resolved".into(),
            });
        }
        DeviceGrantStatus::Pending => {}
    }

    let user_code = row.user_code.clone();
    let mut am: device_authorization::ActiveModel = row.into();
    am.status = Set(DeviceGrantStatus::Denied);
    am.update(&state.db).await?;

    audit_device(
        &state,
        &principal,
        &headers,
        "auth:device_denied",
        &user_code,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// 查活跃（未过期）行。过期与不存在同形（返回 None → 404，不可枚举）。
async fn find_active(
    state: &AppState,
    user_code: &str,
) -> Result<Option<device_authorization::Model>, ApiError> {
    let now = Utc::now();
    Ok(device_authorization::Entity::find()
        .filter(device_authorization::Column::UserCode.eq(user_code))
        .filter(device_authorization::Column::ExpiresAt.gt(now))
        .one(&state.db)
        .await?)
}

/// 写 approve/deny 的审计行（best-effort，不因审计失败而让用户操作失败）。
async fn audit_device(
    state: &AppState,
    principal: &Principal,
    headers: &HeaderMap,
    action: &str,
    user_code: &str,
) {
    let ctx = RequestCtx::from_headers(headers);
    audit::write_swallowing(
        &state.db,
        AuditEntry {
            actor_type: audit_log::ActorType::User,
            actor_id: Some(principal.user_id),
            org_id: principal.org_id,
            app_id: None,
            action: action.to_string(),
            resource_type: Some("device_authorization".into()),
            resource_id: Some(user_code.to_string()),
            ip: ctx.ip.clone(),
            user_agent: ctx.user_agent.clone(),
            metadata: serde_json::json!({ "user_code": user_code }),
        },
    )
    .await;
}
