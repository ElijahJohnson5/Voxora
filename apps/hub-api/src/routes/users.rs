use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::db::schema::{pods, user_pod_bookmarks, users};
use crate::error::{ApiError, FieldError};
use crate::models::pod::{Pod, PodResponse};
use crate::models::user::{NewUser, PublicUserResponse, User, UserResponse};
use crate::AppState;

/// POST /api/v1/users — Register a new user.
#[derive(Debug, Deserialize)]
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
        .route("/users/:user_id", get(get_user))
}

async fn create_user(
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
async fn get_me(
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

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// `PATCH /api/v1/users/@me` — Update the current user's profile.
async fn update_me(
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
async fn get_user(
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

#[derive(Debug, Serialize)]
pub struct MyPodsResponse {
    pub data: Vec<PodResponse>,
}

/// `GET /api/v1/users/@me/pods` — List Pods the current user has bookmarked.
async fn get_my_pods(
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

    let data: Vec<PodResponse> = bookmarked_pods.into_iter().map(PodResponse::from).collect();

    Ok(Json(MyPodsResponse { data }))
}
