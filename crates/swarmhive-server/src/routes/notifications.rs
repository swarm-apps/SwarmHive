//! `/api/v1/notifications/*` — notification subscriptions, webhook endpoints,
//! and delivery log management.
//!
//! All routes require `notification:manage`. Webhook signing secrets are
//! encrypted at rest and only returned on create / rotate.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder,
    QuerySelect, TransactionTrait,
};
use serde::Deserialize;
use serde_json::json;
use swarmhive_api_types::{self as api, PermissionName};
use swarmhive_entity::{
    app, notification_delivery, notification_delivery_attempt, notification_subscription,
    webhook_endpoint,
};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::Principal;
use crate::auth::principal::Scope;
use crate::error::{ApiError, ApiErrorResponses};
use crate::notify::channel::{
    DeliveryRequest, DeliveryTarget, NotificationChannel, WebhookChannel, validate_webhook_url,
};
use crate::notify::signer;
use crate::require_permission;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_subscriptions, create_subscription))
        .routes(routes!(delete_subscription))
        .routes(routes!(list_webhook_endpoints, create_webhook_endpoint))
        .routes(routes!(update_webhook_endpoint, delete_webhook_endpoint))
        .routes(routes!(rotate_webhook_secret))
        .routes(routes!(test_webhook_endpoint))
        .routes(routes!(list_deliveries))
        .routes(routes!(get_delivery))
        .routes(routes!(redeliver))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeliveriesQuery {
    pub webhook_endpoint_id: Option<Uuid>,
    pub status: Option<api::DeliveryStatus>,
    #[param(default = 50, maximum = 200)]
    pub limit: Option<u64>,
}

fn require_manage(principal: &Principal) -> Result<(), ApiError> {
    require_permission!(principal, PermissionName::NotificationManage, Scope::None)
}

fn validation(detail: impl Into<String>) -> ApiError {
    ApiError::Validation {
        detail: detail.into(),
    }
}

async fn ensure_app_scope(
    state: &AppState,
    principal: &Principal,
    app_id: Uuid,
) -> Result<(), ApiError> {
    app::Entity::find()
        .filter(app::Column::Id.eq(app_id))
        .filter(app::Column::OrgId.eq(principal.org_id))
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

fn clean_optional_string(input: Option<String>) -> Option<String> {
    input
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[utoipa::path(
    get, path = "/api/v1/notifications/subscriptions",
    responses((status = 200, body = Vec<api::Subscription>, description = "Notification subscriptions."), ApiErrorResponses),
    tag = "notifications",
)]
async fn list_subscriptions(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::Subscription>>, ApiError> {
    require_manage(&principal)?;
    let rows = notification_subscription::Entity::find()
        .order_by_desc(notification_subscription::Column::CreatedAt)
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(api::Subscription::from).collect()))
}

#[utoipa::path(
    post, path = "/api/v1/notifications/subscriptions",
    request_body = api::CreateSubscriptionReq,
    responses((status = 201, body = api::Subscription, description = "Subscription created."), ApiErrorResponses),
    tag = "notifications",
)]
async fn create_subscription(
    principal: Principal,
    State(state): State<AppState>,
    Json(req): Json<api::CreateSubscriptionReq>,
) -> Result<(StatusCode, Json<api::Subscription>), ApiError> {
    require_manage(&principal)?;
    if let Some(app_id) = req.app_id {
        ensure_app_scope(&state, &principal, app_id).await?;
    }

    let email_to = clean_optional_string(req.email_to);
    let (webhook_endpoint_id, email_to) = match req.channel_kind {
        api::NotificationChannelKind::Email => {
            let to = email_to
                .ok_or_else(|| validation("email_to is required for email subscriptions"))?;
            if !to.contains('@') {
                return Err(validation("email_to must be an email address"));
            }
            if req.webhook_endpoint_id.is_some() {
                return Err(validation(
                    "webhook_endpoint_id must be empty for email subscriptions",
                ));
            }
            (None, Some(to))
        }
        api::NotificationChannelKind::Webhook => {
            let endpoint_id = req.webhook_endpoint_id.ok_or_else(|| {
                validation("webhook_endpoint_id is required for webhook subscriptions")
            })?;
            webhook_endpoint::Entity::find_by_id(endpoint_id)
                .one(&state.db)
                .await?
                .ok_or(ApiError::NotFound)?;
            if email_to.is_some() {
                return Err(validation(
                    "email_to must be empty for webhook subscriptions",
                ));
            }
            (Some(endpoint_id), None)
        }
    };

    let model = notification_subscription::ActiveModel {
        id: Set(Uuid::now_v7()),
        event_type: Set(req.event_type.into()),
        app_id: Set(req.app_id),
        channel_kind: Set(req.channel_kind.into()),
        webhook_endpoint_id: Set(webhook_endpoint_id),
        email_to: Set(email_to),
        created_at: Set(Utc::now()),
    }
    .insert(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(api::Subscription::from(&model))))
}

#[utoipa::path(
    delete, path = "/api/v1/notifications/subscriptions/{id}",
    params(("id" = Uuid, Path, description = "Subscription id.")),
    responses((status = 204, description = "Subscription deleted."), ApiErrorResponses),
    tag = "notifications",
)]
async fn delete_subscription(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_manage(&principal)?;
    let res = notification_subscription::Entity::delete_by_id(id)
        .exec(&state.db)
        .await?;
    if res.rows_affected == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/api/v1/notifications/webhook-endpoints",
    responses((status = 200, body = Vec<api::WebhookEndpoint>, description = "Webhook endpoints. Secrets are never included."), ApiErrorResponses),
    tag = "notifications",
)]
async fn list_webhook_endpoints(
    principal: Principal,
    State(state): State<AppState>,
) -> Result<Json<Vec<api::WebhookEndpoint>>, ApiError> {
    require_manage(&principal)?;
    let rows = webhook_endpoint::Entity::find()
        .order_by_asc(webhook_endpoint::Column::Name)
        .all(&state.db)
        .await?;
    Ok(Json(rows.iter().map(api::WebhookEndpoint::from).collect()))
}

#[utoipa::path(
    post, path = "/api/v1/notifications/webhook-endpoints",
    request_body = api::CreateWebhookEndpointReq,
    responses((status = 201, body = api::CreateWebhookEndpointResp, description = "Webhook endpoint created; signing secret is returned exactly once."), ApiErrorResponses),
    tag = "notifications",
)]
async fn create_webhook_endpoint(
    principal: Principal,
    State(state): State<AppState>,
    Json(req): Json<api::CreateWebhookEndpointReq>,
) -> Result<(StatusCode, Json<api::CreateWebhookEndpointResp>), ApiError> {
    require_manage(&principal)?;
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(validation("name is required"));
    }
    let url = req.url.trim().to_string();
    validate_webhook_url(&url).map_err(|err| validation(err.to_string()))?;

    let provider_kind = req.provider_kind;
    // generic:SwarmHive 生成 whsec_,一次性返回。IM(飞书/钉钉):存用户提供的可选加签密钥
    // (slack/discord 无密钥),resp.secret 为空(无 SwarmHive 密钥可揭示)。
    let (secret_to_store, secret_to_reveal) = match provider_kind {
        api::WebhookProviderKind::Generic => {
            let generated = signer::generate_secret();
            (generated.clone(), generated)
        }
        _ => (req.secret.unwrap_or_default(), String::new()),
    };
    let secret_encrypted = state.secret_key.encrypt(&secret_to_store)?;
    let model = webhook_endpoint::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set(name),
        url: Set(url),
        provider_kind: Set(Some(webhook_endpoint::ProviderKind::from(provider_kind))),
        secret_encrypted: Set(secret_encrypted),
        previous_secret_encrypted: NotSet,
        previous_secret_expires_at: NotSet,
        failing_since: NotSet,
        disabled: Set(false),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(api::CreateWebhookEndpointResp {
            endpoint: api::WebhookEndpoint::from(&model),
            secret: secret_to_reveal,
        }),
    ))
}

#[utoipa::path(
    patch, path = "/api/v1/notifications/webhook-endpoints/{id}",
    params(("id" = Uuid, Path, description = "Webhook endpoint id.")),
    request_body = api::UpdateWebhookEndpointReq,
    responses((status = 200, body = api::WebhookEndpoint, description = "Webhook endpoint metadata updated; signing secret is never returned."), ApiErrorResponses),
    tag = "notifications",
)]
async fn update_webhook_endpoint(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<api::UpdateWebhookEndpointReq>,
) -> Result<Json<api::WebhookEndpoint>, ApiError> {
    require_manage(&principal)?;
    let existing = webhook_endpoint::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut am = existing.into_active_model();

    if let Some(name) = req.name {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(validation("name is required"));
        }
        am.name = Set(name);
    }
    if let Some(url) = req.url {
        let url = url.trim().to_string();
        validate_webhook_url(&url).map_err(|err| validation(err.to_string()))?;
        am.url = Set(url);
    }
    if let Some(disabled) = req.disabled {
        am.disabled = Set(disabled);
        // 任何显式的手动 disabled 变更(停用或启用)都清失败健康标记 —— 这样
        // 「disabled && failing_since」只由 worker 自动停用产生,UI / CLI 据此可靠区分
        // 「因失败自动停用」与「手动停用」,且重新启用即重置自动停用窗口。
        am.failing_since = Set(None);
    }

    let saved = am.update(&state.db).await?;
    Ok(Json(api::WebhookEndpoint::from(&saved)))
}

#[utoipa::path(
    delete, path = "/api/v1/notifications/webhook-endpoints/{id}",
    params(("id" = Uuid, Path, description = "Webhook endpoint id.")),
    responses((status = 204, description = "Webhook endpoint deleted. Subscriptions pointing to it are removed too."), ApiErrorResponses),
    tag = "notifications",
)]
async fn delete_webhook_endpoint(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_manage(&principal)?;
    let txn = state.db.begin().await?;
    notification_subscription::Entity::delete_many()
        .filter(notification_subscription::Column::WebhookEndpointId.eq(id))
        .exec(&txn)
        .await?;
    let res = webhook_endpoint::Entity::delete_by_id(id)
        .exec(&txn)
        .await?;
    if res.rows_affected == 0 {
        txn.rollback().await?;
        return Err(ApiError::NotFound);
    }
    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 密钥轮换宽限期:旧密钥保留多久参与双签(Standard Webhooks 零停机轮换)。
const ROTATION_GRACE_HOURS: i64 = 24;

#[utoipa::path(
    post, path = "/api/v1/notifications/webhook-endpoints/{id}/rotate-secret",
    params(("id" = Uuid, Path, description = "Webhook endpoint id.")),
    responses((status = 200, body = api::RotateSecretResp, description = "Signing secret rotated and returned exactly once."), ApiErrorResponses),
    tag = "notifications",
)]
async fn rotate_webhook_secret(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<api::RotateSecretResp>, ApiError> {
    require_manage(&principal)?;
    let existing = webhook_endpoint::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // 轮换只适用于 generic(whsec_);IM 的加签密钥是用户自有,改用 delete + recreate。
    if existing
        .provider_kind
        .map(api::WebhookProviderKind::from)
        .unwrap_or_default()
        != api::WebhookProviderKind::Generic
    {
        return Err(validation(
            "secret rotation applies to generic webhook endpoints only",
        ));
    }
    // 宽限期内禁止再次轮换:只有一个 previous slot,二次轮换会覆盖上一把旧密钥,让仍在
    // 用更早密钥的接收端立刻验签失败。要求等当前宽限期结束(接收端切换完)再轮换。
    // 首次轮换 `previous_secret_expires_at` 为 None,if-let 不匹配自动放行。判据 `> now` 与
    // worker 双签定义(worker.rs:`expires_at > now` 才双签)一致——边界时刻两处都视作宽限
    // 结束(允许再轮换 + 单签),自洽。属资源状态冲突 → 409(与 releases.rs 状态约束一致)。
    if let Some(expires_at) = existing.previous_secret_expires_at
        && expires_at > Utc::now()
    {
        return Err(ApiError::Conflict {
            detail: "a previous secret is still within its rotation grace window; wait for it to expire before rotating again"
                .into(),
        });
    }
    let secret = signer::generate_secret();
    // 旧密钥移入 previous + 宽限 24h:期间投递新旧双签,接收端零停机切换。
    let previous_encrypted = existing.secret_encrypted.clone();
    let mut am = existing.into_active_model();
    am.secret_encrypted = Set(state.secret_key.encrypt(&secret)?);
    am.previous_secret_encrypted = Set(Some(previous_encrypted));
    am.previous_secret_expires_at = Set(Some(
        Utc::now() + chrono::Duration::hours(ROTATION_GRACE_HOURS),
    ));
    let saved = am.update(&state.db).await?;
    Ok(Json(api::RotateSecretResp {
        id: saved.id,
        secret,
    }))
}

#[utoipa::path(
    post, path = "/api/v1/notifications/webhook-endpoints/{id}/test",
    params(("id" = Uuid, Path, description = "Webhook endpoint id.")),
    responses((status = 200, body = api::WebhookEndpointTestResp, description = "One signed webhook.test request was attempted without creating a delivery log entry."), ApiErrorResponses),
    tag = "notifications",
)]
async fn test_webhook_endpoint(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<api::WebhookEndpointTestResp>, ApiError> {
    require_manage(&principal)?;
    let endpoint = webhook_endpoint::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;

    let secret = match state.secret_key.decrypt(&endpoint.secret_encrypted) {
        Ok(secret) => secret,
        Err(err) => {
            return Ok(Json(webhook_test_failed(
                None,
                format!("webhook secret decrypt failed: {err}"),
            )));
        }
    };

    let provider_kind = endpoint
        .provider_kind
        .map(api::WebhookProviderKind::from)
        .unwrap_or_default();
    let channel = WebhookChannel::new();
    let result = match provider_kind {
        api::WebhookProviderKind::Generic => {
            // generic:发一条 webhook.test(Standard Webhooks 单签,无需双签),success 看 HTTP 2xx。
            let event_id = Uuid::now_v7();
            let body = serde_json::to_string(&json!({
                "id": event_id,
                "type": "webhook.test",
                "source": "swarmhive",
                "time": Utc::now(),
                "data": { "test": true, "endpoint_id": endpoint.id, "endpoint_name": endpoint.name }
            }))
            .map_err(|err| {
                ApiError::Internal(anyhow::anyhow!("serialize webhook test event: {err}"))
            })?;
            channel
                .deliver_payload(&endpoint.url, &secret, None, &event_id.to_string(), body)
                .await
        }
        kind => {
            // IM:合成一条测试事件,走与真实投递完全相同的 deliver_im(平台原生消息 + 平台加签
            // + is_im_success 判定),避免用 generic 路径误把飞书/钉钉的 code!=0 当成功。
            let event = api::NotificationEvent {
                id: Uuid::now_v7(),
                event_type: api::NotificationEventType::ReleasePublished,
                source: "swarmhive".into(),
                time: Utc::now(),
                data: json!({
                    "app_slug": "(test)", "version": "(test)", "channel": "(test)",
                    "notes": "SwarmHive webhook test."
                }),
            };
            channel
                .deliver(DeliveryRequest {
                    event,
                    target: DeliveryTarget::Webhook {
                        url: endpoint.url.clone(),
                        secret,
                        previous_secret: None,
                        provider_kind: kind,
                    },
                })
                .await
        }
    };
    let resp = match result {
        Ok(outcome) => api::WebhookEndpointTestResp {
            ok: true,
            response_code: outcome.response_code,
            detail: "webhook test request succeeded".into(),
        },
        Err(err) => webhook_test_failed(err.response_code, err.message),
    };
    Ok(Json(resp))
}

fn webhook_test_failed(
    response_code: Option<i32>,
    detail: impl Into<String>,
) -> api::WebhookEndpointTestResp {
    api::WebhookEndpointTestResp {
        ok: false,
        response_code,
        detail: detail.into(),
    }
}

#[utoipa::path(
    get, path = "/api/v1/notifications/deliveries",
    params(DeliveriesQuery),
    responses((status = 200, body = Vec<api::Delivery>, description = "Delivery log entries."), ApiErrorResponses),
    tag = "notifications",
)]
async fn list_deliveries(
    principal: Principal,
    State(state): State<AppState>,
    Query(query): Query<DeliveriesQuery>,
) -> Result<Json<Vec<api::Delivery>>, ApiError> {
    require_manage(&principal)?;
    let mut q = notification_delivery::Entity::find()
        .order_by_desc(notification_delivery::Column::UpdatedAt)
        .limit(query.limit.unwrap_or(50).min(200));
    if let Some(endpoint_id) = query.webhook_endpoint_id {
        q = q.filter(notification_delivery::Column::WebhookEndpointId.eq(endpoint_id));
    }
    if let Some(status) = query.status {
        q = q.filter(
            notification_delivery::Column::Status
                .eq(notification_delivery::DeliveryStatus::from(status)),
        );
    }
    let rows = q.all(&state.db).await?;
    Ok(Json(rows.iter().map(api::Delivery::from).collect()))
}

#[utoipa::path(
    get, path = "/api/v1/notifications/deliveries/{id}",
    params(("id" = Uuid, Path, description = "Delivery id.")),
    responses((status = 200, body = api::DeliveryDetail, description = "Delivery plus its request/response snapshot."), ApiErrorResponses),
    tag = "notifications",
)]
async fn get_delivery(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<api::DeliveryDetail>, ApiError> {
    require_manage(&principal)?;
    let delivery = notification_delivery::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    // 连带加载逐次尝试时间线(按 attempt_no 升序)。
    let attempts = notification_delivery_attempt::Entity::find()
        .filter(notification_delivery_attempt::Column::DeliveryId.eq(id))
        .order_by_asc(notification_delivery_attempt::Column::AttemptNo)
        .all(&state.db)
        .await?;
    let mut detail = api::DeliveryDetail::from(&delivery);
    detail.attempts = attempts.iter().map(api::DeliveryAttempt::from).collect();
    Ok(Json(detail))
}

#[utoipa::path(
    post, path = "/api/v1/notifications/deliveries/{id}/attempts",
    params(("id" = Uuid, Path, description = "Delivery id.")),
    responses((status = 200, body = api::Delivery, description = "Delivery re-enqueued for manual redelivery; webhook-id is preserved."), ApiErrorResponses),
    tag = "notifications",
)]
async fn redeliver(
    principal: Principal,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<api::Delivery>, ApiError> {
    require_manage(&principal)?;
    let delivery = notification_delivery::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut am = delivery.into_active_model();
    am.status = Set(notification_delivery::DeliveryStatus::Pending);
    am.response_code = Set(None);
    am.last_error = Set(None);
    am.next_retry_at = Set(None);
    // 与上面清 response_code/last_error 一致:置回 pending 时一并清旧投递快照,
    // 避免 pending 状态下详情仍展示上一次投递的请求/响应(下次投递会重新写)。
    am.request_body = Set(None);
    am.request_timestamp = Set(None);
    am.request_signature = Set(None);
    am.response_body = Set(None);
    let saved = am.update(&state.db).await?;
    Ok(Json(api::Delivery::from(&saved)))
}
