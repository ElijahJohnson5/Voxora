//! Ban endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::put;
use axum::{Json, Router};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use diesel_async::AsyncConnection;
use scoped_futures::ScopedFutureExt;
use serde::Deserialize;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{bans, communities, community_members};
use crate::error::ApiError;
use crate::models::ban::{Ban, NewBan};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/communities/{community_id}/bans/{user_id}",
        put(ban_member).delete(unban_member),
    )
}

// ---------------------------------------------------------------------------
// PUT /api/v1/communities/:community_id/bans/:user_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BanPath {
    pub community_id: String,
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct BanRequest {
    pub reason: Option<String>,
}

async fn ban_member(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<BanPath>,
    Json(body): Json<BanRequest>,
) -> Result<Json<Ban>, ApiError> {
    permissions::check_permission(
        &state.db,
        &path.community_id,
        &auth_user_id,
        permissions::BAN_MEMBERS,
    )
    .await?;

    // Cannot ban self.
    if path.user_id == auth_user_id {
        return Err(ApiError::bad_request("You cannot ban yourself"));
    }

    // Cannot ban community owner.
    if permissions::is_owner(&state.db, &path.community_id, &path.user_id).await? {
        return Err(ApiError::bad_request("Cannot ban the community owner"));
    }

    // Check not already banned.
    let mut conn = state.db.get().await?;
    let existing: Option<Ban> = diesel_async::RunQueryDsl::get_result(
        bans::table
            .find((&path.community_id, &path.user_id))
            .select(Ban::as_select()),
        &mut conn,
    )
    .await
    .optional()?;

    if existing.is_some() {
        return Err(ApiError::conflict("User is already banned"));
    }

    // Transaction: insert ban + remove from community_members + decrement member_count.
    let community_id = path.community_id.clone();
    let target_user_id = path.user_id.clone();
    let reason = body.reason.clone();
    let banned_by = auth_user_id.clone();

    let ban = conn
        .transaction::<_, ApiError, _>(|conn| {
            async move {
                let ban: Ban = diesel_async::RunQueryDsl::get_result(
                    diesel::insert_into(bans::table)
                        .values(NewBan {
                            community_id: &community_id,
                            user_id: &target_user_id,
                            reason: reason.as_deref(),
                            banned_by: &banned_by,
                        })
                        .returning(Ban::as_returning()),
                    conn,
                )
                .await?;

                // Remove from community_members if they are a member.
                let deleted = diesel_async::RunQueryDsl::execute(
                    diesel::delete(community_members::table.find((&community_id, &target_user_id))),
                    conn,
                )
                .await?;

                // Decrement member_count if they were a member.
                if deleted > 0 {
                    diesel_async::RunQueryDsl::execute(
                        diesel::update(communities::table.find(&community_id))
                            .set(communities::member_count.eq(communities::member_count - 1)),
                        conn,
                    )
                    .await?;
                }

                Ok(ban)
            }
            .scope_boxed()
        })
        .await?;

    Ok(Json(ban))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/bans/:user_id
// ---------------------------------------------------------------------------

async fn unban_member(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<BanPath>,
) -> Result<StatusCode, ApiError> {
    permissions::check_permission(
        &state.db,
        &path.community_id,
        &auth_user_id,
        permissions::BAN_MEMBERS,
    )
    .await?;

    let mut conn = state.db.get().await?;

    let deleted = diesel_async::RunQueryDsl::execute(
        diesel::delete(bans::table.find((&path.community_id, &path.user_id))),
        &mut conn,
    )
    .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Ban not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}
