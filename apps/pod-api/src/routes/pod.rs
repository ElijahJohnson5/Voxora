//! Pod management endpoints: pod roles, pod member roles, pod bans.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use diesel_async::AsyncConnection;
use scoped_futures::ScopedFutureExt;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{communities, community_members, pod_bans, pod_member_roles, pod_roles, pod_users};
use crate::error::{ApiError, ApiErrorBody};
use crate::models::audit_log;
use crate::models::pod_ban::{NewPodBan, PodBan};
use crate::models::pod_role::{NewPodRole, PodRole, UpdatePodRole};
use crate::pod_permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pod/roles", get(list_pod_roles).post(create_pod_role))
        .route(
            "/pod/roles/{role_id}",
            axum::routing::patch(update_pod_role).delete(delete_pod_role),
        )
        .route(
            "/pod/members/{user_id}/roles/{role_id}",
            axum::routing::put(assign_pod_role).delete(unassign_pod_role),
        )
        .route("/pod/bans", get(list_pod_bans))
        .route(
            "/pod/bans/{user_id}",
            axum::routing::put(pod_ban_user).delete(pod_unban_user),
        )
}

// ---------------------------------------------------------------------------
// GET /api/v1/pod/roles
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/pod/roles",
    tag = "Pod Roles",
    responses(
        (status = 200, description = "List of pod roles", body = [PodRole]),
    ),
)]
pub async fn list_pod_roles(
    State(state): State<AppState>,
) -> Result<Json<Vec<PodRole>>, ApiError> {
    let mut conn = state.db.get().await?;

    let list: Vec<PodRole> = diesel_async::RunQueryDsl::load(
        pod_roles::table
            .order(pod_roles::position.asc())
            .select(PodRole::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// POST /api/v1/pod/roles
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePodRoleRequest {
    pub name: String,
    pub color: Option<i32>,
    pub permissions: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/pod/roles",
    tag = "Pod Roles",
    security(("bearer" = [])),
    request_body = CreatePodRoleRequest,
    responses(
        (status = 201, description = "Pod role created", body = PodRole),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
    ),
)]
pub async fn create_pod_role(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreatePodRoleRequest>,
) -> Result<(StatusCode, Json<PodRole>), ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &user_id,
        pod_permissions::POD_MANAGE_ROLES,
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

    // Cannot set POD_ADMINISTRATOR unless caller is pod owner.
    if perms & pod_permissions::POD_ADMINISTRATOR != 0
        && !pod_permissions::is_pod_owner(pod_owner_id, &user_id)
    {
        return Err(ApiError::forbidden(
            "Only the pod owner can grant POD_ADMINISTRATOR",
        ));
    }

    let mut conn = state.db.get().await?;

    // Auto-assign position = max existing + 1.
    let max_pos: Option<i32> = diesel_async::RunQueryDsl::get_result(
        pod_roles::table.select(diesel::dsl::max(pod_roles::position)),
        &mut conn,
    )
    .await?;

    let position = max_pos.unwrap_or(0) + 1;

    // Hierarchy check.
    let caller_highest =
        pod_permissions::get_highest_pod_role_position(&state.db, pod_owner_id, &user_id).await?;
    if position >= caller_highest && caller_highest != i32::MAX {
        return Err(ApiError::forbidden(
            "Cannot create a role at or above your highest role position",
        ));
    }

    let role_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::ROLE);
    let now = Utc::now();

    let role: PodRole = diesel_async::RunQueryDsl::get_result(
        diesel::insert_into(pod_roles::table)
            .values(NewPodRole {
                id: &role_id,
                name: &name,
                color: body.color,
                position,
                permissions: perms,
                is_default: false,
                created_at: now,
            })
            .returning(PodRole::as_returning()),
        &mut conn,
    )
    .await?;

    audit_log::log(
        &state.db,
        "pod",
        &user_id,
        "pod_role.create",
        Some("pod_role"),
        Some(&role.id),
        None,
        None,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(role)))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/pod/roles/:role_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePodRoleRequest {
    pub name: Option<String>,
    pub color: Option<Option<i32>>,
    pub permissions: Option<i64>,
    pub position: Option<i32>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/pod/roles/{role_id}",
    tag = "Pod Roles",
    security(("bearer" = [])),
    params(
        ("role_id" = String, Path, description = "Pod role ID"),
    ),
    request_body = UpdatePodRoleRequest,
    responses(
        (status = 200, description = "Pod role updated", body = PodRole),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Role not found", body = ApiErrorBody),
    ),
)]
pub async fn update_pod_role(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(role_id): Path<String>,
    Json(body): Json<UpdatePodRoleRequest>,
) -> Result<Json<PodRole>, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &user_id,
        pod_permissions::POD_MANAGE_ROLES,
    )
    .await?;

    let mut conn = state.db.get().await?;

    // Look up target role.
    let target: PodRole = diesel_async::RunQueryDsl::get_result(
        pod_roles::table
            .find(&role_id)
            .select(PodRole::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Pod role not found"))?;

    // Prevent editing @everyone's name.
    if target.is_default && body.name.is_some() {
        return Err(ApiError::bad_request(
            "Cannot change the name of the @everyone role",
        ));
    }

    // Hierarchy check.
    let caller_highest =
        pod_permissions::get_highest_pod_role_position(&state.db, pod_owner_id, &user_id).await?;

    if caller_highest != i32::MAX && target.position >= caller_highest {
        return Err(ApiError::forbidden(
            "Cannot edit a role at or above your highest role position",
        ));
    }

    if let Some(new_pos) = body.position {
        if caller_highest != i32::MAX && new_pos >= caller_highest {
            return Err(ApiError::forbidden(
                "Cannot move a role to or above your highest role position",
            ));
        }
    }

    // Cannot set POD_ADMINISTRATOR unless caller is pod owner.
    if let Some(perms) = body.permissions {
        if perms & pod_permissions::POD_ADMINISTRATOR != 0
            && !pod_permissions::is_pod_owner(pod_owner_id, &user_id)
        {
            return Err(ApiError::forbidden(
                "Only the pod owner can grant POD_ADMINISTRATOR",
            ));
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

    let changeset = UpdatePodRole {
        name: body.name.map(|n| n.trim().to_string()),
        color: body.color,
        position: body.position,
        permissions: body.permissions,
    };

    let updated: PodRole = diesel_async::RunQueryDsl::get_result(
        diesel::update(pod_roles::table.find(&role_id))
            .set(&changeset)
            .returning(PodRole::as_returning()),
        &mut conn,
    )
    .await?;

    // Build changes JSON.
    let mut changes = serde_json::Map::new();
    if target.name != updated.name {
        changes.insert(
            "name".to_string(),
            serde_json::json!({ "old": target.name, "new": updated.name }),
        );
    }
    if target.permissions != updated.permissions {
        changes.insert(
            "permissions".to_string(),
            serde_json::json!({ "old": target.permissions, "new": updated.permissions }),
        );
    }
    if target.color != updated.color {
        changes.insert(
            "color".to_string(),
            serde_json::json!({ "old": target.color, "new": updated.color }),
        );
    }
    if target.position != updated.position {
        changes.insert(
            "position".to_string(),
            serde_json::json!({ "old": target.position, "new": updated.position }),
        );
    }
    let changes_val = if changes.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(changes))
    };

    audit_log::log(
        &state.db,
        "pod",
        &user_id,
        "pod_role.update",
        Some("pod_role"),
        Some(&role_id),
        changes_val,
        None,
    )
    .await?;

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/pod/roles/:role_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/pod/roles/{role_id}",
    tag = "Pod Roles",
    security(("bearer" = [])),
    params(
        ("role_id" = String, Path, description = "Pod role ID"),
    ),
    responses(
        (status = 204, description = "Pod role deleted"),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Role not found", body = ApiErrorBody),
    ),
)]
pub async fn delete_pod_role(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &user_id,
        pod_permissions::POD_MANAGE_ROLES,
    )
    .await?;

    let mut conn = state.db.get().await?;

    // Look up target role.
    let target: PodRole = diesel_async::RunQueryDsl::get_result(
        pod_roles::table
            .find(&role_id)
            .select(PodRole::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Pod role not found"))?;

    // Cannot delete @everyone.
    if target.is_default {
        return Err(ApiError::bad_request("Cannot delete the @everyone role"));
    }

    // Hierarchy check.
    let caller_highest =
        pod_permissions::get_highest_pod_role_position(&state.db, pod_owner_id, &user_id).await?;

    if caller_highest != i32::MAX && target.position >= caller_highest {
        return Err(ApiError::forbidden(
            "Cannot delete a role at or above your highest role position",
        ));
    }

    // Delete pod_member_roles entries (cascade handles this via FK, but be explicit).
    diesel_async::RunQueryDsl::execute(
        diesel::delete(pod_member_roles::table.filter(pod_member_roles::role_id.eq(&role_id))),
        &mut conn,
    )
    .await?;

    // Delete the role.
    diesel_async::RunQueryDsl::execute(
        diesel::delete(pod_roles::table.find(&role_id)),
        &mut conn,
    )
    .await?;

    audit_log::log(
        &state.db,
        "pod",
        &user_id,
        "pod_role.delete",
        Some("pod_role"),
        Some(&role_id),
        None,
        None,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// PUT /api/v1/pod/members/:user_id/roles/:role_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PodMemberRolePath {
    pub user_id: String,
    pub role_id: String,
}

#[utoipa::path(
    put,
    path = "/api/v1/pod/members/{user_id}/roles/{role_id}",
    tag = "Pod Roles",
    security(("bearer" = [])),
    params(
        ("user_id" = String, Path, description = "User ID"),
        ("role_id" = String, Path, description = "Pod role ID"),
    ),
    responses(
        (status = 204, description = "Role assigned"),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "User or role not found", body = ApiErrorBody),
    ),
)]
pub async fn assign_pod_role(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<PodMemberRolePath>,
) -> Result<StatusCode, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &auth_user_id,
        pod_permissions::POD_MANAGE_ROLES,
    )
    .await?;

    let mut conn = state.db.get().await?;

    // Verify user exists.
    diesel_async::RunQueryDsl::get_result::<String>(
        pod_users::table.find(&path.user_id).select(pod_users::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("User not found"))?;

    // Verify role exists.
    diesel_async::RunQueryDsl::get_result::<String>(
        pod_roles::table.find(&path.role_id).select(pod_roles::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Pod role not found"))?;

    // Idempotent insert.
    diesel_async::RunQueryDsl::execute(
        diesel::insert_into(pod_member_roles::table)
            .values((
                pod_member_roles::user_id.eq(&path.user_id),
                pod_member_roles::role_id.eq(&path.role_id),
            ))
            .on_conflict_do_nothing(),
        &mut conn,
    )
    .await?;

    audit_log::log(
        &state.db,
        "pod",
        &auth_user_id,
        "pod_member_role.assign",
        Some("user"),
        Some(&path.user_id),
        Some(serde_json::json!({ "role_id": path.role_id })),
        None,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/pod/members/:user_id/roles/:role_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/pod/members/{user_id}/roles/{role_id}",
    tag = "Pod Roles",
    security(("bearer" = [])),
    params(
        ("user_id" = String, Path, description = "User ID"),
        ("role_id" = String, Path, description = "Pod role ID"),
    ),
    responses(
        (status = 204, description = "Role unassigned"),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Assignment not found", body = ApiErrorBody),
    ),
)]
pub async fn unassign_pod_role(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<PodMemberRolePath>,
) -> Result<StatusCode, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &auth_user_id,
        pod_permissions::POD_MANAGE_ROLES,
    )
    .await?;

    let mut conn = state.db.get().await?;

    let deleted = diesel_async::RunQueryDsl::execute(
        diesel::delete(
            pod_member_roles::table
                .filter(pod_member_roles::user_id.eq(&path.user_id))
                .filter(pod_member_roles::role_id.eq(&path.role_id)),
        ),
        &mut conn,
    )
    .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Role assignment not found"));
    }

    audit_log::log(
        &state.db,
        "pod",
        &auth_user_id,
        "pod_member_role.unassign",
        Some("user"),
        Some(&path.user_id),
        Some(serde_json::json!({ "role_id": path.role_id })),
        None,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/v1/pod/bans
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/pod/bans",
    tag = "Pod Bans",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "List of pod bans", body = [PodBan]),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
    ),
)]
pub async fn list_pod_bans(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<PodBan>>, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &user_id,
        pod_permissions::POD_BAN_MEMBERS,
    )
    .await?;

    let mut conn = state.db.get().await?;

    let list: Vec<PodBan> = diesel_async::RunQueryDsl::load(
        pod_bans::table
            .order(pod_bans::created_at.desc())
            .select(PodBan::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// PUT /api/v1/pod/bans/:user_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct PodBanRequest {
    pub reason: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/v1/pod/bans/{user_id}",
    tag = "Pod Bans",
    security(("bearer" = [])),
    params(
        ("user_id" = String, Path, description = "User ID to ban"),
    ),
    request_body = PodBanRequest,
    responses(
        (status = 200, description = "User banned from pod", body = PodBan),
        (status = 400, description = "Cannot ban self or pod owner", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 409, description = "Already banned", body = ApiErrorBody),
    ),
)]
pub async fn pod_ban_user(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(target_user_id): Path<String>,
    Json(body): Json<PodBanRequest>,
) -> Result<Json<PodBan>, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &auth_user_id,
        pod_permissions::POD_BAN_MEMBERS,
    )
    .await?;

    // Cannot ban self.
    if target_user_id == auth_user_id {
        return Err(ApiError::bad_request("You cannot ban yourself"));
    }

    // Cannot ban pod owner.
    if pod_permissions::is_pod_owner(pod_owner_id, &target_user_id) {
        return Err(ApiError::bad_request("Cannot ban the pod owner"));
    }

    let mut conn = state.db.get().await?;

    // Check not already banned.
    let existing: Option<PodBan> = diesel_async::RunQueryDsl::get_result(
        pod_bans::table
            .find(&target_user_id)
            .select(PodBan::as_select()),
        &mut conn,
    )
    .await
    .optional()?;

    if existing.is_some() {
        return Err(ApiError::conflict("User is already banned from the pod"));
    }

    let reason = body.reason.clone();
    let banned_by = auth_user_id.clone();
    let target = target_user_id.clone();

    let ban = conn
        .transaction::<_, ApiError, _>(|conn| {
            async move {
                // Insert pod ban.
                let ban: PodBan = diesel_async::RunQueryDsl::get_result(
                    diesel::insert_into(pod_bans::table)
                        .values(NewPodBan {
                            user_id: &target,
                            reason: reason.as_deref(),
                            banned_by: &banned_by,
                        })
                        .returning(PodBan::as_returning()),
                    conn,
                )
                .await?;

                // Remove pod_member_roles for this user.
                diesel_async::RunQueryDsl::execute(
                    diesel::delete(
                        pod_member_roles::table
                            .filter(pod_member_roles::user_id.eq(&target)),
                    ),
                    conn,
                )
                .await?;

                // Remove from all community_members and decrement member_counts.
                let memberships: Vec<String> = diesel_async::RunQueryDsl::load(
                    community_members::table
                        .filter(community_members::user_id.eq(&target))
                        .select(community_members::community_id),
                    conn,
                )
                .await?;

                if !memberships.is_empty() {
                    diesel_async::RunQueryDsl::execute(
                        diesel::delete(
                            community_members::table
                                .filter(community_members::user_id.eq(&target)),
                        ),
                        conn,
                    )
                    .await?;

                    // Decrement member_count for each affected community.
                    for cid in &memberships {
                        diesel_async::RunQueryDsl::execute(
                            diesel::update(communities::table.find(cid))
                                .set(communities::member_count.eq(communities::member_count - 1)),
                            conn,
                        )
                        .await?;
                    }
                }

                Ok(ban)
            }
            .scope_boxed()
        })
        .await?;

    audit_log::log(
        &state.db,
        "pod",
        &auth_user_id,
        "pod.ban",
        Some("user"),
        Some(&target_user_id),
        None,
        body.reason.as_deref(),
    )
    .await?;

    Ok(Json(ban))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/pod/bans/:user_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/pod/bans/{user_id}",
    tag = "Pod Bans",
    security(("bearer" = [])),
    params(
        ("user_id" = String, Path, description = "User ID to unban"),
    ),
    responses(
        (status = 204, description = "User unbanned from pod"),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Ban not found", body = ApiErrorBody),
    ),
)]
pub async fn pod_unban_user(
    AuthUser {
        user_id: auth_user_id,
    }: AuthUser,
    State(state): State<AppState>,
    Path(target_user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let pod_owner_id = state.config.pod_owner_id.as_deref();

    pod_permissions::check_pod_permission(
        &state.db,
        pod_owner_id,
        &auth_user_id,
        pod_permissions::POD_BAN_MEMBERS,
    )
    .await?;

    let mut conn = state.db.get().await?;

    let deleted = diesel_async::RunQueryDsl::execute(
        diesel::delete(pod_bans::table.find(&target_user_id)),
        &mut conn,
    )
    .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Ban not found"));
    }

    audit_log::log(
        &state.db,
        "pod",
        &auth_user_id,
        "pod.unban",
        Some("user"),
        Some(&target_user_id),
        None,
        None,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
