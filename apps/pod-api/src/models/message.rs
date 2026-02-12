use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::messages;

#[derive(Debug, Clone, Queryable, Selectable, Serialize)]
#[diesel(table_name = messages)]
pub struct Message {
    pub id: i64,
    pub channel_id: String,
    pub author_id: String,
    pub content: Option<String>,
    #[serde(rename = "type")]
    pub type_: i16,
    pub flags: i32,
    pub reply_to: Option<i64>,
    pub edited_at: Option<DateTime<Utc>>,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub id: i64,
    pub channel_id: &'a str,
    pub author_id: &'a str,
    pub content: Option<&'a str>,
    pub type_: i16,
    pub flags: i32,
    pub reply_to: Option<i64>,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = messages)]
pub struct UpdateMessage {
    pub content: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
}
