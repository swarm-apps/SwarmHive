//! `axum::FromRequestParts` impl that resolves a [`Principal`] from either
//! an `Authorization: Bearer …` header (PAT / API Token) or a cookie session.
//!
//! Precedence: Bearer header wins when present (D6 in design.md). A malformed
//! Authorization header rejects with `Unauthorized` rather than falling
//! through to the cookie — explicit headers must not be silently ignored.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use tower_sessions::Session;

use super::service::{self, RequestCtx};
use super::{Principal, bearer};
use crate::error::ApiError;
use crate::state::AppState;

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(auth) = parts.headers.get(header::AUTHORIZATION) {
            let value = auth.to_str().map_err(|_| ApiError::Unauthorized)?;
            let ctx = RequestCtx::from_headers(&parts.headers);
            return bearer::resolve(&state.db, value, &ctx).await;
        }

        let session = Session::from_request_parts(parts, &())
            .await
            .map_err(|(status, msg)| {
                if status == StatusCode::UNAUTHORIZED {
                    ApiError::Unauthorized
                } else {
                    ApiError::Internal(anyhow::anyhow!(
                        "session middleware error ({status}): {msg}"
                    ))
                }
            })?;

        service::load_principal(&state.db, &session).await
    }
}
