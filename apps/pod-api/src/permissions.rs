use diesel::prelude::*;
use diesel::result::OptionalExtension;

use crate::db::pool::DbPool;
use crate::db::schema::{communities, community_members, roles};
use crate::error::ApiError;
use crate::models::community_member::CommunityMember;

// Permission bitflags (RFC §7.2.2)
pub const VIEW_CHANNEL: i64 = 1 << 0;
pub const SEND_MESSAGES: i64 = 1 << 1;
pub const MANAGE_MESSAGES: i64 = 1 << 3;
pub const MANAGE_CHANNELS: i64 = 1 << 4;
pub const MANAGE_COMMUNITY: i64 = 1 << 5;
pub const MANAGE_ROLES: i64 = 1 << 6;
pub const KICK_MEMBERS: i64 = 1 << 7;
pub const BAN_MEMBERS: i64 = 1 << 8;
pub const INVITE_MEMBERS: i64 = 1 << 9;
pub const USE_REACTIONS: i64 = 1 << 16;
pub const MENTION_EVERYONE: i64 = 1 << 19;
pub const ADMINISTRATOR: i64 = 1 << 31;

pub const DEFAULT_EVERYONE_PERMISSIONS: i64 =
    VIEW_CHANNEL | SEND_MESSAGES | USE_REACTIONS | INVITE_MEMBERS;

/// Check if a user is the owner of a community.
pub async fn is_owner(pool: &DbPool, community_id: &str, user_id: &str) -> Result<bool, ApiError> {
    let mut conn = pool.get().await?;

    let count: i64 = diesel_async::RunQueryDsl::get_result(
        communities::table
            .filter(communities::id.eq(community_id))
            .filter(communities::owner_id.eq(user_id))
            .count(),
        &mut conn,
    )
    .await?;

    Ok(count > 0)
}

/// Check if a user has a specific permission in a community.
/// Owners implicitly have ADMINISTRATOR.
pub async fn check_permission(
    pool: &DbPool,
    community_id: &str,
    user_id: &str,
    required: i64,
) -> Result<(), ApiError> {
    if is_owner(pool, community_id, user_id).await? {
        return Ok(());
    }

    let mut conn = pool.get().await?;

    // Get community member record.
    let member: CommunityMember = diesel_async::RunQueryDsl::get_result(
        community_members::table
            .find((community_id, user_id))
            .select(CommunityMember::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::forbidden("You are not a member of this community"))?;

    // Get permissions from all applicable roles (explicit + @everyone).
    let permissions: Vec<i64> = diesel_async::RunQueryDsl::load(
        roles::table
            .filter(roles::community_id.eq(community_id))
            .filter(
                roles::is_default
                    .eq(true)
                    .or(roles::id.eq_any(&member.roles)),
            )
            .select(roles::permissions),
        &mut conn,
    )
    .await?;

    let combined: i64 = permissions.iter().fold(0i64, |acc, p| acc | p);

    if combined & ADMINISTRATOR != 0 || combined & required != 0 {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "You do not have permission to perform this action",
        ))
    }
}

/// Get the highest role position for a user in a community.
/// Owners return `i32::MAX` to bypass hierarchy checks.
pub async fn get_highest_role_position(
    pool: &DbPool,
    community_id: &str,
    user_id: &str,
) -> Result<i32, ApiError> {
    if is_owner(pool, community_id, user_id).await? {
        return Ok(i32::MAX);
    }

    let mut conn = pool.get().await?;

    // Get member's explicit role IDs.
    let member_roles: Vec<String> = diesel_async::RunQueryDsl::get_result::<Vec<String>>(
        community_members::table
            .find((community_id, user_id))
            .select(community_members::roles),
        &mut conn,
    )
    .await
    .optional()?
    .unwrap_or_default();

    if member_roles.is_empty() {
        return Ok(0); // Only has @everyone (position 0)
    }

    let max_pos: Option<i32> = diesel_async::RunQueryDsl::get_result(
        roles::table
            .filter(roles::community_id.eq(community_id))
            .filter(roles::id.eq_any(&member_roles))
            .select(diesel::dsl::max(roles::position)),
        &mut conn,
    )
    .await?;

    Ok(max_pos.unwrap_or(0))
}
