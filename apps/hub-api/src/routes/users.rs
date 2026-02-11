use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use diesel_async::RunQueryDsl;
use serde::Deserialize;

use crate::db::schema::users;
use crate::error::{ApiError, FieldError};
use crate::models::user::{NewUser, User, UserResponse};
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
    Router::new().route("/users", post(create_user))
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
