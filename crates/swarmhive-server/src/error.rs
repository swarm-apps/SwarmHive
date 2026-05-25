//! Server error type with RFC 9457 `application/problem+json` response.
//!
//! Domain errors implement `From<…> for ApiError`; handlers return
//! `Result<T, ApiError>`. The `IntoResponse` impl emits problem+json with
//! a stable `type` URI per variant so clients (Admin SPA, CLI) can branch.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sea_orm::DbErr;
use serde::Serialize;
use serde_json::json;

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

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::Db(_) | ApiError::Internal(_) | ApiError::Config(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden { .. } => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
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
        }
    }
}

/// RFC 9457 problem+json body.
#[derive(Debug, Serialize)]
struct Problem {
    #[serde(rename = "type")]
    type_uri: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_permission: Option<String>,
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

        let mut resp = (status, Json(json!(body))).into_response();
        resp.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/problem+json"),
        );
        resp
    }
}
