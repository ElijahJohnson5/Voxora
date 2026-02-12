//! Incoming opcode dispatch: IDENTIFY, HEARTBEAT, and unknown ops.

use std::collections::HashSet;

use diesel::prelude::*;
use serde_json::Value;

use crate::auth::tokens;
use crate::db::schema::{channels, communities, community_members, roles};
use crate::models::channel::Channel;
use crate::models::community::Community;
use crate::models::community_member::CommunityMember;
use crate::models::pod_user::PodUser;
use crate::models::role::Role;
use crate::AppState;

use super::events::{GatewayMessage, IdentifyPayload};
use super::session::GatewaySession;

/// Heartbeat interval sent to clients in the READY payload (ms).
pub const HEARTBEAT_INTERVAL_MS: u64 = 41250;

/// Process an IDENTIFY opcode. Returns a (`GatewaySession`, READY message) on success.
pub async fn handle_identify(
    state: &AppState,
    payload: IdentifyPayload,
) -> Result<(GatewaySession, GatewayMessage), &'static str> {
    // Consume the WS ticket (single-use).
    let ticket_data = tokens::consume_ws_ticket(state.kv.as_ref(), &payload.ticket)
        .await
        .map_err(|_| "Ticket lookup failed")?
        .ok_or("Invalid or expired ticket")?;

    let user_id = ticket_data.user_id;

    let mut conn = state.db.get().await.map_err(|_| "Database unavailable")?;

    // Load user profile.
    let user: PodUser = diesel_async::RunQueryDsl::get_result(
        crate::db::schema::pod_users::table
            .find(&user_id)
            .select(PodUser::as_select()),
        &mut conn,
    )
    .await
    .map_err(|_| "User not found")?;

    // Load community memberships.
    let memberships: Vec<CommunityMember> = diesel_async::RunQueryDsl::load(
        community_members::table
            .filter(community_members::user_id.eq(&user_id))
            .select(CommunityMember::as_select()),
        &mut conn,
    )
    .await
    .map_err(|_| "Failed to load memberships")?;

    let community_ids: Vec<String> = memberships.iter().map(|m| m.community_id.clone()).collect();
    let community_set: HashSet<String> = community_ids.iter().cloned().collect();

    // Build the READY payload with communities, channels, and roles.
    let mut community_data: Vec<Value> = Vec::new();

    if !community_ids.is_empty() {
        let comms: Vec<Community> = diesel_async::RunQueryDsl::load(
            communities::table
                .filter(communities::id.eq_any(&community_ids))
                .select(Community::as_select()),
            &mut conn,
        )
        .await
        .map_err(|_| "Failed to load communities")?;

        let all_channels: Vec<Channel> = diesel_async::RunQueryDsl::load(
            channels::table
                .filter(channels::community_id.eq_any(&community_ids))
                .order(channels::position.asc())
                .select(Channel::as_select()),
            &mut conn,
        )
        .await
        .map_err(|_| "Failed to load channels")?;

        let all_roles: Vec<Role> = diesel_async::RunQueryDsl::load(
            roles::table
                .filter(roles::community_id.eq_any(&community_ids))
                .order(roles::position.asc())
                .select(Role::as_select()),
            &mut conn,
        )
        .await
        .map_err(|_| "Failed to load roles")?;

        for comm in comms {
            let chs: Vec<&Channel> = all_channels
                .iter()
                .filter(|c| c.community_id == comm.id)
                .collect();
            let rls: Vec<&Role> = all_roles
                .iter()
                .filter(|r| r.community_id == comm.id)
                .collect();

            community_data.push(serde_json::json!({
                "id": comm.id,
                "name": comm.name,
                "description": comm.description,
                "icon_url": comm.icon_url,
                "owner_id": comm.owner_id,
                "member_count": comm.member_count,
                "channels": serde_json::to_value(&chs).unwrap_or_default(),
                "roles": serde_json::to_value(&rls).unwrap_or_default(),
            }));
        }
    }

    let session_id = voxora_common::id::prefixed_ulid("gw_");

    let ready_data = serde_json::json!({
        "session_id": session_id,
        "user": {
            "id": user.id,
            "username": user.username,
            "display_name": user.display_name,
            "avatar_url": user.avatar_url,
        },
        "communities": community_data,
        "heartbeat_interval": HEARTBEAT_INTERVAL_MS,
    });

    let session = GatewaySession::new(session_id, user_id, community_set);
    let seq = session.next_seq();
    let ready_msg = GatewayMessage::dispatch("READY", seq, ready_data);

    Ok((session, ready_msg))
}
