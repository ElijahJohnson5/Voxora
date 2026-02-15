//! Pin/unpin message endpoints.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::Deserialize;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channels, messages};
use crate::error::{ApiError, ApiErrorBody};
use crate::gateway::events::EventName;
use crate::gateway::fanout::BroadcastPayload;
use crate::models::audit_log;
use crate::models::channel::Channel;
use crate::models::message::Message;
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels/{channel_id}/pins", get(list_pins))
        .route(
            "/channels/{channel_id}/pins/{message_id}",
            axum::routing::put(pin_message).delete(unpin_message),
        )
}

#[derive(Debug, Deserialize)]
pub struct PinPath {
    pub channel_id: String,
    pub message_id: String,
}

impl PinPath {
    fn message_id_i64(&self) -> Result<i64, ApiError> {
        self.message_id
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid message ID"))
    }
}

// ---------------------------------------------------------------------------
// PUT /api/v1/channels/:channel_id/pins/:message_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/pins/{message_id}",
    tag = "Pins",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
    ),
    responses(
        (status = 200, description = "Message pinned", body = Message),
        (status = 400, description = "Max pins reached", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn pin_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<PinPath>,
) -> Result<Json<Message>, ApiError> {
    let message_id = path.message_id_i64()?;
    let mut conn = state.db.get().await?;

    // Look up message by id + channel_id.
    let message: Message = diesel_async::RunQueryDsl::get_result(
        messages::table
            .filter(messages::id.eq(message_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(Message::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

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

    // Check MANAGE_MESSAGES permission (channel-aware).
    permissions::check_channel_permission(
        &state.db,
        &channel.community_id,
        &path.channel_id,
        &user_id,
        permissions::MANAGE_MESSAGES,
    )
    .await?;

    // If already pinned, return idempotently.
    if message.pinned {
        return Ok(Json(message));
    }

    // Count pinned messages in channel — max 50.
    let pin_count: i64 = diesel_async::RunQueryDsl::get_result(
        messages::table
            .filter(messages::channel_id.eq(&path.channel_id))
            .filter(messages::pinned.eq(true))
            .count(),
        &mut conn,
    )
    .await?;

    if pin_count >= 50 {
        return Err(ApiError::bad_request(
            "Maximum of 50 pinned messages per channel",
        ));
    }

    // Update message to pinned.
    let updated: Message = diesel_async::RunQueryDsl::get_result(
        diesel::update(
            messages::table
                .filter(messages::id.eq(message_id))
                .filter(messages::channel_id.eq(&path.channel_id)),
        )
        .set(messages::pinned.eq(true))
        .returning(Message::as_returning()),
        &mut conn,
    )
    .await?;

    audit_log::log(
        &state.db,
        &channel.community_id,
        &user_id,
        "message.pin",
        Some("message"),
        Some(&path.message_id),
        None,
        None,
    )
    .await?;

    // Broadcast CHANNEL_PINS_UPDATE.
    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id,
        event_name: EventName::CHANNEL_PINS_UPDATE.to_string(),
        data: serde_json::json!({
            "channel_id": path.channel_id,
        }),
    });

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/pins/:message_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/pins/{message_id}",
    tag = "Pins",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
    ),
    responses(
        (status = 200, description = "Message unpinned", body = Message),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn unpin_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<PinPath>,
) -> Result<Json<Message>, ApiError> {
    let message_id = path.message_id_i64()?;
    let mut conn = state.db.get().await?;

    // Look up message by id + channel_id.
    let message: Message = diesel_async::RunQueryDsl::get_result(
        messages::table
            .filter(messages::id.eq(message_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(Message::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

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

    // Check MANAGE_MESSAGES permission (channel-aware).
    permissions::check_channel_permission(
        &state.db,
        &channel.community_id,
        &path.channel_id,
        &user_id,
        permissions::MANAGE_MESSAGES,
    )
    .await?;

    // If not pinned, return 404.
    if !message.pinned {
        return Err(ApiError::not_found("Message is not pinned"));
    }

    // Update message to unpinned.
    let updated: Message = diesel_async::RunQueryDsl::get_result(
        diesel::update(
            messages::table
                .filter(messages::id.eq(message_id))
                .filter(messages::channel_id.eq(&path.channel_id)),
        )
        .set(messages::pinned.eq(false))
        .returning(Message::as_returning()),
        &mut conn,
    )
    .await?;

    audit_log::log(
        &state.db,
        &channel.community_id,
        &user_id,
        "message.unpin",
        Some("message"),
        Some(&path.message_id),
        None,
        None,
    )
    .await?;

    // Broadcast CHANNEL_PINS_UPDATE.
    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id,
        event_name: EventName::CHANNEL_PINS_UPDATE.to_string(),
        data: serde_json::json!({
            "channel_id": path.channel_id,
        }),
    });

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/pins
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/pins",
    tag = "Pins",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
    ),
    responses(
        (status = 200, description = "Pinned messages", body = [Message]),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Channel not found", body = ApiErrorBody),
    ),
)]
pub async fn list_pins(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
) -> Result<Json<Vec<Message>>, ApiError> {
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

    // Check VIEW_CHANNEL permission (channel-aware).
    permissions::check_channel_permission(
        &state.db,
        &channel.community_id,
        &channel_id,
        &user_id,
        permissions::VIEW_CHANNEL,
    )
    .await?;

    // Fetch pinned messages ordered by most recently created first.
    let pins: Vec<Message> = diesel_async::RunQueryDsl::load(
        messages::table
            .filter(messages::channel_id.eq(&channel_id))
            .filter(messages::pinned.eq(true))
            .order(messages::created_at.desc())
            .select(Message::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(pins))
}
