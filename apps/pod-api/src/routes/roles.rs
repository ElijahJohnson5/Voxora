//! Role CRUD endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{communities, community_members, roles};
use crate::error::{ApiError, ApiErrorBody};
use crate::models::role::{NewRole, Role, UpdateRole};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/communities/{community_id}/roles",
            get(list_roles).post(create_role),
        )
        .route(
            "/communities/{community_id}/roles/{role_id}",
            axum::routing::patch(update_role).delete(delete_role),
        )
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/roles
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/communities/{community_id}/roles",
    tag = "Roles",
    params(
        ("community_id" = String, Path, description = "Community ID"),
    ),
    responses(
        (status = 200, description = "List of roles", body = [Role]),
        (status = 404, description = "Community not found", body = ApiErrorBody),
    ),
)]
pub async fn list_roles(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
) -> Result<Json<Vec<Role>>, ApiError> {
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

    let list: Vec<Role> = diesel_async::RunQueryDsl::load(
        roles::table
            .filter(roles::community_id.eq(&community_id))
            .order(roles::position.asc())
            .select(Role::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/roles
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRoleRequest {
    pub name: String,
    pub color: Option<i32>,
    pub permissions: Option<i64>,
    pub mentionable: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/v1/communities/{community_id}/roles",
    tag = "Roles",
    security(("bearer" = [])),
    params(
        ("community_id" = String, Path, description = "Community ID"),
    ),
    request_body = CreateRoleRequest,
    responses(
        (status = 201, description = "Role created", body = Role),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
    ),
)]
pub async fn create_role(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(community_id): Path<String>,
    Json(body): Json<CreateRoleRequest>,
) -> Result<(StatusCode, Json<Role>), ApiError> {
    permissions::check_permission(
        &state.db,
        &community_id,
        &user_id,
        permissions::MANAGE_ROLES,
    )
    .await?;

    // Validate name.
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("Role name is required"));
    }
    if name.len() > 100 {
        return Err(ApiError::bad_request(
            "Role name must be 100 characters or fewer",
        ));
    }

    let perms = body.permissions.unwrap_or(0);

    // Cannot set ADMINISTRATOR unless caller is owner.
    if perms & permissions::ADMINISTRATOR != 0 {
        if !permissions::is_owner(&state.db, &community_id, &user_id).await? {
            return Err(ApiError::forbidden(
                "Only the community owner can grant ADMINISTRATOR",
            ));
        }
    }

    let mut conn = state.db.get().await?;

    // Auto-assign position = max existing + 1.
    let max_pos: Option<i32> = diesel_async::RunQueryDsl::get_result(
        roles::table
            .filter(roles::community_id.eq(&community_id))
            .select(diesel::dsl::max(roles::position)),
        &mut conn,
    )
    .await?;

    let position = max_pos.unwrap_or(0) + 1;

    // Role hierarchy: new role position must be < caller's highest.
    let caller_highest =
        permissions::get_highest_role_position(&state.db, &community_id, &user_id).await?;
    if position >= caller_highest && caller_highest != i32::MAX {
        return Err(ApiError::forbidden(
            "Cannot create a role at or above your highest role position",
        ));
    }

    let role_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::ROLE);
    let now = Utc::now();

    let role: Role = diesel_async::RunQueryDsl::get_result(
        diesel::insert_into(roles::table)
            .values(NewRole {
                id: &role_id,
                community_id: &community_id,
                name: &name,
                color: body.color,
                position,
                permissions: perms,
                mentionable: body.mentionable.unwrap_or(false),
                is_default: false,
                created_at: now,
            })
            .returning(Role::as_returning()),
        &mut conn,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(role)))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:community_id/roles/:role_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RolePath {
    pub community_id: String,
    pub role_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateRoleRequest {
    pub name: Option<String>,
    pub color: Option<Option<i32>>,
    pub permissions: Option<i64>,
    pub mentionable: Option<bool>,
    pub position: Option<i32>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/communities/{community_id}/roles/{role_id}",
    tag = "Roles",
    security(("bearer" = [])),
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("role_id" = String, Path, description = "Role ID"),
    ),
    request_body = UpdateRoleRequest,
    responses(
        (status = 200, description = "Role updated", body = Role),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Role not found", body = ApiErrorBody),
    ),
)]
pub async fn update_role(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<RolePath>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<Role>, ApiError> {
    permissions::check_permission(
        &state.db,
        &path.community_id,
        &user_id,
        permissions::MANAGE_ROLES,
    )
    .await?;

    let mut conn = state.db.get().await?;

    // Look up target role.
    let target: Role = diesel_async::RunQueryDsl::get_result(
        roles::table.find(&path.role_id).select(Role::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Role not found"))?;

    // Prevent editing @everyone's name.
    if target.is_default {
        if body.name.is_some() {
            return Err(ApiError::bad_request(
                "Cannot change the name of the @everyone role",
            ));
        }
    }

    // Role hierarchy: target role position must be < caller's highest.
    let caller_highest =
        permissions::get_highest_role_position(&state.db, &path.community_id, &user_id).await?;

    if caller_highest != i32::MAX && target.position >= caller_highest {
        return Err(ApiError::forbidden(
            "Cannot edit a role at or above your highest role position",
        ));
    }

    // If changing position, new position must be < caller's highest.
    if let Some(new_pos) = body.position {
        if caller_highest != i32::MAX && new_pos >= caller_highest {
            return Err(ApiError::forbidden(
                "Cannot move a role to or above your highest role position",
            ));
        }
    }

    // Cannot set ADMINISTRATOR unless caller is owner.
    if let Some(perms) = body.permissions {
        if perms & permissions::ADMINISTRATOR != 0 {
            if !permissions::is_owner(&state.db, &path.community_id, &user_id).await? {
                return Err(ApiError::forbidden(
                    "Only the community owner can grant ADMINISTRATOR",
                ));
            }
        }
    }

    // Validate name if provided.
    if let Some(ref name) = body.name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::bad_request("Role name cannot be empty"));
        }
        if trimmed.len() > 100 {
            return Err(ApiError::bad_request(
                "Role name must be 100 characters or fewer",
            ));
        }
    }

    let changeset = UpdateRole {
        name: body.name.map(|n| n.trim().to_string()),
        color: body.color,
        position: body.position,
        permissions: body.permissions,
        mentionable: body.mentionable,
    };

    let updated: Role = diesel_async::RunQueryDsl::get_result(
        diesel::update(roles::table.find(&path.role_id))
            .set(&changeset)
            .returning(Role::as_returning()),
        &mut conn,
    )
    .await?;

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/roles/:role_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{community_id}/roles/{role_id}",
    tag = "Roles",
    security(("bearer" = [])),
    params(
        ("community_id" = String, Path, description = "Community ID"),
        ("role_id" = String, Path, description = "Role ID"),
    ),
    responses(
        (status = 204, description = "Role deleted"),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Role not found", body = ApiErrorBody),
    ),
)]
pub async fn delete_role(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<RolePath>,
) -> Result<StatusCode, ApiError> {
    permissions::check_permission(
        &state.db,
        &path.community_id,
        &user_id,
        permissions::MANAGE_ROLES,
    )
    .await?;

    let mut conn = state.db.get().await?;

    // Look up target role.
    let target: Role = diesel_async::RunQueryDsl::get_result(
        roles::table.find(&path.role_id).select(Role::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Role not found"))?;

    // Cannot delete @everyone.
    if target.is_default {
        return Err(ApiError::bad_request("Cannot delete the @everyone role"));
    }

    // Role hierarchy: target role position must be < caller's highest.
    let caller_highest =
        permissions::get_highest_role_position(&state.db, &path.community_id, &user_id).await?;

    if caller_highest != i32::MAX && target.position >= caller_highest {
        return Err(ApiError::forbidden(
            "Cannot delete a role at or above your highest role position",
        ));
    }

    // Remove role ID from all community_members.roles arrays that contain it.
    let members_with_role: Vec<(String, String, Vec<String>)> = diesel_async::RunQueryDsl::load(
        community_members::table
            .filter(community_members::community_id.eq(&path.community_id))
            .select((
                community_members::community_id,
                community_members::user_id,
                community_members::roles,
            )),
        &mut conn,
    )
    .await?;

    for (cid, uid, member_roles) in &members_with_role {
        if member_roles.contains(&path.role_id) {
            let updated_roles: Vec<String> = member_roles
                .iter()
                .filter(|r| *r != &path.role_id)
                .cloned()
                .collect();
            diesel_async::RunQueryDsl::execute(
                diesel::update(community_members::table.find((cid, uid)))
                    .set(community_members::roles.eq(updated_roles)),
                &mut conn,
            )
            .await?;
        }
    }

    // Delete the role.
    diesel_async::RunQueryDsl::execute(diesel::delete(roles::table.find(&path.role_id)), &mut conn)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
