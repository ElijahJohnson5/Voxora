//! Channel CRUD endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::Deserialize;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channels, communities};
use crate::error::{ApiError, FieldError};
use crate::gateway::events::EventName;
use crate::gateway::fanout::BroadcastPayload;
use crate::models::channel::{Channel, NewChannel, UpdateChannel};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/communities/{community_id}/channels",
            post(create_channel).get(list_channels),
        )
        .route(
            "/channels/{id}",
            get(get_channel)
                .patch(update_channel)
                .delete(delete_channel),
        )
}

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/channels
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub topic: Option<String>,
    pub position: Option<i32>,
    pub slowmode_seconds: Option<i32>,
    pub nsfw: Option<bool>,
}

async fn create_channel(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(community_id): Path<String>,
    Json(body): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<Channel>), ApiError> {
    // Check community exists.
    let mut conn = state.db.get().await?;
    diesel_async::RunQueryDsl::get_result::<String>(
        communities::table
            .find(&community_id)
            .select(communities::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Community not found"))?;

    // Check permission.
    permissions::check_permission(
        &state.db,
        &community_id,
        &user_id,
        permissions::MANAGE_CHANNELS,
    )
    .await?;

    // Validate name.
    let name = body.name.trim().to_string();
    let mut errors = Vec::new();
    if name.is_empty() {
        errors.push(FieldError {
            field: "name".to_string(),
            message: "Channel name is required".to_string(),
        });
    } else if name.len() > 100 {
        errors.push(FieldError {
            field: "name".to_string(),
            message: "Channel name must be 100 characters or fewer".to_string(),
        });
    }
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    let now = Utc::now();
    let channel_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::CHANNEL);

    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        diesel::insert_into(channels::table)
            .values(NewChannel {
                id: &channel_id,
                community_id: &community_id,
                parent_id: None,
                name: &name,
                topic: body.topic.as_deref(),
                type_: 0,
                position: body.position.unwrap_or(0),
                slowmode_seconds: body.slowmode_seconds.unwrap_or(0),
                nsfw: body.nsfw.unwrap_or(false),
                created_at: now,
                updated_at: now,
            })
            .returning(Channel::as_returning()),
        &mut conn,
    )
    .await?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: community_id.clone(),
        event_name: EventName::CHANNEL_CREATE.to_string(),
        data: serde_json::to_value(&channel).unwrap(),
    });

    Ok((StatusCode::CREATED, Json(channel)))
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/channels
// ---------------------------------------------------------------------------

async fn list_channels(
    State(state): State<AppState>,
    Path(community_id): Path<String>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    // Check community exists.
    let mut conn = state.db.get().await?;
    diesel_async::RunQueryDsl::get_result::<String>(
        communities::table
            .find(&community_id)
            .select(communities::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Community not found"))?;

    let list: Vec<Channel> = diesel_async::RunQueryDsl::load(
        channels::table
            .filter(channels::community_id.eq(&community_id))
            .order(channels::position.asc())
            .select(Channel::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:id
// ---------------------------------------------------------------------------

async fn get_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Channel>, ApiError> {
    let mut conn = state.db.get().await?;

    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table.find(&id).select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    Ok(Json(channel))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/channels/:id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: Option<i32>,
    pub nsfw: Option<bool>,
    pub slowmode_seconds: Option<i32>,
}

async fn update_channel(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateChannelRequest>,
) -> Result<Json<Channel>, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up channel to get community_id.
    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table.find(&id).select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Check permission on the channel's community.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::MANAGE_CHANNELS,
    )
    .await?;

    // Validate name if provided.
    if let Some(ref name) = body.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::validation(vec![FieldError {
                field: "name".to_string(),
                message: "Channel name cannot be empty".to_string(),
            }]));
        }
        if name.len() > 100 {
            return Err(ApiError::validation(vec![FieldError {
                field: "name".to_string(),
                message: "Channel name must be 100 characters or fewer".to_string(),
            }]));
        }
    }

    let changeset = UpdateChannel {
        name: body.name.map(|n| n.trim().to_string()),
        topic: body.topic,
        position: body.position,
        nsfw: body.nsfw,
        slowmode_seconds: body.slowmode_seconds,
        updated_at: Utc::now(),
    };

    let updated: Channel = diesel_async::RunQueryDsl::get_result(
        diesel::update(channels::table.find(&id))
            .set(&changeset)
            .returning(Channel::as_returning()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id.clone(),
        event_name: EventName::CHANNEL_UPDATE.to_string(),
        data: serde_json::to_value(&updated).unwrap(),
    });

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:id
// ---------------------------------------------------------------------------

async fn delete_channel(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up channel to get community_id.
    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table.find(&id).select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Check permission on the channel's community.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::MANAGE_CHANNELS,
    )
    .await?;

    // Prevent deletion of default channel.
    let default_channel: Option<String> = diesel_async::RunQueryDsl::get_result(
        communities::table
            .find(&channel.community_id)
            .select(communities::default_channel),
        &mut conn,
    )
    .await?;

    if default_channel.as_deref() == Some(&id) {
        return Err(ApiError::bad_request("Cannot delete the default channel"));
    }

    diesel_async::RunQueryDsl::execute(diesel::delete(channels::table.find(&id)), &mut conn)
        .await?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id,
        event_name: EventName::CHANNEL_DELETE.to_string(),
        data: serde_json::json!({
            "id": id,
        }),
    });

    Ok(StatusCode::NO_CONTENT)
}
