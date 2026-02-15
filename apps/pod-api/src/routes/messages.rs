//! Message CRUD endpoints.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channels, community_members, messages, reactions, read_states};
use crate::error::{ApiError, ApiErrorBody, FieldError};
use crate::gateway::events::EventName;
use crate::gateway::fanout::BroadcastPayload;
use crate::models::audit_log;
use crate::models::channel::Channel;
use crate::models::message::{Message, NewMessage, UpdateMessage};
use crate::models::reaction::{NewReaction, Reaction};
use crate::models::read_state::NewReadState;
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/messages",
            post(send_message).get(list_messages),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}",
            axum::routing::patch(edit_message).delete(delete_message),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
            axum::routing::put(add_reaction)
                .delete(remove_reaction)
                .get(list_reactions),
        )
}

// ---------------------------------------------------------------------------
// POST /api/v1/channels/:channel_id/messages
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendMessageRequest {
    pub content: Option<String>,
    #[serde(default, deserialize_with = "crate::models::message::deserialize_string_or_number")]
    #[schema(value_type = Option<String>)]
    pub reply_to: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/v1/channels/{channel_id}/messages",
    tag = "Messages",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
    ),
    request_body = SendMessageRequest,
    responses(
        (status = 201, description = "Message sent", body = Message),
        (status = 400, description = "Validation error", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Channel not found", body = ApiErrorBody),
    ),
)]
pub async fn send_message(
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

    // Check SEND_MESSAGES permission (channel-aware).
    permissions::check_channel_permission(
        &state.db,
        &channel.community_id,
        &channel_id,
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
        diesel::insert_into(messages::table)
            .values(NewMessage {
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

    // Increment channel message count.
    let _ = diesel_async::RunQueryDsl::execute(
        diesel::update(channels::table.find(&channel_id))
            .set(channels::message_count.eq(channels::message_count + 1)),
        &mut conn,
    )
    .await;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id.clone(),
        event_name: EventName::MESSAGE_CREATE.to_string(),
        data: serde_json::to_value(&message).unwrap(),
    });

    // Mention detection: parse <@user_id> patterns and increment mention_count.
    let mentioned_ids = parse_mentions(content);
    if !mentioned_ids.is_empty() {
        // Filter to only community members (excluding the author).
        let valid_members: Vec<String> = diesel_async::RunQueryDsl::load(
            community_members::table
                .filter(community_members::community_id.eq(&channel.community_id))
                .filter(community_members::user_id.eq_any(&mentioned_ids))
                .filter(community_members::user_id.ne(&user_id))
                .select(community_members::user_id),
            &mut conn,
        )
        .await
        .unwrap_or_default();

        for mentioned_user_id in &valid_members {
            let _ = diesel_async::RunQueryDsl::execute(
                diesel::insert_into(read_states::table)
                    .values(NewReadState {
                        user_id: mentioned_user_id,
                        channel_id: &channel_id,
                        community_id: &channel.community_id,
                        last_read_id: 0,
                        mention_count: 1,
                    })
                    .on_conflict((read_states::user_id, read_states::channel_id))
                    .do_update()
                    .set((
                        read_states::mention_count.eq(read_states::mention_count + 1),
                        read_states::community_id.eq(&channel.community_id),
                        read_states::updated_at.eq(diesel::dsl::now),
                    )),
                &mut conn,
            )
            .await;
        }
    }

    Ok((StatusCode::CREATED, Json(message)))
}

/// Extract user IDs from `<@user_id>` mention patterns in message content.
fn parse_mentions(content: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    let mut search_from = 0;
    while let Some(start) = content[search_from..].find("<@") {
        let abs_start = search_from + start + 2; // skip "<@"
        if let Some(end) = content[abs_start..].find('>') {
            let user_id = &content[abs_start..abs_start + end];
            if !user_id.is_empty() && !mentions.contains(&user_id.to_string()) {
                mentions.push(user_id.to_string());
            }
            search_from = abs_start + end + 1;
        } else {
            break;
        }
    }
    mentions
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/messages
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListMessagesParams {
    pub before: Option<String>,
    pub after: Option<String>,
    pub around: Option<String>,
    pub limit: Option<i64>,
}

impl ListMessagesParams {
    fn before_id(&self) -> Option<i64> {
        self.before.as_deref().and_then(|s| s.parse().ok())
    }
    fn after_id(&self) -> Option<i64> {
        self.after.as_deref().and_then(|s| s.parse().ok())
    }
    fn around_id(&self) -> Option<i64> {
        self.around.as_deref().and_then(|s| s.parse().ok())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListMessagesResponse {
    pub data: Vec<Message>,
    pub has_more: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/messages",
    tag = "Messages",
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("before" = Option<String>, Query, description = "Fetch messages before this ID"),
        ("after" = Option<String>, Query, description = "Fetch messages after this ID"),
        ("around" = Option<String>, Query, description = "Fetch messages around this ID"),
        ("limit" = Option<i64>, Query, description = "Number of messages (1-100, default 50)"),
    ),
    responses(
        (status = 200, description = "List of messages", body = ListMessagesResponse),
        (status = 404, description = "Channel not found", body = ApiErrorBody),
    ),
)]
pub async fn list_messages(
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<ListMessagesResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    // Check channel exists.
    diesel_async::RunQueryDsl::get_result::<String>(
        channels::table.find(&channel_id).select(channels::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);

    if let Some(around) = params.around_id() {
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

    if let Some(after) = params.after_id() {
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

    if let Some(before) = params.before_id() {
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
    pub message_id: String,
}

impl MessagePath {
    fn message_id_i64(&self) -> Result<i64, ApiError> {
        self.message_id
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid message ID"))
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditMessageRequest {
    pub content: String,
}

#[utoipa::path(
    patch,
    path = "/api/v1/channels/{channel_id}/messages/{message_id}",
    tag = "Messages",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
    ),
    request_body = EditMessageRequest,
    responses(
        (status = 200, description = "Message edited", body = Message),
        (status = 400, description = "Validation error", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn edit_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MessagePath>,
    Json(body): Json<EditMessageRequest>,
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

    // Only the author can edit.
    if message.author_id != user_id {
        return Err(ApiError::forbidden("You can only edit your own messages"));
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
                .filter(messages::id.eq(message_id))
                .filter(messages::channel_id.eq(&path.channel_id)),
        )
        .set(&changeset)
        .returning(Message::as_returning()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

    // Look up channel to get community_id for broadcast.
    let channel: Channel = diesel_async::RunQueryDsl::get_result(
        channels::table
            .find(&path.channel_id)
            .select(Channel::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id,
        event_name: EventName::MESSAGE_UPDATE.to_string(),
        data: serde_json::to_value(&updated).unwrap(),
    });

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/messages/:message_id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/messages/{message_id}",
    tag = "Messages",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
    ),
    responses(
        (status = 204, description = "Message deleted"),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn delete_message(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<MessagePath>,
) -> Result<StatusCode, ApiError> {
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

    // If not the author, check MANAGE_MESSAGES permission (channel-aware).
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

        permissions::check_channel_permission(
            &state.db,
            &channel.community_id,
            &path.channel_id,
            &user_id,
            permissions::MANAGE_MESSAGES,
        )
        .await?;
    }

    // Look up channel community_id for broadcast.
    let channel_community_id: String = diesel_async::RunQueryDsl::get_result(
        channels::table
            .find(&path.channel_id)
            .select(channels::community_id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Log audit entry if mod-delete (author != deleter).
    let is_mod_delete = message.author_id != user_id;

    diesel_async::RunQueryDsl::execute(
        diesel::delete(
            messages::table
                .filter(messages::id.eq(message_id))
                .filter(messages::channel_id.eq(&path.channel_id)),
        ),
        &mut conn,
    )
    .await?;

    if is_mod_delete {
        audit_log::log(
            &state.db,
            &channel_community_id,
            &user_id,
            "message.delete",
            Some("message"),
            Some(&path.message_id),
            None,
            None,
        )
        .await?;
    }

    // Decrement channel message count.
    let _ = diesel_async::RunQueryDsl::execute(
        diesel::update(channels::table.find(&path.channel_id))
            .set(channels::message_count.eq(
                diesel::dsl::sql::<diesel::sql_types::Int4>("GREATEST(message_count - 1, 0)"),
            )),
        &mut conn,
    )
    .await;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel_community_id,
        event_name: EventName::MESSAGE_DELETE.to_string(),
        data: serde_json::json!({
            "id": path.message_id,
            "channel_id": path.channel_id,
        }),
    });

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Reactions
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReactionPath {
    pub channel_id: String,
    pub message_id: String,
    pub emoji: String,
}

impl ReactionPath {
    fn message_id_i64(&self) -> Result<i64, ApiError> {
        self.message_id
            .parse()
            .map_err(|_| ApiError::bad_request("Invalid message ID"))
    }
}

// ---------------------------------------------------------------------------
// PUT /api/v1/channels/:channel_id/messages/:message_id/reactions/:emoji
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
    tag = "Reactions",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
        ("emoji" = String, Path, description = "Emoji"),
    ),
    responses(
        (status = 200, description = "Reaction added", body = Reaction),
        (status = 400, description = "Invalid emoji", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn add_reaction(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<ReactionPath>,
) -> Result<Json<Reaction>, ApiError> {
    let msg_id = path.message_id_i64()?;
    let mut conn = state.db.get().await?;

    // Verify message exists in this channel.
    let message_id: i64 = diesel_async::RunQueryDsl::get_result(
        messages::table
            .filter(messages::id.eq(msg_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(messages::id),
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

    // Check USE_REACTIONS permission (channel-aware).
    permissions::check_channel_permission(
        &state.db,
        &channel.community_id,
        &path.channel_id,
        &user_id,
        permissions::USE_REACTIONS,
    )
    .await?;

    // Validate emoji.
    if path.emoji.is_empty() || path.emoji.len() > 32 {
        return Err(ApiError::bad_request(
            "Emoji must be between 1 and 32 characters",
        ));
    }

    // Check max 20 unique emoji per message.
    let distinct_count: i64 = diesel_async::RunQueryDsl::get_result(
        reactions::table
            .filter(reactions::message_id.eq(message_id))
            .select(diesel::dsl::count_distinct(reactions::emoji)),
        &mut conn,
    )
    .await?;

    if distinct_count >= 20 {
        // Check if this emoji already exists on the message.
        let emoji_exists: Option<String> = diesel_async::RunQueryDsl::get_result(
            reactions::table
                .filter(reactions::message_id.eq(message_id))
                .filter(reactions::emoji.eq(&path.emoji))
                .select(reactions::emoji),
            &mut conn,
        )
        .await
        .optional()?;

        if emoji_exists.is_none() {
            return Err(ApiError::bad_request(
                "Maximum of 20 unique emoji reactions per message",
            ));
        }
    }

    // Insert (idempotent via ON CONFLICT DO NOTHING).
    let now = Utc::now();
    diesel_async::RunQueryDsl::execute(
        diesel::insert_into(reactions::table)
            .values(NewReaction {
                message_id,
                user_id: &user_id,
                emoji: &path.emoji,
                created_at: now,
            })
            .on_conflict_do_nothing(),
        &mut conn,
    )
    .await?;

    // Fetch the row (may have been previously inserted with a different created_at).
    let reaction: Reaction = diesel_async::RunQueryDsl::get_result(
        reactions::table
            .filter(reactions::message_id.eq(message_id))
            .filter(reactions::user_id.eq(&user_id))
            .filter(reactions::emoji.eq(&path.emoji))
            .select(Reaction::as_select()),
        &mut conn,
    )
    .await?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel.community_id,
        event_name: EventName::MESSAGE_REACTION_ADD.to_string(),
        data: serde_json::to_value(&reaction).unwrap(),
    });

    Ok(Json(reaction))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/channels/:channel_id/messages/:message_id/reactions/:emoji
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
    tag = "Reactions",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
        ("emoji" = String, Path, description = "Emoji"),
    ),
    responses(
        (status = 204, description = "Reaction removed"),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn remove_reaction(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<ReactionPath>,
) -> Result<StatusCode, ApiError> {
    let msg_id = path.message_id_i64()?;
    let mut conn = state.db.get().await?;

    // Verify message exists in this channel.
    diesel_async::RunQueryDsl::get_result::<i64>(
        messages::table
            .filter(messages::id.eq(msg_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(messages::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

    // Look up channel community_id for broadcast.
    let channel_community_id: String = diesel_async::RunQueryDsl::get_result(
        channels::table
            .find(&path.channel_id)
            .select(channels::community_id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Channel not found"))?;

    // Delete the reaction (no error if absent).
    diesel_async::RunQueryDsl::execute(
        diesel::delete(
            reactions::table
                .filter(reactions::message_id.eq(msg_id))
                .filter(reactions::user_id.eq(&user_id))
                .filter(reactions::emoji.eq(&path.emoji)),
        ),
        &mut conn,
    )
    .await?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: channel_community_id,
        event_name: EventName::MESSAGE_REACTION_REMOVE.to_string(),
        data: serde_json::json!({
            "message_id": path.message_id,
            "user_id": user_id,
            "emoji": path.emoji,
            "channel_id": path.channel_id,
        }),
    });

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/v1/channels/:channel_id/messages/:message_id/reactions/:emoji
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
    tag = "Reactions",
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
        ("message_id" = String, Path, description = "Message ID"),
        ("emoji" = String, Path, description = "Emoji"),
    ),
    responses(
        (status = 200, description = "List of reactions", body = [Reaction]),
        (status = 404, description = "Message not found", body = ApiErrorBody),
    ),
)]
pub async fn list_reactions(
    State(state): State<AppState>,
    Path(path): Path<ReactionPath>,
) -> Result<Json<Vec<Reaction>>, ApiError> {
    let msg_id = path.message_id_i64()?;
    let mut conn = state.db.get().await?;

    // Verify message exists in this channel.
    diesel_async::RunQueryDsl::get_result::<i64>(
        messages::table
            .filter(messages::id.eq(msg_id))
            .filter(messages::channel_id.eq(&path.channel_id))
            .select(messages::id),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Message not found"))?;

    let results: Vec<Reaction> = diesel_async::RunQueryDsl::load(
        reactions::table
            .filter(reactions::message_id.eq(msg_id))
            .filter(reactions::emoji.eq(&path.emoji))
            .select(Reaction::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(results))
}
