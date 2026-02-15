//! Channel permission override endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channel_overrides, channels};
use crate::error::{ApiError, ApiErrorBody};
use crate::models::audit_log;
use crate::models::channel::Channel;
use crate::models::channel_override::{ChannelOverride, NewChannelOverride};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/overrides",
            get(list_overrides),
        )
        .route(
            "/channels/{channel_id}/overrides/{target_type}/{target_id}",
            axum::routing::put(upsert_override).delete(delete_override),
        )
}

/// Map target_type string to i16.
fn parse_target_type(s: &str) -> Result<i16, ApiError> {
    match s {
        "role" => Ok(0),
        "user" => Ok(1),
        _ => Err(ApiError::bad_request(
            "target_type must be 'role' or 'user'",
        )),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/overrides
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/overrides",
    tag = "Channel Overrides",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
    ),
    responses(
        (status = 200, description = "List of channel overrides", body = [ChannelOverride]),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Channel not found", body = ApiErrorBody),
    ),
)]
pub async fn list_overrides(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
) -> Result<Json<Vec<ChannelOverride>>, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up channel to get community_id.
    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table
            .find(&channel_id)
            .select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Check MANAGE_CHANNELS permission.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::MANAGE_CHANNELS,
    )
    .await?;

    let list: Vec<ChannelOverride> = diesel_async::RunQueryDsl::load(
        channel_overrides::table
            .filter(channel_overrides::channel_id.eq(&channel_id))
            .select(ChannelOverride::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// PUT /api/v1/channels/:channel_id/overrides/:target_type/:target_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct OverridePath {
    pub channel_id: String,
    pub target_type: String,
    pub target_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertOverrideRequest {
    pub allow: i64,
    pub deny: i64,
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/overrides/{target_type}/{target_id}",
    tag = "Channel Overrides",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("target_type" = String, Path, description = "Target type: 'role' or 'user'"),
        ("target_id" = String, Path, description = "Target ID (role or user ID)"),
    ),
    request_body = UpsertOverrideRequest,
    responses(
        (status = 200, description = "Override upserted", body = ChannelOverride),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Channel not found", body = ApiErrorBody),
    ),
)]
pub async fn upsert_override(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<OverridePath>,
    Json(body): Json<UpsertOverrideRequest>,
) -> Result<Json<ChannelOverride>, ApiError> {
    let target_type_i16 = parse_target_type(&path.target_type)?;

    let mut conn = state.db.get().await?;

    // Look up channel to get community_id.
    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table
            .find(&path.channel_id)
            .select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Check MANAGE_ROLES permission.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::MANAGE_ROLES,
    )
    .await?;

    let values = NewChannelOverride {
        channel_id: &path.channel_id,
        target_type: target_type_i16,
        target_id: &path.target_id,
        allow: body.allow,
        deny: body.deny,
    };

    let result: ChannelOverride = diesel_async::RunQueryDsl::get_result(
        diesel::insert_into(channel_overrides::table)
            .values(&values)
            .on_conflict((
                channel_overrides::channel_id,
                channel_overrides::target_type,
                channel_overrides::target_id,
            ))
            .do_update()
            .set((
                channel_overrides::allow.eq(body.allow),
                channel_overrides::deny.eq(body.deny),
            ))
            .returning(ChannelOverride::as_returning()),
        &mut conn,
    )
    .await?;

    audit_log::log(
        &state.db,
        &channel.community_id,
        &user_id,
        "channel_override.update",
        Some("channel"),
        Some(&path.channel_id),
        Some(serde_json::json!({
            "target_type": path.target_type,
            "target_id": path.target_id,
            "allow": body.allow,
            "deny": body.deny,
        })),
        None,
    )
    .await?;

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/overrides/:target_type/:target_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/overrides/{target_type}/{target_id}",
    tag = "Channel Overrides",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("target_type" = String, Path, description = "Target type: 'role' or 'user'"),
        ("target_id" = String, Path, description = "Target ID (role or user ID)"),
    ),
    responses(
        (status = 204, description = "Override deleted"),
        (status = 400, description = "Bad request", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Override not found", body = ApiErrorBody),
    ),
)]
pub async fn delete_override(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<OverridePath>,
) -> Result<StatusCode, ApiError> {
    let target_type_i16 = parse_target_type(&path.target_type)?;

    let mut conn = state.db.get().await?;

    // Look up channel to get community_id.
    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table
            .find(&path.channel_id)
            .select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Check MANAGE_ROLES permission.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::MANAGE_ROLES,
    )
    .await?;

    let deleted = diesel_async::RunQueryDsl::execute(
        diesel::delete(
            channel_overrides::table
                .filter(channel_overrides::channel_id.eq(&path.channel_id))
                .filter(channel_overrides::target_type.eq(target_type_i16))
                .filter(channel_overrides::target_id.eq(&path.target_id)),
        ),
        &mut conn,
    )
    .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Override not found"));
    }

    audit_log::log(
        &state.db,
        &channel.community_id,
        &user_id,
        "channel_override.delete",
        Some("channel"),
        Some(&path.channel_id),
        Some(serde_json::json!({
            "target_type": path.target_type,
            "target_id": path.target_id,
        })),
        None,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
