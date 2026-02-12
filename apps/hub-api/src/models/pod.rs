use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::pods;

/// Full pod row from the database.
#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = pods)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Pod {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub url: String,
    pub region: Option<String>,
    pub client_id: String,
    #[serde(skip)]
    pub client_secret: String,
    pub public: bool,
    pub capabilities: Vec<String>,
    pub max_members: i32,
    pub version: Option<String>,
    pub status: String,
    pub member_count: i32,
    pub online_count: i32,
    pub community_count: i32,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
