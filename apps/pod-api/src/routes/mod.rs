pub mod audit_log;
pub mod auth;
pub mod bans;
pub mod channel_overrides;
pub mod channels;
pub mod communities;
pub mod health;
pub mod invites;
pub mod members;
pub mod messages;
pub mod pins;
pub mod pod;
pub mod read_states;
pub mod roles;

use axum::Router;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(crate::gateway::server::router())
        .nest(
            "/api/v1",
            auth::router()
                .merge(communities::router())
                .merge(channels::router())
                .merge(messages::router())
                .merge(invites::router())
                .merge(members::router())
                .merge(roles::router())
                .merge(bans::router())
                .merge(pins::router())
                .merge(read_states::router())
                .merge(audit_log::router())
                .merge(pod::router())
                .merge(channel_overrides::router()),
        )
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        // Health
        health::health,
        // Auth
        auth::login,
        auth::refresh,
        // Communities
        communities::create_community,
        communities::list_communities,
        communities::get_community,
        communities::update_community,
        communities::delete_community,
        // Channels
        channels::create_channel,
        channels::list_channels,
        channels::get_channel,
        channels::update_channel,
        channels::delete_channel,
        // Messages
        messages::send_message,
        messages::list_messages,
        messages::edit_message,
        messages::delete_message,
        // Reactions
        messages::add_reaction,
        messages::remove_reaction,
        messages::list_reactions,
        // Members
        members::list_members,
        members::get_member,
        members::remove_member,
        members::update_member,
        // Roles
        roles::list_roles,
        roles::create_role,
        roles::update_role,
        roles::delete_role,
        // Invites
        invites::create_invite,
        invites::list_invites,
        invites::delete_invite,
        invites::get_invite,
        invites::accept_invite,
        // Bans
        bans::ban_member,
        bans::unban_member,
        // Pins
        pins::pin_message,
        pins::unpin_message,
        pins::list_pins,
        // Read States
        read_states::get_unread_counts,
        read_states::mark_as_read,
        // Audit Log
        audit_log::list_audit_log,
        // Pod Roles
        pod::list_pod_roles,
        pod::create_pod_role,
        pod::update_pod_role,
        pod::delete_pod_role,
        pod::assign_pod_role,
        pod::unassign_pod_role,
        // Pod Bans
        pod::list_pod_bans,
        pod::pod_ban_user,
        pod::pod_unban_user,
        // Channel Overrides
        channel_overrides::list_overrides,
        channel_overrides::upsert_override,
        channel_overrides::delete_override,
    ),
    components(
        schemas(
            // Error types
            crate::error::ApiErrorBody,
            crate::error::ApiErrorDetail,
            crate::error::FieldError,
            // Models
            crate::models::community::Community,
            crate::models::community::CommunityResponse,
            crate::models::channel::Channel,
            crate::models::message::Message,
            crate::models::community_member::CommunityMember,
            crate::models::role::Role,
            crate::models::invite::Invite,
            crate::models::reaction::Reaction,
            crate::models::pod_user::PodUser,
            crate::models::ban::Ban,
            // Route request/response types
            health::HealthResponse,
            auth::LoginRequest,
            auth::LoginResponse,
            auth::UserInfo,
            auth::RefreshRequest,
            auth::RefreshResponse,
            communities::CreateCommunityRequest,
            communities::UpdateCommunityRequest,
            channels::CreateChannelRequest,
            channels::UpdateChannelRequest,
            messages::SendMessageRequest,
            messages::EditMessageRequest,
            messages::ListMessagesResponse,
            members::ListMembersResponse,
            members::UpdateMemberRequest,
            roles::CreateRoleRequest,
            roles::UpdateRoleRequest,
            invites::CreateInviteRequest,
            invites::InviteInfoResponse,
            bans::BanRequest,
            read_states::UnreadCountsResponse,
            read_states::ChannelUnreadEntry,
            read_states::MarkAsReadRequest,
            audit_log::AuditLogResponse,
            crate::models::audit_log::AuditLogEntry,
            // Pod models
            crate::models::pod_role::PodRole,
            crate::models::pod_ban::PodBan,
            crate::models::channel_override::ChannelOverride,
            // Pod route types
            pod::CreatePodRoleRequest,
            pod::UpdatePodRoleRequest,
            pod::PodBanRequest,
            channel_overrides::UpsertOverrideRequest,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Health", description = "Health check"),
        (name = "Auth", description = "Authentication"),
        (name = "Communities", description = "Community management"),
        (name = "Channels", description = "Channel management"),
        (name = "Messages", description = "Messaging"),
        (name = "Reactions", description = "Message reactions"),
        (name = "Members", description = "Community members"),
        (name = "Roles", description = "Role management"),
        (name = "Invites", description = "Invite management"),
        (name = "Bans", description = "Ban management"),
        (name = "Pins", description = "Message pinning"),
        (name = "Read States", description = "Read state and unread counts"),
        (name = "Audit Log", description = "Audit log"),
        (name = "Pod Roles", description = "Pod-level role management"),
        (name = "Pod Bans", description = "Pod-level ban management"),
        (name = "Channel Overrides", description = "Channel permission overrides"),
    )
)]
pub struct ApiDoc;
