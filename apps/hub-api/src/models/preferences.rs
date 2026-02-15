use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::user_preferences;

/// Row from the `user_preferences` table.
#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = user_preferences)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct UserPreferences {
    pub user_id: String,
    pub preferred_pods: Vec<String>,
    pub updated_at: DateTime<Utc>,
}
