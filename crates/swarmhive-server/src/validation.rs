//! `GardeJson<T>` — JSON body extractor that also runs `garde::Validate`.
//!
//! Replaces the per-handler boilerplate:
//!
//! ```ignore
//! Json(req): Json<T>
//! req.validate().map_err(|e| ApiError::Validation { detail: e.to_string() })?;
//! ```
//!
//! Both the JSON parse failure (malformed body, wrong Content-Type, etc.)
//! and the validation failure surface as `ApiError::Validation`, so they
//! round-trip through the RFC 9457 `application/problem+json` response
//! path uniformly. Handlers only see a fully-validated `T`.

use axum::Json;
use axum::extract::{FromRequest, Request};
use garde::Validate;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

#[derive(Debug, Clone, Copy, Default)]
pub struct GardeJson<T>(pub T);

impl<S, T> FromRequest<S> for GardeJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate<Context = ()> + Send + 'static,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) =
            Json::<T>::from_request(req, state)
                .await
                .map_err(|rej| ApiError::Validation {
                    detail: rej.body_text(),
                })?;
        value.validate().map_err(|e| ApiError::Validation {
            detail: e.to_string(),
        })?;
        Ok(GardeJson(value))
    }
}
