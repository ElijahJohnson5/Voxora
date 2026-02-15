//! Read-state endpoints: unread counts and mark-as-read.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use diesel::sql_types::{BigInt, Integer, Nullable, Text};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channels, read_states};
use crate::error::{ApiError, ApiErrorBody};
use crate::models::channel::Channel;
use crate::models::read_state::NewReadState;
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/unread-counts", get(get_unread_counts))
        .route("/channels/{channel_id}/read", put(mark_as_read))
}

// ---------------------------------------------------------------------------
// GET /api/v1/unread-counts
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountsResponse {
    pub channels: Vec<ChannelUnreadEntry>,
    pub last_updated: DateTime<Utc>,
}

fn serialize_i64_as_string<S: serde::Serializer>(val: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&val.to_string())
}

fn serialize_option_i64_as_string<S: serde::Serializer>(
    val: &Option<i64>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match val {
        Some(v) => s.serialize_some(&v.to_string()),
        None => s.serialize_none(),
    }
}

#[derive(Debug, Serialize, ToSchema, QueryableByName)]
pub struct ChannelUnreadEntry {
    #[diesel(sql_type = Text)]
    pub channel_id: String,
    #[diesel(sql_type = Text)]
    pub community_id: String,
    #[diesel(sql_type = BigInt)]
    #[serde(serialize_with = "serialize_i64_as_string")]
    #[schema(value_type = String)]
    pub unread_count: i64,
    #[diesel(sql_type = Integer)]
    pub mention_count: i32,
    #[diesel(sql_type = Nullable<BigInt>)]
    #[serde(serialize_with = "serialize_option_i64_as_string")]
    #[schema(value_type = Option<String>)]
    pub last_message_id: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/unread-counts",
    tag = "Read States",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Unread counts per channel", body = UnreadCountsResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    ),
)]
pub async fn get_unread_counts(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
) -> Result<(StatusCode, [(axum::http::header::HeaderName, &'static str); 1], Json<UnreadCountsResponse>), ApiError> {
    let mut conn = state.db.get().await?;

    let entries: Vec<ChannelUnreadEntry> = diesel_async::RunQueryDsl::load(
        diesel::sql_query(
            "SELECT \
                rs.channel_id, \
                rs.community_id, \
                rs.mention_count, \
                (SELECT COUNT(*) FROM messages m WHERE m.channel_id = rs.channel_id AND m.id > rs.last_read_id) AS unread_count, \
                (SELECT MAX(m.id) FROM messages m WHERE m.channel_id = rs.channel_id) AS last_message_id \
            FROM read_states rs \
            WHERE rs.user_id = $1"
        )
        .bind::<Text, _>(&user_id),
        &mut conn,
    )
    .await?;

    let last_updated = Utc::now();

    Ok((
        StatusCode::OK,
        [(axum::http::header::CACHE_CONTROL, "max-age=10")],
        Json(UnreadCountsResponse {
            channels: entries,
            last_updated,
        }),
    ))
}

// ---------------------------------------------------------------------------
// PUT /api/v1/channels/:channel_id/read
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkAsReadRequest {
    #[schema(value_type = String)]
    pub message_id: String,
}

#[utoipa::path(
    put,
    path = "/api/v1/channels/{channel_id}/read",
    tag = "Read States",
    security(("bearer" = [])),
    params(
        ("channel_id" = String, Path, description = "Channel ID"),
    ),
    request_body = MarkAsReadRequest,
    responses(
        (status = 204, description = "Channel marked as read"),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Channel not found", body = ApiErrorBody),
    ),
)]
pub async fn mark_as_read(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
    Json(body): Json<MarkAsReadRequest>,
) -> Result<StatusCode, ApiError> {
    let message_id: i64 = body
        .message_id
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid message_id"))?;

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

    // Check VIEW_CHANNEL permission.
    permissions::check_permission(
        &state.db,
        &channel.community_id,
        &user_id,
        permissions::VIEW_CHANNEL,
    )
    .await?;

    // UPSERT read_states row.
    diesel_async::RunQueryDsl::execute(
        diesel::insert_into(read_states::table)
            .values(NewReadState {
                user_id: &user_id,
                channel_id: &channel_id,
                community_id: &channel.community_id,
                last_read_id: message_id,
                mention_count: 0,
            })
            .on_conflict((read_states::user_id, read_states::channel_id))
            .do_update()
            .set((
                read_states::last_read_id.eq(message_id),
                read_states::mention_count.eq(0),
                read_states::community_id.eq(&channel.community_id),
                read_states::updated_at.eq(diesel::dsl::now),
            )),
        &mut conn,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
