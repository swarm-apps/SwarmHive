//! Server error type with RFC 9457 `application/problem+json` response.
//!
//! Domain errors implement `From<…> for ApiError`; handlers return
//! `Result<T, ApiError>`. The `IntoResponse` impl emits problem+json with
//! a stable `type` URI per variant so clients (Admin SPA, CLI) can branch.
//!
//! For OpenAPI documentation, [`ApiErrorResponses`] is a sibling enum that
//! derives `utoipa::IntoResponses` and enumerates the 7 status codes this
//! error type can produce. Handlers reference it once in their
//! `#[utoipa::path(... responses(..., ApiErrorResponses))]` to get the full
//! error matrix in the generated OpenAPI doc without per-handler boilerplate.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sea_orm::DbErr;
use serde::Serialize;
use serde_json::json;
use utoipa::{IntoResponses, ToSchema};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("database error: {0}")]
    Db(#[from] DbErr),

    #[error("configuration error: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: missing permission {required_permission}")]
    Forbidden { required_permission: String },

    #[error("resource not found")]
    NotFound,

    #[error("validation failed: {detail}")]
    Validation { detail: String },

    #[error("conflict: {detail}")]
    Conflict { detail: String },

    #[error("resource is gone: {detail}")]
    Gone { detail: String },

    /// Escape hatch for endpoints that need a stable problem `type` URI
    /// distinguishable from the generic `gone` / `conflict` / `validation`
    /// buckets above. Used by bootstrap (`bootstrap_already_complete`,
    /// `bootstrap_email_mismatch`) and `account_locked_until`, so the SPA
    /// can branch on `type` without parsing the human `detail` string.
    /// `status` MUST be one of the codes enumerated in `ApiErrorResponses`.
    #[error("{detail}")]
    Typed {
        status: StatusCode,
        type_uri: &'static str,
        title: &'static str,
        detail: String,
        /// Free-form extra fields merged into the problem+json body
        /// (e.g. `{ "locked_until": "2026-05-27T15:00:00Z" }`).
        extra: serde_json::Map<String, serde_json::Value>,
    },

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<crate::crypto::CryptoError> for ApiError {
    fn from(err: crate::crypto::CryptoError) -> Self {
        ApiError::Internal(anyhow::anyhow!("crypto failure: {err}"))
    }
}

impl ApiError {
    /// 构造一个带稳定 `type` URI 的 `Typed` 错误（`extra` 为空）。各 route 文件里
    /// 「extra 为空」的 `ApiError::Typed { … }` 字面量统一走这里，少抄一段样板。
    /// 需要附带业务字段（`locked_until` / `expected_email` 等）时仍直接构造 `Typed`。
    pub fn typed(
        status: StatusCode,
        type_uri: &'static str,
        title: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        ApiError::Typed {
            status,
            type_uri,
            title,
            detail: detail.into(),
            extra: serde_json::Map::new(),
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            ApiError::Db(_) | ApiError::Internal(_) | ApiError::Config(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden { .. } => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Conflict { .. } => StatusCode::CONFLICT,
            ApiError::Gone { .. } => StatusCode::GONE,
            ApiError::Typed { status, .. } => *status,
        }
    }

    fn type_uri(&self) -> &'static str {
        match self {
            ApiError::Db(_) | ApiError::Internal(_) | ApiError::Config(_) => {
                "https://swarmhive.dev/errors/internal"
            }
            ApiError::Unauthorized => "https://swarmhive.dev/errors/unauthorized",
            ApiError::Forbidden { .. } => "https://swarmhive.dev/errors/forbidden",
            ApiError::NotFound => "https://swarmhive.dev/errors/not-found",
            ApiError::Validation { .. } => "https://swarmhive.dev/errors/validation",
            ApiError::Conflict { .. } => "https://swarmhive.dev/errors/conflict",
            ApiError::Gone { .. } => "https://swarmhive.dev/errors/gone",
            ApiError::Typed { type_uri, .. } => type_uri,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            ApiError::Db(_) | ApiError::Internal(_) | ApiError::Config(_) => {
                "Internal Server Error"
            }
            ApiError::Unauthorized => "Unauthorized",
            ApiError::Forbidden { .. } => "Forbidden",
            ApiError::NotFound => "Not Found",
            ApiError::Validation { .. } => "Unprocessable Entity",
            ApiError::Conflict { .. } => "Conflict",
            ApiError::Gone { .. } => "Gone",
            ApiError::Typed { title, .. } => title,
        }
    }
}

/// RFC 9457 problem+json body.
///
/// `pub` + `ToSchema` so OpenAPI can reference this as
/// `#/components/schemas/Problem` from every error response.
#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    /// Stable error type URI; clients SHOULD branch on this rather than
    /// matching on `title` strings.
    #[serde(rename = "type")]
    #[schema(rename = "type", example = "https://swarmhive.dev/errors/unauthorized")]
    pub type_uri: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub detail: String,
    /// Populated only on `403 Forbidden`; names the permission the caller
    /// is missing (e.g. `"release:publish"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_permission: Option<String>,
}

/// OpenAPI-only enumeration of every status the [`ApiError`] type can produce.
///
/// Referenced once per handler in `#[utoipa::path(... responses(..., ApiErrorResponses))]`.
/// Has no runtime instances — exists purely to drive `IntoResponses` doc
/// generation; the actual response bodies are produced by `ApiError`'s
/// `IntoResponse` impl below.
#[derive(IntoResponses)]
#[allow(dead_code)]
pub enum ApiErrorResponses {
    #[response(
        status = 401,
        description = "Unauthenticated request, or invalid credentials.",
        content_type = "application/problem+json"
    )]
    Unauthorized(Problem),

    #[response(
        status = 403,
        description = "Authenticated caller lacks the required permission. `required_permission` field names which.",
        content_type = "application/problem+json"
    )]
    Forbidden(Problem),

    #[response(
        status = 404,
        description = "Resource not found.",
        content_type = "application/problem+json"
    )]
    NotFound(Problem),

    #[response(
        status = 409,
        description = "Conflict with current resource state (e.g. setup already complete).",
        content_type = "application/problem+json"
    )]
    Conflict(Problem),

    #[response(
        status = 410,
        description = "Resource has been consumed or expired (e.g. setup token already used).",
        content_type = "application/problem+json"
    )]
    Gone(Problem),

    #[response(
        status = 422,
        description = "Request body failed validation (garde / serde).",
        content_type = "application/problem+json"
    )]
    UnprocessableEntity(Problem),

    #[response(
        status = 500,
        description = "Internal server error (database, config, or unexpected fault).",
        content_type = "application/problem+json"
    )]
    InternalServerError(Problem),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let detail = self.to_string();
        let required_permission = match &self {
            ApiError::Forbidden {
                required_permission,
            } => Some(required_permission.clone()),
            _ => None,
        };

        // Log server-side faults so they're traceable; client errors stay quiet.
        if status.is_server_error() {
            tracing::error!(?self, "api error");
        }

        let body = Problem {
            type_uri: self.type_uri(),
            title: self.title(),
            status: status.as_u16(),
            detail,
            required_permission,
        };
        let mut json_body = json!(body);
        // Merge any free-form fields from the `Typed` variant
        // (e.g. `locked_until`, `expected_email`) into the top-level object.
        if let ApiError::Typed { extra, .. } = &self
            && let Some(obj) = json_body.as_object_mut()
        {
            for (k, v) in extra {
                obj.insert(k.clone(), v.clone());
            }
        }

        let mut resp = (status, Json(json_body)).into_response();
        resp.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        resp
    }
}
