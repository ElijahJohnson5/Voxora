use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::middleware::AuthUser;
use crate::db::schema::{pods, user_pod_bookmarks, user_preferences, users};
use crate::error::{ApiError, ApiErrorBody, FieldError};
use crate::models::pod::{Pod, PodResponse};
use crate::models::user::{NewUser, PublicUserResponse, User, UserResponse};
use crate::AppState;

/// POST /api/v1/users — Register a new user.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    pub display_name: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", post(create_user))
        .route("/users/@me", get(get_me).patch(update_me))
        .route("/users/@me/pods", get(get_my_pods))
        .route(
            "/users/@me/preferences",
            get(get_preferences).patch(update_preferences),
        )
        .route("/users/{user_id}", get(get_user))
}

#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "Users",
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "User created", body = UserResponse),
        (status = 400, description = "Validation error", body = ApiErrorBody),
        (status = 409, description = "Username or email conflict", body = ApiErrorBody),
    ),
)]
pub async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), ApiError> {
    // --- Validation ---
    let mut errors: Vec<FieldError> = Vec::new();

    // Username: 2–32 chars, alphanumeric + _ . -
    let username = body.username.trim().to_string();
    if username.len() < 2 || username.len() > 32 {
        errors.push(FieldError {
            field: "username".into(),
            message: "Username must be 2–32 characters".into(),
        });
    } else if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        errors.push(FieldError {
            field: "username".into(),
            message: "Username may only contain letters, digits, underscores, dots, and hyphens"
                .into(),
        });
    }

    // Display name: 1–64 chars
    let display_name = body.display_name.trim().to_string();
    if display_name.is_empty() || display_name.len() > 64 {
        errors.push(FieldError {
            field: "display_name".into(),
            message: "Display name must be 1–64 characters".into(),
        });
    }

    // Email: basic presence check (when provided)
    let email = body.email.as_ref().map(|e| e.trim().to_lowercase());
    if let Some(ref e) = email {
        if !e.contains('@') || e.len() < 3 {
            errors.push(FieldError {
                field: "email".into(),
                message: "Invalid email address".into(),
            });
        }
    }

    // Password: min 10 chars
    if body.password.len() < 10 {
        errors.push(FieldError {
            field: "password".into(),
            message: "Password must be at least 10 characters".into(),
        });
    }

    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    // --- Hash password with Argon2id ---
    let password_hash = hash_password(&body.password)?;

    // --- Generate ID ---
    let id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::USER);
    let username_lower = username.to_lowercase();

    let new_user = NewUser {
        id,
        username: username.clone(),
        username_lower,
        display_name,
        email,
        password_hash,
    };

    // --- Insert ---
    let mut conn = state.db.get().await?;

    let user: User = diesel::insert_into(users::table)
        .values(&new_user)
        .returning(users::all_columns)
        .get_result(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                ref info,
            ) => {
                let constraint = info.constraint_name().unwrap_or("");
                if constraint.contains("username") {
                    ApiError::conflict("Username is already taken")
                } else if constraint.contains("email") {
                    ApiError::conflict("Email is already registered")
                } else {
                    ApiError::conflict("A user with that information already exists")
                }
            }
            other => ApiError::from(other),
        })?;

    tracing::info!(user_id = %user.id, username = %user.username, "user registered");

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}

/// Hash a password using Argon2id with a random salt.
fn hash_password(password: &str) -> Result<String, ApiError> {
    use argon2::Argon2;
    use password_hash::rand_core::OsRng;
    use password_hash::{PasswordHasher, SaltString};

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::error!(?e, "password hashing failed");
            ApiError::internal("Failed to process password")
        })
}

// =========================================================================
// GET /api/v1/users/@me — Current authenticated user
// =========================================================================

/// `GET /api/v1/users/@me` — Return the current user's full profile.
#[utoipa::path(
    get,
    path = "/api/v1/users/@me",
    tag = "Users",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Current user profile", body = UserResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    ),
)]
pub async fn get_me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UserResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    let user: User = users::table
        .find(&auth.user_id)
        .select(User::as_select())
        .first(&mut conn)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(UserResponse::from(user)))
}

// =========================================================================
// PATCH /api/v1/users/@me — Update own profile
// =========================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// `PATCH /api/v1/users/@me` — Update the current user's profile.
#[utoipa::path(
    patch,
    path = "/api/v1/users/@me",
    tag = "Users",
    security(("bearer" = [])),
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Updated user profile", body = UserResponse),
        (status = 400, description = "Validation error", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    ),
)]
pub async fn update_me(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    // --- Validation ---
    let mut errors: Vec<FieldError> = Vec::new();

    let display_name = body.display_name.as_ref().map(|n| n.trim().to_string());
    if let Some(ref name) = display_name {
        if name.is_empty() || name.len() > 64 {
            errors.push(FieldError {
                field: "display_name".into(),
                message: "Display name must be 1–64 characters".into(),
            });
        }
    }

    let avatar_url = body.avatar_url.as_ref().map(|u| u.trim().to_string());
    if let Some(ref url) = avatar_url {
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
            errors.push(FieldError {
                field: "avatar_url".into(),
                message: "Avatar URL must start with http:// or https://".into(),
            });
        }
    }

    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    // If nothing to update, just return the current user.
    if display_name.is_none() && avatar_url.is_none() {
        return get_me(State(state), auth).await;
    }

    let mut conn = state.db.get().await?;
    let now = Utc::now();

    // Build update — always set updated_at.
    let user: User = diesel::update(users::table.find(&auth.user_id))
        .set((
            display_name
                .as_deref()
                .map(|n| users::display_name.eq(n.to_string())),
            avatar_url.as_deref().map(|u| {
                if u.is_empty() {
                    users::avatar_url.eq(None::<String>)
                } else {
                    users::avatar_url.eq(Some(u.to_string()))
                }
            }),
            Some(users::updated_at.eq(now)),
        ))
        .returning(users::all_columns)
        .get_result(&mut conn)
        .await
        .map_err(ApiError::from)?;

    tracing::info!(user_id = %user.id, "profile updated");

    Ok(Json(UserResponse::from(user)))
}

// =========================================================================
// GET /api/v1/users/{user_id} — Public profile
// =========================================================================

/// `GET /api/v1/users/{user_id}` — Return a user's public profile.
#[utoipa::path(
    get,
    path = "/api/v1/users/{user_id}",
    tag = "Users",
    params(
        ("user_id" = String, Path, description = "User ID"),
    ),
    responses(
        (status = 200, description = "Public user profile", body = PublicUserResponse),
        (status = 404, description = "User not found", body = ApiErrorBody),
    ),
)]
pub async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<PublicUserResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    let user: User = users::table
        .find(&user_id)
        .select(User::as_select())
        .first(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found("User not found"))?;

    Ok(Json(PublicUserResponse::from(user)))
}

// =========================================================================
// GET /api/v1/users/@me/pods — List bookmarked pods
// =========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct MyPodEntry {
    #[serde(flatten)]
    pub pod: PodResponse,
    pub preferred: bool,
    pub relay: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyPodsResponse {
    pub data: Vec<MyPodEntry>,
}

/// `GET /api/v1/users/@me/pods` — List Pods the current user has bookmarked.
#[utoipa::path(
    get,
    path = "/api/v1/users/@me/pods",
    tag = "Users",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Bookmarked pods", body = MyPodsResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    ),
)]
pub async fn get_my_pods(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<MyPodsResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    let bookmarked_pods: Vec<Pod> = user_pod_bookmarks::table
        .inner_join(pods::table.on(pods::id.eq(user_pod_bookmarks::pod_id)))
        .filter(user_pod_bookmarks::user_id.eq(&auth.user_id))
        .filter(pods::status.eq("active"))
        .order(user_pod_bookmarks::created_at.desc())
        .select(Pod::as_select())
        .load(&mut conn)
        .await
        .map_err(ApiError::from)?;

    // Load user's preferred pods
    let preferred_pods: Vec<String> = user_preferences::table
        .find(&auth.user_id)
        .select(user_preferences::preferred_pods)
        .first::<Vec<String>>(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?
        .unwrap_or_default();

    let data: Vec<MyPodEntry> = bookmarked_pods
        .into_iter()
        .map(|p| {
            let preferred = preferred_pods.contains(&p.id);
            MyPodEntry {
                pod: PodResponse::from(p),
                preferred,
                relay: false, // No managed pods in Phase 2
            }
        })
        .collect();

    Ok(Json(MyPodsResponse { data }))
}

// =========================================================================
// GET /api/v1/users/@me/preferences — User preferences
// =========================================================================

#[derive(Debug, Serialize, ToSchema)]
pub struct PreferencesResponse {
    pub preferred_pods: Vec<String>,
    pub max_preferred_pods: i32,
}

/// `GET /api/v1/users/@me/preferences` — Return the current user's preferences.
#[utoipa::path(
    get,
    path = "/api/v1/users/@me/preferences",
    tag = "Users",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "User preferences", body = PreferencesResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    ),
)]
pub async fn get_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<PreferencesResponse>, ApiError> {
    let mut conn = state.db.get().await?;

    let row = user_preferences::table
        .find(&auth.user_id)
        .select(user_preferences::preferred_pods)
        .first::<Vec<String>>(&mut conn)
        .await
        .optional()
        .map_err(ApiError::from)?;

    let preferred_pods = row.unwrap_or_default();

    Ok(Json(PreferencesResponse {
        preferred_pods,
        max_preferred_pods: 10,
    }))
}

// =========================================================================
// PATCH /api/v1/users/@me/preferences — Update user preferences
// =========================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePreferencesRequest {
    pub preferred_pods: Vec<String>,
}

/// `PATCH /api/v1/users/@me/preferences` — Update the current user's preferences.
#[utoipa::path(
    patch,
    path = "/api/v1/users/@me/preferences",
    tag = "Users",
    security(("bearer" = [])),
    request_body = UpdatePreferencesRequest,
    responses(
        (status = 200, description = "Updated preferences", body = PreferencesResponse),
        (status = 400, description = "Validation error", body = ApiErrorBody),
        (status = 401, description = "Unauthorized", body = ApiErrorBody),
    ),
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpdatePreferencesRequest>,
) -> Result<Json<PreferencesResponse>, ApiError> {
    // Validate max 10
    if body.preferred_pods.len() > 10 {
        return Err(ApiError::validation(vec![FieldError {
            field: "preferred_pods".into(),
            message: "Maximum of 10 preferred pods allowed".into(),
        }]));
    }

    let mut conn = state.db.get().await?;

    if !body.preferred_pods.is_empty() {
        // Validate all pod IDs exist and are active
        let active_pods: Vec<String> = pods::table
            .filter(pods::id.eq_any(&body.preferred_pods))
            .filter(pods::status.eq("active"))
            .select(pods::id)
            .load(&mut conn)
            .await
            .map_err(ApiError::from)?;

        if active_pods.len() != body.preferred_pods.len() {
            return Err(ApiError::bad_request(
                "One or more pod IDs are invalid or inactive",
            ));
        }

        // Validate user has bookmarks for all listed pods
        let bookmarked: Vec<String> = user_pod_bookmarks::table
            .filter(user_pod_bookmarks::user_id.eq(&auth.user_id))
            .filter(user_pod_bookmarks::pod_id.eq_any(&body.preferred_pods))
            .select(user_pod_bookmarks::pod_id)
            .load(&mut conn)
            .await
            .map_err(ApiError::from)?;

        if bookmarked.len() != body.preferred_pods.len() {
            return Err(ApiError::bad_request(
                "You must be a member of all preferred pods",
            ));
        }
    }

    // Upsert
    diesel::insert_into(user_preferences::table)
        .values((
            user_preferences::user_id.eq(&auth.user_id),
            user_preferences::preferred_pods.eq(&body.preferred_pods),
            user_preferences::updated_at.eq(Utc::now()),
        ))
        .on_conflict(user_preferences::user_id)
        .do_update()
        .set((
            user_preferences::preferred_pods.eq(&body.preferred_pods),
            user_preferences::updated_at.eq(Utc::now()),
        ))
        .execute(&mut conn)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(PreferencesResponse {
        preferred_pods: body.preferred_pods,
        max_preferred_pods: 10,
    }))
}
