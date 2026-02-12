//! Member endpoints.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::db::schema::{communities, community_members};
use crate::error::ApiError;
use crate::models::community_member::CommunityMember;
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/communities/:community_id/members",
            get(list_members),
        )
        .route(
            "/communities/:community_id/members/:user_id",
            axum::routing::delete(remove_member).patch(update_member),
        )
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/members
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListMembersParams {
    pub limit: Option<i64>,
    pub after: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListMembersResponse {
    pub data: Vec<CommunityMember>,
    pub has_more: bool,
}

async fn list_members(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
    Query(params): Query<ListMembersParams>,
) -> Result<Json<ListMembersResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    // Check community exists.
    diesel_async::RunQueryDsl::get_result::<String>(
        communities::table
            .find(&community_id)
            .select(communities::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Community not found"))?;

    let limit = params.limit.unwrap_or(100).clamp(1, 1000);

    let mut query = community_members::table
        .filter(community_members::community_id.eq(&community_id))
        .order((
            community_members::joined_at.asc(),
            community_members::user_id.asc(),
        ))
        .limit(limit + 1)
        .select(CommunityMember::as_select())
        .into_boxed();

    if let Some(after) = &params.after {
        query = query.filter(community_members::user_id.gt(after));
    }

    let rows: Vec<CommunityMember> = diesel_async::RunQueryDsl::load(query, &mut conn).await?;

    let has_more = rows.len() as i64 > limit;
    let data: Vec<CommunityMember> = rows.into_iter().take(limit as usize).collect();

    Ok(Json(ListMembersResponse { data, has_more }))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MemberPath {
    pub community_id: String,
    pub user_id: String,
}

async fn remove_member(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MemberPath>,
) -> Result<StatusCode, ApiError> {
    // Check if target is owner.
    if permissions::is_owner(&state.db, &path.community_id, &path.user_id).await? {
        return Err(ApiError::bad_request(
            "Owner cannot leave or be removed",
        ));
    }

    // If not self-remove, check KICK_MEMBERS permission.
    if path.user_id != auth_user_id {
        permissions::check_permission(
            &state.db,
            &path.community_id,
            &auth_user_id,
            permissions::KICK_MEMBERS,
        )
        .await?;
    }

    let mut conn = state.db.get().await?;

    let deleted = diesel_async::RunQueryDsl::execute(
        diesel::delete(
            community_members::table.find((&path.community_id, &path.user_id)),
        ),
        &mut conn,
    )
    .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Member not found"));
    }

    // Decrement member count.
    diesel_async::RunQueryDsl::execute(
        diesel::update(communities::table.find(&path.community_id))
            .set(communities::member_count.eq(communities::member_count - 1)),
        &mut conn,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpdateMemberRequest {
    pub nickname: Option<String>,
    pub roles: Option<Vec<String>>,
}

async fn update_member(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MemberPath>,
    Json(body): Json<UpdateMemberRequest>,
) -> Result<Json<CommunityMember>, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up existing member.
    let existing: CommunityMember = diesel_async::RunQueryDsl::get_result(
        community_members::table
            .find((&path.community_id, &path.user_id))
            .select(CommunityMember::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Member not found"))?;

    let is_self = path.user_id == auth_user_id;

    if is_self {
        if body.roles.is_some() {
            return Err(ApiError::forbidden(
                "You cannot change your own roles",
            ));
        }
    } else {
        permissions::check_permission(
            &state.db,
            &path.community_id,
            &auth_user_id,
            permissions::MANAGE_ROLES,
        )
        .await?;
    }

    // Process nickname: None → keep existing, Some(""/whitespace) → NULL, Some(valid) → set.
    let updated_nickname = match &body.nickname {
        Some(n) => {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                if trimmed.len() > 32 {
                    return Err(ApiError::bad_request(
                        "Nickname must be 32 characters or fewer",
                    ));
                }
                Some(trimmed.to_string())
            }
        }
        None => existing.nickname.clone(),
    };

    let updated_roles = match &body.roles {
        Some(r) => r.clone(),
        None => existing.roles.clone(),
    };

    let updated: CommunityMember = diesel_async::RunQueryDsl::get_result(
        diesel::update(
            community_members::table.find((&path.community_id, &path.user_id)),
        )
        .set((
            community_members::nickname.eq(&updated_nickname),
            community_members::roles.eq(&updated_roles),
        ))
        .returning(CommunityMember::as_returning()),
        &mut conn,
    )
    .await?;

    Ok(Json(updated))
}
