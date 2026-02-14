//! Member endpoints.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{communities, community_members, pod_users};
use crate::error::{ApiError, ApiErrorBody};
use crate::gateway::events::EventName;
use crate::gateway::fanout::BroadcastPayload;
use crate::models::community_member::{CommunityMember, CommunityMemberRow};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/communities/{community_id}/members", get(list_members))
        .route(
            "/communities/{community_id}/members/{user_id}",
            get(get_member).delete(remove_member).patch(update_member),
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

#[derive(Debug, Serialize, ToSchema)]
pub struct ListMembersResponse {
    pub data: Vec<CommunityMember>,
    pub has_more: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/members",
    tag = "Members",
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("limit" = Option<i64>, Query, description = "Max members to return"),
        ("after" = Option<String>, Query, description = "Cursor: user ID to start after"),
    ),
    responses(
        (status = 200, description = "List of members", body = ListMembersResponse),
        (status = 404, description = "Community not found", body = ApiErrorBody),
    )
)]
pub async fn list_members(
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
        .inner_join(pod_users::table.on(pod_users::id.eq(community_members::user_id)))
        .filter(community_members::community_id.eq(&community_id))
        .order((
            community_members::joined_at.asc(),
            community_members::user_id.asc(),
        ))
        .limit(limit + 1)
        .select((CommunityMemberRow::as_select(), (pod_users::display_name, pod_users::username, pod_users::avatar_url)))
        .into_boxed();

    if let Some(after) = &params.after {
        query = query.filter(community_members::user_id.gt(after));
    }

    let rows: Vec<(CommunityMemberRow, (String, String, Option<String>))> =
        diesel_async::RunQueryDsl::load(query, &mut conn).await?;

    let has_more = rows.len() as i64 > limit;
    let data: Vec<CommunityMember> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(row, (display_name, username, avatar_url))| CommunityMember {
            community_id: row.community_id,
            user_id: row.user_id,
            nickname: row.nickname,
            roles: row.roles,
            joined_at: row.joined_at,
            display_name,
            username,
            avatar_url,
        })
        .collect();

    Ok(Json(ListMembersResponse { data, has_more }))
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MemberPath {
    pub community_id: String,
    pub user_id: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/members/{user_id}",
    tag = "Members",
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("user_id" = String, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "Member details", body = CommunityMember),
        (status = 404, description = "Member not found", body = ApiErrorBody),
    )
)]
pub async fn get_member(
    State(state): State<AppState>,
    Path(path): Path<MemberPath>,
) -> Result<Json<CommunityMember>, ApiError> {
    let mut conn = state.db.get().await?;

    let (row, (display_name, username, avatar_url)): (CommunityMemberRow, (String, String, Option<String>)) =
        diesel_async::RunQueryDsl::get_result(
            community_members::table
                .inner_join(pod_users::table.on(pod_users::id.eq(community_members::user_id)))
                .filter(community_members::community_id.eq(&path.community_id))
                .filter(community_members::user_id.eq(&path.user_id))
                .select((CommunityMemberRow::as_select(), (pod_users::display_name, pod_users::username, pod_users::avatar_url))),
            &mut conn,
        )
        .await
        .optional()?
        .ok_or_else(|| ApiError::not_found("Member not found"))?;

    let member = CommunityMember {
        community_id: row.community_id,
        user_id: row.user_id,
        nickname: row.nickname,
        roles: row.roles,
        joined_at: row.joined_at,
        display_name,
        username,
        avatar_url,
    };

    Ok(Json(member))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}/members/{user_id}",
    tag = "Members",
    security(("bearer" = [])),
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("user_id" = String, Path, description = "User ID"),
    ),
    responses(
        (status = 204, description = "Member removed"),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Member not found", body = ApiErrorBody),
    )
)]
pub async fn remove_member(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MemberPath>,
) -> Result<StatusCode, ApiError> {
    // Check if target is owner.
    if permissions::is_owner(&state.db, &path.community_id, &path.user_id).await? {
        return Err(ApiError::bad_request("Owner cannot leave or be removed"));
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
        diesel::delete(community_members::table.find((&path.community_id, &path.user_id))),
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

    state.broadcast.dispatch(BroadcastPayload {
        community_id: path.community_id,
        event_name: EventName::MEMBER_LEAVE.to_string(),
        data: serde_json::json!({
            "user_id": path.user_id,
        }),
    });

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:community_id/members/:user_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateMemberRequest {
    pub nickname: Option<String>,
    pub roles: Option<Vec<String>>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/communities/{community_id}/members/{user_id}",
    tag = "Members",
    security(("bearer" = [])),
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("user_id" = String, Path, description = "User ID"),
    ),
    request_body = UpdateMemberRequest,
    responses(
        (status = 200, description = "Updated member", body = CommunityMember),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Member not found", body = ApiErrorBody),
    )
)]
pub async fn update_member(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MemberPath>,
    Json(body): Json<UpdateMemberRequest>,
) -> Result<Json<CommunityMember>, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up existing member.
    let existing: CommunityMemberRow = diesel_async::RunQueryDsl::get_result(
        community_members::table
            .find((&path.community_id, &path.user_id))
            .select(CommunityMemberRow::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Member not found"))?;

    let is_self = path.user_id == auth_user_id;

    if is_self {
        if body.roles.is_some() {
            return Err(ApiError::forbidden("You cannot change your own roles"));
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

    let updated_row: CommunityMemberRow = diesel_async::RunQueryDsl::get_result(
        diesel::update(community_members::table.find((&path.community_id, &path.user_id)))
            .set((
                community_members::nickname.eq(&updated_nickname),
                community_members::roles.eq(&updated_roles),
            ))
            .returning(CommunityMemberRow::as_returning()),
        &mut conn,
    )
    .await?;

    // Re-query to get user info from the join.
    let (display_name, username, avatar_url): (String, String, Option<String>) =
        diesel_async::RunQueryDsl::get_result(
            pod_users::table
                .filter(pod_users::id.eq(&path.user_id))
                .select((pod_users::display_name, pod_users::username, pod_users::avatar_url)),
            &mut conn,
        )
        .await?;

    let updated = CommunityMember {
        community_id: updated_row.community_id,
        user_id: updated_row.user_id,
        nickname: updated_row.nickname,
        roles: updated_row.roles,
        joined_at: updated_row.joined_at,
        display_name,
        username,
        avatar_url,
    };

    state.broadcast.dispatch(BroadcastPayload {
        community_id: path.community_id,
        event_name: EventName::MEMBER_UPDATE.to_string(),
        data: serde_json::to_value(&updated).unwrap(),
    });

    Ok(Json(updated))
}
