//! Community CRUD endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::OptionalExtension;
use diesel_async::AsyncConnection;
use scoped_futures::ScopedFutureExt;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{channels, communities, community_members, roles};
use crate::error::{ApiError, ApiErrorBody, FieldError};
use crate::gateway::events::EventName;
use crate::gateway::fanout::BroadcastPayload;
use crate::models::audit_log;
use crate::models::channel::{Channel, NewChannel};
use crate::models::community::{Community, CommunityResponse, NewCommunity, UpdateCommunity};
use crate::models::community_member::NewCommunityMember;
use crate::models::role::{NewRole, Role};
use crate::permissions;
use crate::pod_permissions;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/communities", post(create_community).get(list_communities))
        .route(
            "/communities/{id}",
            get(get_community)
                .patch(update_community)
                .delete(delete_community),
        )
}

// ---------------------------------------------------------------------------
// POST /api/v1/communities
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCommunityRequest {
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/communities",
    tag = "Communities",
    security(("bearer" = [])),
    request_body = CreateCommunityRequest,
    responses(
        (status = 201, description = "Community created", body = CommunityResponse),
        (status = 400, description = "Validation error", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    )
)]
pub async fn create_community(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateCommunityRequest>,
) -> Result<(StatusCode, Json<CommunityResponse>), ApiError> {
    // Check pod-level permission.
    pod_permissions::check_pod_permission(
        &state.db,
        state.config.pod_owner_id.as_deref(),
        &user_id,
        pod_permissions::POD_CREATE_COMMUNITY,
    )
    .await?;

    // Validate.
    let name = body.name.trim().to_string();
    let mut errors = Vec::new();
    if name.is_empty() {
        errors.push(FieldError {
            field: "name".to_string(),
            message: "Community name is required".to_string(),
        });
    } else if name.len() > 100 {
        errors.push(FieldError {
            field: "name".to_string(),
            message: "Community name must be 100 characters or fewer".to_string(),
        });
    }
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    let now = Utc::now();
    let community_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::COMMUNITY);
    let role_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::ROLE);
    let channel_id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::CHANNEL);
    let description = body.description;
    let icon_url = body.icon_url;

    let mut conn = state.db.get().await?;

    let (community, role, channel) = conn
        .transaction::<_, ApiError, _>(|conn| {
            async move {
                // 1. Insert community.
                diesel_async::RunQueryDsl::execute(
                    diesel::insert_into(communities::table).values(NewCommunity {
                        id: &community_id,
                        name: &name,
                        description: description.as_deref(),
                        icon_url: icon_url.as_deref(),
                        owner_id: &user_id,
                        default_channel: None,
                        member_count: 1,
                        created_at: now,
                        updated_at: now,
                    }),
                    conn,
                )
                .await?;

                // 2. Insert @everyone role.
                let role: Role = diesel_async::RunQueryDsl::get_result(
                    diesel::insert_into(roles::table)
                        .values(NewRole {
                            id: &role_id,
                            community_id: &community_id,
                            name: "@everyone",
                            color: None,
                            position: 0,
                            permissions: permissions::DEFAULT_EVERYONE_PERMISSIONS,
                            mentionable: false,
                            is_default: true,
                            created_at: now,
                        })
                        .returning(Role::as_returning()),
                    conn,
                )
                .await?;

                // 3. Insert #general channel.
                let channel: Channel = diesel_async::RunQueryDsl::get_result(
                    diesel::insert_into(channels::table)
                        .values(NewChannel {
                            id: &channel_id,
                            community_id: &community_id,
                            parent_id: None,
                            name: "general",
                            topic: None,
                            type_: 0,
                            position: 0,
                            slowmode_seconds: 0,
                            nsfw: false,
                            created_at: now,
                            updated_at: now,
                        })
                        .returning(Channel::as_returning()),
                    conn,
                )
                .await?;

                // 4. Update community.default_channel.
                let community: Community = diesel_async::RunQueryDsl::get_result(
                    diesel::update(communities::table.find(&community_id))
                        .set(communities::default_channel.eq(&channel_id))
                        .returning(Community::as_returning()),
                    conn,
                )
                .await?;

                // 5. Insert creator as community member.
                diesel_async::RunQueryDsl::execute(
                    diesel::insert_into(community_members::table).values(NewCommunityMember {
                        community_id: &community_id,
                        user_id: &user_id,
                        nickname: None,
                        roles: vec![],
                        joined_at: now,
                    }),
                    conn,
                )
                .await?;

                Ok((community, role, channel))
            }
            .scope_boxed()
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CommunityResponse {
            community,
            channels: vec![channel],
            roles: vec![role],
        }),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/communities",
    tag = "Communities",
    responses(
        (status = 200, description = "List of communities", body = [Community]),
    )
)]
pub async fn list_communities(State(state): State<AppState>) -> Result<Json<Vec<Community>>, ApiError> {
    let mut conn = state.db.get().await?;

    let list: Vec<Community> = diesel_async::RunQueryDsl::load(
        communities::table
            .order(communities::created_at.desc())
            .select(Community::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// GET /api/v1/communities/:id
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/communities/{id}",
    tag = "Communities",
    params(
        ("id" = String, Path, description = "Community ID"),
    ),
    responses(
        (status = 200, description = "Community details", body = CommunityResponse),
        (status = 404, description = "Community not found", body = ApiErrorBody),
    )
)]
pub async fn get_community(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CommunityResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    let community: Community = diesel_async::RunQueryDsl::get_result(
        communities::table.find(&id).select(Community::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Community not found"))?;

    let chs: Vec<Channel> = diesel_async::RunQueryDsl::load(
        channels::table
            .filter(channels::community_id.eq(&id))
            .order(channels::position.asc())
            .select(Channel::as_select()),
        &mut conn,
    )
    .await?;

    let rls: Vec<Role> = diesel_async::RunQueryDsl::load(
        roles::table
            .filter(roles::community_id.eq(&id))
            .order(roles::position.asc())
            .select(Role::as_select()),
        &mut conn,
    )
    .await?;

    Ok(Json(CommunityResponse {
        community,
        channels: chs,
        roles: rls,
    }))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/communities/:id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCommunityRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/communities/{id}",
    tag = "Communities",
    security(("bearer" = [])),
    params(
        ("id" = String, Path, description = "Community ID"),
    ),
    request_body = UpdateCommunityRequest,
    responses(
        (status = 200, description = "Community updated", body = Community),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
        (status = 404, description = "Community not found", body = ApiErrorBody),
    )
)]
pub async fn update_community(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateCommunityRequest>,
) -> Result<Json<Community>, ApiError> {
    // Check permission.
    permissions::check_permission(&state.db, &id, &user_id, permissions::MANAGE_COMMUNITY).await?;

    // Validate name if provided.
    if let Some(ref name) = body.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::validation(vec![FieldError {
                field: "name".to_string(),
                message: "Community name cannot be empty".to_string(),
            }]));
        }
        if name.len() > 100 {
            return Err(ApiError::validation(vec![FieldError {
                field: "name".to_string(),
                message: "Community name must be 100 characters or fewer".to_string(),
            }]));
        }
    }

    let mut conn = state.db.get().await?;

    // Fetch existing community for changes diff.
    let old: Community = diesel_async::RunQueryDsl::get_result(
        communities::table
            .find(&id)
            .select(Community::as_select()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Community not found"))?;

    let changeset = UpdateCommunity {
        name: body.name.map(|n| n.trim().to_string()),
        description: body.description,
        icon_url: body.icon_url,
        updated_at: Utc::now(),
    };

    let community: Community = diesel_async::RunQueryDsl::get_result(
        diesel::update(communities::table.find(&id))
            .set(&changeset)
            .returning(Community::as_returning()),
        &mut conn,
    )
    .await
    .optional()?
    .ok_or_else(|| ApiError::not_found("Community not found"))?;

    // Build changes JSON.
    let mut changes = serde_json::Map::new();
    if old.name != community.name {
        changes.insert(
            "name".to_string(),
            serde_json::json!({ "old": old.name, "new": community.name }),
        );
    }
    if old.description != community.description {
        changes.insert(
            "description".to_string(),
            serde_json::json!({ "old": old.description, "new": community.description }),
        );
    }
    if old.icon_url != community.icon_url {
        changes.insert(
            "icon_url".to_string(),
            serde_json::json!({ "old": old.icon_url, "new": community.icon_url }),
        );
    }
    let changes_val = if changes.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(changes))
    };

    audit_log::log(
        &state.db,
        &id,
        &user_id,
        "community.update",
        Some("community"),
        Some(&id),
        changes_val,
        None,
    )
    .await?;

    state.broadcast.dispatch(BroadcastPayload {
        community_id: id,
        event_name: EventName::COMMUNITY_UPDATE.to_string(),
        data: serde_json::to_value(&community).unwrap(),
    });

    Ok(Json(community))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/communities/:id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/v1/communities/{id}",
    tag = "Communities",
    security(("bearer" = [])),
    params(
        ("id" = String, Path, description = "Community ID"),
    ),
    responses(
        (status = 204, description = "Community deleted"),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
        (status = 403, description = "Forbidden", body = ApiErrorBody),
    )
)]
pub async fn delete_community(
    AuthUser { user_id }: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Owner can always delete. Otherwise, check POD_MANAGE_COMMUNITIES.
    if !permissions::is_owner(&state.db, &id, &user_id).await? {
        pod_permissions::check_pod_permission(
            &state.db,
            state.config.pod_owner_id.as_deref(),
            &user_id,
            pod_permissions::POD_MANAGE_COMMUNITIES,
        )
        .await?;
    }

    let mut conn = state.db.get().await?;

    let deleted =
        diesel_async::RunQueryDsl::execute(diesel::delete(communities::table.find(&id)), &mut conn)
            .await?;

    if deleted == 0 {
        return Err(ApiError::not_found("Community not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}
