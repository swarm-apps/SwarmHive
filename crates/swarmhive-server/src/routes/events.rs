//! `POST /api/v1/events` — SDK 主动事件上报(`add-telemetry-events`)。
//!
//! 公开端点(挂 sensitive 子树受 governor 限流),**不可信源**:event 白名单、
//! app 必须存在、client_id 长度、error_message 截断逐项兜底;写库失败 swallow
//! 仍返 200(遥测 fire-and-forget,客户端不应重试)。设备数等关键指标只从
//! 可信的 update_event 统计,本表数据仅用于漏斗/失败率等自报指标。

use axum::Json;
use axum::extract::State;
use garde::Validate;
use sea_orm::ActiveValue::Set;
use swarmhive_api_types::{ClientEventKind, ReportEventReq};
use swarmhive_entity::client_event;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::{ApiError, ApiErrorResponses};
use crate::routes::apps::find_app_by_slug;
use crate::services::telemetry;
use crate::state::AppState;
use crate::validation::GardeJson;

/// error_message 落库上限(防自报字段撑爆行宽)。
const ERROR_MESSAGE_MAX: usize = 512;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(report_client_event))
}

/// 包一层 garde 校验(api-types 不依赖 garde,route-local 惯例)。
#[derive(Debug, serde::Deserialize, Validate, utoipa::ToSchema)]
pub struct ReportEventBody {
    #[garde(skip)]
    #[serde(flatten)]
    pub inner: ReportEventReq,
}

fn kind_to_entity(kind: ClientEventKind) -> client_event::ClientEventName {
    use client_event::ClientEventName as E;
    match kind {
        ClientEventKind::DownloadStarted => E::DownloadStarted,
        ClientEventKind::DownloadCompleted => E::DownloadCompleted,
        ClientEventKind::DownloadFailed => E::DownloadFailed,
        ClientEventKind::InstallStarted => E::InstallStarted,
        ClientEventKind::InstallFailed => E::InstallFailed,
        ClientEventKind::AppStartedAfterUpdate => E::AppStartedAfterUpdate,
    }
}

#[utoipa::path(
    post, path = "/api/v1/events",
    request_body = ReportEventBody,
    responses(
        (status = 200, description = "Event accepted (fire-and-forget; clients must not retry)."),
        ApiErrorResponses,
    ),
    tag = "telemetry",
)]
async fn report_client_event(
    State(state): State<AppState>,
    GardeJson(body): GardeJson<ReportEventBody>,
) -> Result<(), ApiError> {
    let req = body.inner;

    // 手工校验(serde flatten 与 garde 字段注解不兼容,集中在这里)。
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::Validation {
            detail: "client_id must be 1..=64 characters".into(),
        });
    }
    if req.platform.is_empty() || req.platform.len() > 64 {
        return Err(ApiError::Validation {
            detail: "platform must be 1..=64 characters".into(),
        });
    }
    if req.bytes_total.is_some_and(|v| v < 0) || req.duration_ms.is_some_and(|v| v < 0) {
        return Err(ApiError::Validation {
            detail: "bytes_total / duration_ms must be non-negative".into(),
        });
    }

    // app 必须存在(404)——这是唯一一处真校验外部状态;event 白名单由 serde enum 保证。
    let app = find_app_by_slug(&state.db, &req.app).await?;

    let truncate = |s: Option<String>, max: usize| {
        s.map(|v| {
            if v.len() > max {
                v.chars().take(max).collect()
            } else {
                v
            }
        })
    };

    let am = client_event::ActiveModel {
        id: Set(Uuid::now_v7()),
        event_name: Set(kind_to_entity(req.event)),
        app_id: Set(app.id),
        channel: Set(truncate(req.channel, 64)),
        target_version: Set(truncate(req.target_version, 64)),
        previous_version: Set(truncate(req.previous_version, 64)),
        platform: Set(req.platform),
        client_id: Set(req.client_id),
        bytes_total: Set(req.bytes_total),
        duration_ms: Set(req.duration_ms),
        error_code: Set(truncate(req.error_code, 64)),
        error_message: Set(truncate(req.error_message, ERROR_MESSAGE_MAX)),
        created_at: Set(chrono::Utc::now()),
    };
    // 写库失败 swallow,仍 200(fire-and-forget 契约)。
    telemetry::record_client_event(&state.db, am).await;
    Ok(())
}
