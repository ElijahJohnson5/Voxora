use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;
use utoipa::ToSchema;

use crate::db::schema::channels;

#[derive(Debug, AsChangeset)]
#[diesel(table_name = channels)]
pub struct UpdateChannel {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: Option<i32>,
    pub nsfw: Option<bool>,
    pub slowmode_seconds: Option<i32>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = channels)]
pub struct Channel {
    pub id: String,
    pub community_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub topic: Option<String>,
    #[serde(rename = "type")]
    pub type_: i16,
    pub position: i32,
    pub slowmode_seconds: i32,
    pub nsfw: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = channels)]
pub struct NewChannel<'a> {
    pub id: &'a str,
    pub community_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub name: &'a str,
    pub topic: Option<&'a str>,
    pub type_: i16,
    pub position: i32,
    pub slowmode_seconds: i32,
    pub nsfw: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
