use chrono::{DateTime, Utc};
use diesel::prelude::*;

use crate::db::schema::sessions;

/// Full session row from the database.
///
/// Note: `ip_address` (Inet) is omitted and handled separately if needed.
/// Using `Selectable` so `.select(Session::as_select())` only queries listed columns.
#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = sessions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub refresh_token: String,
    pub user_agent: Option<String>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    pub created_at: DateTime<Utc>,
}

/// Insertable struct for creating a new session.
///
/// `ip_address` omitted — defaults to NULL.
#[derive(Debug, Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession {
    pub id: String,
    pub user_id: String,
    pub refresh_token: String,
    pub user_agent: Option<String>,
    pub expires_at: DateTime<Utc>,
}
