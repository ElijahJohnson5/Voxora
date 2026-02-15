use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::db::schema::pods;
use crate::error::ApiError;
use crate::models::pod::Pod;
use crate::AppState;

/// Authenticated pod extracted from the `Authorization: Bearer <client_secret>` header.
///
/// Looks up the pod by `client_secret` in the database and returns the full pod row.
///
/// ```ignore
/// async fn handler(pod_client: PodClient) -> impl IntoResponse { ... }
/// ```
#[derive(Debug)]
pub struct PodClient {
    pub pod: Pod,
}

impl FromRequestParts<AppState> for PodClient {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("Missing Authorization header"))?;

        let secret = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::unauthorized("Invalid Authorization header format"))?;

        let mut conn = state.db.get().await?;

        let pod: Pod = pods::table
            .filter(pods::client_secret.eq(secret))
            .select(Pod::as_select())
            .first(&mut conn)
            .await
            .optional()
            .map_err(ApiError::from)?
            .ok_or_else(|| ApiError::unauthorized("Invalid client credentials"))?;

        Ok(PodClient { pod })
    }
}
