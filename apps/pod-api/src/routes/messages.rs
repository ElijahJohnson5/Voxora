//! Message CRUD endpoints.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channels, messages};
use crate::error::{ApiError, FieldError};
use crate::models::channel::Channel;
use crate::models::message::{Message, NewMessage, UpdateMessage};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/channels/:channel_id/messages",
            post(send_message).get(list_messages),
        )
        .route(
            "/api/v1/channels/:channel_id/messages/:message_id",
            axum::routing::patch(edit_message).delete(delete_message),
        )
}

// ---------------------------------------------------------------------------
// POST /api/v1/channels/:channel_id/messages
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub content: Option<String>,
    pub reply_to: Option<i64>,
}

async fn send_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
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

    // Check SEND_MESSAGES permission.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::SEND_MESSAGES,
    )
    .await?;

    // Validate content.
    let content = body.content.as_deref().map(|s| s.trim());
    let mut errors = Vec::new();
    match content {
        None | Some("") => {
            errors.push(FieldError {
                field: "content".to_string(),
                message: "Message content is required".to_string(),
            });
        }
        Some(c) if c.len() > 4000 => {
            errors.push(FieldError {
                field: "content".to_string(),
                message: "Message content must be 4000 characters or fewer".to_string(),
            });
        }
        _ => {}
    }
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    let content = content.unwrap();

    // Validate reply_to if provided.
    if let Some(reply_id) = body.reply_to {
        let exists: Option<i64> = diesel_async::RunQueryDsl::get_result(
            messages::table
                .filter(messages::id.eq(reply_id))
                .filter(messages::channel_id.eq(&channel_id))
                .select(messages::id),
            &mut conn,
        )
        .await
        .optional()?;

        if exists.is_none() {
            return Err(ApiError::not_found("Replied-to message not found"));
        }
    }

    let id = state.snowflake.generate();
    let now = Utc::now();

    let message: Message = diesel_async::RunQueryDsl::get_result(
        diesel::insert_into(messages::table).values(NewMessage {
            id,
            channel_id: &channel_id,
            author_id: &user_id,
            content: Some(content),
            type_: 0,
            flags: 0,
            reply_to: body.reply_to,
            pinned: false,
            created_at: now,
        })
        .returning(Message::as_returning()),
        &mut conn,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(message)))
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/messages
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListMessagesParams {
    pub before: Option<i64>,
    pub after: Option<i64>,
    pub around: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListMessagesResponse {
    pub data: Vec<Message>,
    pub has_more: bool,
}

async fn list_messages(
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<ListMessagesResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    // Check channel exists.
    diesel_async::RunQueryDsl::get_result::<String>(
        channels::table
            .find(&channel_id)
            .select(channels::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);

    if let Some(around) = params.around {
        // Fetch half before + half after the target.
        let half = limit / 2;

        let before_msgs: Vec<Message> = diesel_async::RunQueryDsl::load(
            messages::table
                .filter(messages::channel_id.eq(&channel_id))
                .filter(messages::id.lt(around))
                .order(messages::id.desc())
                .limit(half)
                .select(Message::as_select()),
            &mut conn,
        )
        .await?;

        let after_msgs: Vec<Message> = diesel_async::RunQueryDsl::load(
            messages::table
                .filter(messages::channel_id.eq(&channel_id))
                .filter(messages::id.ge(around))
                .order(messages::id.asc())
                .limit(limit - half)
                .select(Message::as_select()),
            &mut conn,
        )
        .await?;

        let mut data: Vec<Message> = before_msgs.into_iter().rev().collect();
        data.extend(after_msgs);

        return Ok(Json(ListMessagesResponse {
            data,
            has_more: false,
        }));
    }

    if let Some(after) = params.after {
        // Fetch messages after the cursor, ascending.
        let rows: Vec<Message> = diesel_async::RunQueryDsl::load(
            messages::table
                .filter(messages::channel_id.eq(&channel_id))
                .filter(messages::id.gt(after))
                .order(messages::id.asc())
                .limit(limit + 1)
                .select(Message::as_select()),
            &mut conn,
        )
        .await?;

        let has_more = rows.len() as i64 > limit;
        let data: Vec<Message> = rows.into_iter().take(limit as usize).collect();

        return Ok(Json(ListMessagesResponse { data, has_more }));
    }

    // Default: before cursor (or latest messages).
    let mut query = messages::table
        .filter(messages::channel_id.eq(&channel_id))
        .order(messages::id.desc())
        .limit(limit + 1)
        .select(Message::as_select())
        .into_boxed();

    if let Some(before) = params.before {
        query = query.filter(messages::id.lt(before));
    }

    let rows: Vec<Message> = diesel_async::RunQueryDsl::load(query, &mut conn).await?;

    let has_more = rows.len() as i64 > limit;
    let mut data: Vec<Message> = rows.into_iter().take(limit as usize).collect();
    data.reverse(); // Return in ascending (chronological) order.

    Ok(Json(ListMessagesResponse { data, has_more }))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/channels/:channel_id/messages/:message_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MessagePath {
    pub channel_id: String,
    pub message_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct EditMessageRequest {
    pub content: String,
}

async fn edit_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MessagePath>,
    Json(body): Json<EditMessageRequest>,
) -> Result<Json<Message>, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up message by id + channel_id.
    let message: Message = diesel_async::RunQueryDsl::get_result(
        messages::table
            .filter(messages::id.eq(path.message_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(Message::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

    // Only the author can edit.
    if message.author_id != user_id {
        return Err(ApiError::forbidden(
            "You can only edit your own messages",
        ));
    }

    // Validate content.
    let content = body.content.trim().to_string();
    if content.is_empty() {
        return Err(ApiError::validation(vec![FieldError {
            field: "content".to_string(),
            message: "Message content cannot be empty".to_string(),
        }]));
    }
    if content.len() > 4000 {
        return Err(ApiError::validation(vec![FieldError {
            field: "content".to_string(),
            message: "Message content must be 4000 characters or fewer".to_string(),
        }]));
    }

    let changeset = UpdateMessage {
        content: Some(content),
        edited_at: Some(Utc::now()),
    };

    let updated: Message = diesel_async::RunQueryDsl::get_result(
        diesel::update(
            messages::table
                .filter(messages::id.eq(path.message_id))
                .filter(messages::channel_id.eq(&path.channel_id)),
        )
        .set(&changeset)
        .returning(Message::as_returning()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/messages/:message_id
// ---------------------------------------------------------------------------

async fn delete_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MessagePath>,
) -> Result<StatusCode, ApiError> {
    let mut conn = state.db.get().await?;

    // Look up message by id + channel_id.
    let message: Message = diesel_async::RunQueryDsl::get_result(
        messages::table
            .filter(messages::id.eq(path.message_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(Message::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

    // If not the author, check MANAGE_MESSAGES permission.
    if message.author_id != user_id {
        let channel: Channel = diesel_async::RunQueryDsl::get_result(
            channels::table
                .find(&path.channel_id)
                .select(Channel::as_select()),
            &mut conn,
        )
        .await
        .optional()?
        .ok_or_else(|| ApiError::not_found("Channel not found"))?;

        permissions::check_permission(
            &state.db,
            &channel.community_id,
            &user_id,
            permissions::MANAGE_MESSAGES,
        )
        .await?;
    }

    diesel_async::RunQueryDsl::execute(
        diesel::delete(
            messages::table
                .filter(messages::id.eq(path.message_id))
                .filter(messages::channel_id.eq(&path.channel_id)),
        ),
        &mut conn,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
