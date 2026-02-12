//! Invite endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use diesel_async::AsyncConnection;
use rand::Rng;
use scoped_futures::ScopedFutureExt;
use serde::Deserialize;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{bans, communities, community_members, invites};
use crate::error::ApiError;
use crate::gateway::events::EventName;
use crate::gateway::fanout::BroadcastPayload;
use crate::models::community_member::{CommunityMember, NewCommunityMember};
use crate::models::invite::{Invite, NewInvite};
use crate::permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/communities/{community_id}/invites",
            post(create_invite).get(list_invites),
        )
        .route(
            "/communities/{community_id}/invites/{code}",
            delete(delete_invite),
        )
        .route("/invites/{code}/accept", post(accept_invite))
}

// ---------------------------------------------------------------------------
// POST /api/v1/communities/:community_id/invites
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateInviteRequest {
    pub max_uses: Option<i32>,
    pub max_age_seconds: Option<i32>,
}

fn generate_invite_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

async fn create_invite(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(community_id): Path<String>,
    Json(body): Json<CreateInviteRequest>,
) -> Result<(StatusCode, Json<Invite>), ApiError> {
    permissions::check_permission(
        &state.db,
        &community_id,
        &user_id,
        permissions::INVITE_MEMBERS,
    )
    .await?;

    if let Some(max_uses) = body.max_uses {
        if max_uses <= 0 {
            return Err(ApiError::bad_request("max_uses must be greater than 0"));
        }
    }
    if let Some(max_age) = body.max_age_seconds {
        if max_age <= 0 {
            return Err(ApiError::bad_request(
                "max_age_seconds must be greater than 0",
            ));
        }
    }

    let now = Utc::now();
    let expires_at = body
        .max_age_seconds
        .map(|s| now + Duration::seconds(s as i64));
    let code = generate_invite_code();

    let mut conn = state.db.get().await?;

    let invite: Invite = diesel_async::RunQueryDsl::get_result(
        diesel::insert_into(invites::table)
            .values(NewInvite {
                code: &code,
                community_id: &community_id,
                channel_id: None,
                inviter_id: &user_id,
                max_uses: body.max_uses,
                use_count: 0,
                max_age_seconds: body.max_age_seconds,
                created_at: now,
                expires_at,
            })
            .returning(Invite::as_returning()),
        &mut conn,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(invite)))
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:community_id/invites
// ---------------------------------------------------------------------------

async fn list_invites(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(community_id): Path<String>,
) -> Result<Json<Vec<Invite>>, ApiError> {
    permissions::check_permission(
        &state.db,
        &community_id,
        &user_id,
        permissions::MANAGE_COMMUNITY,
    )
    .await?;

    let now = Utc::now();
    let mut conn = state.db.get().await?;

    let list: Vec<Invite> = diesel_async::RunQueryDsl::load(
        invites::table
            .filter(invites::community_id.eq(&community_id))
            .filter(
                invites::expires_at
                    .is_null()
                    .or(invites::expires_at.gt(now)),
            )
            .select(Invite::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:community_id/invites/:code
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InvitePath {
    pub community_id: String,
    pub code: String,
}

async fn delete_invite(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(path): Path<InvitePath>,
) -> Result<StatusCode, ApiError> {
    permissions::check_permission(
        &state.db,
        &path.community_id,
        &user_id,
        permissions::MANAGE_COMMUNITY,
    )
    .await?;

    let mut conn = state.db.get().await?;

    let deleted = diesel_async::RunQueryDsl::execute(
        diesel::delete(
            invites::table
                .filter(invites::code.eq(&path.code))
                .filter(invites::community_id.eq(&path.community_id)),
        ),
        &mut conn,
    )
    .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Invite not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/v1/invites/:code/accept
// ---------------------------------------------------------------------------

async fn accept_invite(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<(StatusCode, Json<CommunityMember>), ApiError> {
    let mut conn = state.db.get().await?;

    // Look up invite.
    let invite: Invite = diesel_async::RunQueryDsl::get_result(
        invites::table.find(&code).select(Invite::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Invite not found"))?;

    // Check not expired.
    if let Some(expires_at) = invite.expires_at {
        if expires_at < Utc::now() {
            return Err(ApiError::bad_request("Invite has expired"));
        }
    }

    // Check use count.
    if let Some(max_uses) = invite.max_uses {
        if invite.use_count >= max_uses {
            return Err(ApiError::bad_request("Invite has reached maximum uses"));
        }
    }

    // Check not already a member.
    let existing: Option<CommunityMember> = diesel_async::RunQueryDsl::get_result(
        community_members::table
            .find((&invite.community_id, &user_id))
            .select(CommunityMember::as_select()),
        &mut conn,
    )
    .await
    .optional()?;

    if existing.is_some() {
        return Err(ApiError::conflict(
            "You are already a member of this community",
        ));
    }

    // Check if user is banned.
    let banned: Option<String> = diesel_async::RunQueryDsl::get_result(
        bans::table
            .find((&invite.community_id, &user_id))
            .select(bans::user_id),
        &mut conn,
    )
    .await
    .optional()?;

    if banned.is_some() {
        return Err(ApiError::forbidden("You are banned from this community"));
    }

    // Transaction: insert member + increment use_count + increment member_count.
    let now = Utc::now();
    let community_id = invite.community_id.clone();

    let member = conn
        .transaction::<_, ApiError, _>(|conn| {
            async move {
                let member: CommunityMember = diesel_async::RunQueryDsl::get_result(
                    diesel::insert_into(community_members::table)
                        .values(NewCommunityMember {
                            community_id: &community_id,
                            user_id: &user_id,
                            nickname: None,
                            roles: vec![],
                            joined_at: now,
                        })
                        .returning(CommunityMember::as_returning()),
                    conn,
                )
                .await?;

                diesel_async::RunQueryDsl::execute(
                    diesel::update(invites::table.find(&code))
                        .set(invites::use_count.eq(invites::use_count + 1)),
                    conn,
                )
                .await?;

                diesel_async::RunQueryDsl::execute(
                    diesel::update(communities::table.find(&community_id))
                        .set(communities::member_count.eq(communities::member_count + 1)),
                    conn,
                )
                .await?;

                Ok(member)
            }
            .scope_boxed()
        })
        .await?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: invite.community_id,
        event_name: EventName::MEMBER_JOIN.to_string(),
        data: serde_json::to_value(&member).unwrap(),
    });

    Ok((StatusCode::CREATED, Json(member)))
}
