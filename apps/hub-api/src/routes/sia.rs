use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::auth::sia;
use crate::db::schema::{pods, user_pod_bookmarks, users};
use crate::error::ApiError;
use crate::models::bookmark::NewUserPodBookmark;
use crate::models::pod::Pod;
use crate::models::user::User;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/oidc/sia", post(issue_sia))
}

#[derive(Debug, Deserialize)]
pub struct SiaRequest {
    pub pod_id: String,
}

#[derive(Debug, Serialize)]
pub struct SiaResponse {
    pub sia: String,
    pub expires_at: String,
}

/// `POST /api/v1/oidc/sia` — Issue a Signed Identity Assertion for a target Pod.
async fn issue_sia(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<SiaRequest>,
) -> Result<Json<SiaResponse>, ApiError> {
    // Require `pods` scope.
    if !auth.scopes.iter().any(|s| s == "pods") {
        return Err(ApiError::forbidden(
            "Access token must have 'pods' scope to request a SIA",
        ));
    }

    // Validate pod_id format.
    if !body.pod_id.starts_with("pod_") {
        return Err(ApiError::bad_request("Invalid pod_id format"));
    }

    let mut conn = state.db.get().await?;

    // Look up the Pod — must exist and be active.
    let pod: Pod = pods::table
        .find(&body.pod_id)
        .select(Pod::as_select())
        .first(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("Pod not found"))?;

    if pod.status != "active" {
        return Err(ApiError::bad_request("Pod is not active"));
    }

    // Upsert a bookmark so the Hub remembers this user ↔ pod association.
    diesel::insert_into(user_pod_bookmarks::table)
        .values(NewUserPodBookmark {
            user_id: auth.user_id.clone(),
            pod_id: body.pod_id.clone(),
        })
        .on_conflict_do_nothing()
        .execute(&mut conn)
        .await
        .map_err(ApiError::from)?;

    // Load the authenticated user's profile.
    let user: User = users::table
        .find(&auth.user_id)
        .select(User::as_select())
        .first(&mut conn)
        .await
        .map_err(ApiError::from)?;

    // Mint the SIA JWT.
    let (token, expires_at) = sia::mint_sia(
        &state.keys,
        &state.config.hub_domain,
        &user.id,
        &pod.id,
        &user.username,
        &user.display_name,
        user.avatar_url.as_deref(),
        user.email.as_deref(),
        user.email_verified,
        user.flags,
    )?;

    Ok(Json(SiaResponse {
        sia: token,
        expires_at: expires_at.to_rfc3339(),
    }))
}
