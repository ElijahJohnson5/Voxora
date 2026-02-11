use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::users;

/// Full user row from the database.
#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: String,
    pub username: String,
    pub username_lower: String,
    pub display_name: String,
    pub email: Option<String>,
    pub email_verified: bool,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub avatar_url: Option<String>,
    pub flags: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Insertable struct for creating a new user.
#[derive(Debug, Insertable)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub id: String,
    pub username: String,
    pub username_lower: String,
    pub display_name: String,
    pub email: Option<String>,
    pub password_hash: String,
}

/// Public-facing user response (no sensitive fields).
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub avatar_url: Option<String>,
    pub flags: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
            email: u.email,
            email_verified: u.email_verified,
            avatar_url: u.avatar_url,
            flags: u.flags,
            status: u.status,
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}
