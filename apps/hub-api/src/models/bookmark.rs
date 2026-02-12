use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::user_pod_bookmarks;

/// A user ↔ pod bookmark row from the database.
#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = user_pod_bookmarks)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct UserPodBookmark {
    pub user_id: String,
    pub pod_id: String,
    pub created_at: DateTime<Utc>,
}

/// Insertable struct for creating a new bookmark.
#[derive(Debug, Insertable)]
#[diesel(table_name = user_pod_bookmarks)]
pub struct NewUserPodBookmark {
    pub user_id: String,
    pub pod_id: String,
}
