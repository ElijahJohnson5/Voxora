use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::roles;

#[derive(Debug, Queryable, Selectable, Serialize)]
#[diesel(table_name = roles)]
pub struct Role {
    pub id: String,
    pub community_id: String,
    pub name: String,
    pub color: Option<i32>,
    pub position: i32,
    pub permissions: i64,
    pub mentionable: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = roles)]
pub struct NewRole<'a> {
    pub id: &'a str,
    pub community_id: &'a str,
    pub name: &'a str,
    pub color: Option<i32>,
    pub position: i32,
    pub permissions: i64,
    pub mentionable: bool,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}
