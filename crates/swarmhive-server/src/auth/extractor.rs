//! `axum::FromRequestParts` impl that turns a cookie session into a
//! [`Principal`]. Requires `SessionManagerLayer` (tower-sessions) to be
//! wired ahead of the handler — without it `Session::from_request_parts`
//! returns an error and we surface `Internal`.
//!
//! Bearer-token path (PAT / API Token) is reserved for
//! `add-pat-and-api-token`; for now an `Authorization: Bearer …` header
//! short-circuits to `Unauthorized` so it doesn't fall through to the
//! cookie path with a misleading rejection.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use tower_sessions::Session;

use super::{Principal, service};
use crate::error::ApiError;
use crate::state::AppState;

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if parts.headers.get(header::AUTHORIZATION).is_some() {
            // Bearer flow lands here once add-pat-and-api-token ships;
            // until then any Authorization header is an auth failure.
            return Err(ApiError::Unauthorized);
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
