//! Pod-level permission bitflags and resolution.
//!
//! Separate namespace from community-level permissions in `permissions.rs`.

use diesel::prelude::*;
use diesel::result::OptionalExtension;

use crate::db::pool::DbPool;
use crate::db::schema::{pod_bans, pod_member_roles, pod_roles};
use crate::error::ApiError;

// Pod permission bitflags
pub const POD_CREATE_COMMUNITY: i64 = 1 << 0;
pub const POD_MANAGE_COMMUNITIES: i64 = 1 << 1;
pub const POD_BAN_MEMBERS: i64 = 1 << 2;
pub const POD_MANAGE_INVITES: i64 = 1 << 3;
pub const POD_VIEW_AUDIT_LOG: i64 = 1 << 4;
pub const POD_MANAGE_SETTINGS: i64 = 1 << 5;
pub const POD_MANAGE_ROLES: i64 = 1 << 6;
pub const POD_ADMINISTRATOR: i64 = 1 << 15;

/// All pod permissions combined.
const ALL_POD_PERMISSIONS: i64 = POD_CREATE_COMMUNITY
    | POD_MANAGE_COMMUNITIES
    | POD_BAN_MEMBERS
    | POD_MANAGE_INVITES
    | POD_VIEW_AUDIT_LOG
    | POD_MANAGE_SETTINGS
    | POD_MANAGE_ROLES
    | POD_ADMINISTRATOR;

/// Check if a user is the pod owner.
pub fn is_pod_owner(pod_owner_id: Option<&str>, user_id: &str) -> bool {
    pod_owner_id.is_some_and(|owner| owner == user_id)
}

/// Compute the effective pod permissions for a user.
///
/// Pod owner and POD_ADMINISTRATOR holders get all permissions.
pub async fn compute_pod_permissions(
    pool: &DbPool,
    pod_owner_id: Option<&str>,
    user_id: &str,
) -> Result<i64, ApiError> {
    // Pod owner gets everything.
    if is_pod_owner(pod_owner_id, user_id) {
        return Ok(ALL_POD_PERMISSIONS);
    }

    let mut conn = pool.get().await?;

    // Get @everyone pod role permissions.
    let everyone_perms: Option<i64> = diesel_async::RunQueryDsl::get_result(
        pod_roles::table
            .filter(pod_roles::is_default.eq(true))
            .select(pod_roles::permissions),
        &mut conn,
    )
    .await
    .optional()?;

    // Get permissions from explicitly assigned pod roles.
    let explicit_perms: Vec<i64> = diesel_async::RunQueryDsl::load(
        pod_roles::table
            .inner_join(pod_member_roles::table.on(pod_member_roles::role_id.eq(pod_roles::id)))
            .filter(pod_member_roles::user_id.eq(user_id))
            .select(pod_roles::permissions),
        &mut conn,
    )
    .await?;

    let mut combined = everyone_perms.unwrap_or(0);
    for p in &explicit_perms {
        combined |= p;
    }

    // POD_ADMINISTRATOR grants all permissions.
    if combined & POD_ADMINISTRATOR != 0 {
        return Ok(ALL_POD_PERMISSIONS);
    }

    Ok(combined)
}

/// Check if a user has a specific pod-level permission.
pub async fn check_pod_permission(
    pool: &DbPool,
    pod_owner_id: Option<&str>,
    user_id: &str,
    required: i64,
) -> Result<(), ApiError> {
    let perms = compute_pod_permissions(pool, pod_owner_id, user_id).await?;

    if perms & required != 0 {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "You do not have permission to perform this action",
        ))
    }
}

/// Check if a user is banned from the pod.
pub async fn is_pod_banned(pool: &DbPool, user_id: &str) -> Result<bool, ApiError> {
    let mut conn = pool.get().await?;

    let count: i64 = diesel_async::RunQueryDsl::get_result(
        pod_bans::table
            .filter(pod_bans::user_id.eq(user_id))
            .count(),
        &mut conn,
    )
    .await?;

    Ok(count > 0)
}

/// Get the highest pod role position for a user.
/// Pod owner returns `i32::MAX` to bypass hierarchy checks.
pub async fn get_highest_pod_role_position(
    pool: &DbPool,
    pod_owner_id: Option<&str>,
    user_id: &str,
) -> Result<i32, ApiError> {
    if is_pod_owner(pod_owner_id, user_id) {
        return Ok(i32::MAX);
    }

    let mut conn = pool.get().await?;

    let max_pos: Option<i32> = diesel_async::RunQueryDsl::get_result(
        pod_roles::table
            .inner_join(pod_member_roles::table.on(pod_member_roles::role_id.eq(pod_roles::id)))
            .filter(pod_member_roles::user_id.eq(user_id))
            .select(diesel::dsl::max(pod_roles::position)),
        &mut conn,
    )
    .await?;

    Ok(max_pos.unwrap_or(0))
}
