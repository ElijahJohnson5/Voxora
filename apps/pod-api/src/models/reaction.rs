use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::Serialize;

use crate::db::schema::reactions;

#[derive(Debug, Clone, Queryable, Selectable, Serialize)]
#[diesel(table_name = reactions)]
pub struct Reaction {
    pub message_id: i64,
    pub user_id: String,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = reactions)]
pub struct NewReaction<'a> {
    pub message_id: i64,
    pub user_id: &'a str,
    pub emoji: &'a str,
    pub created_at: DateTime<Utc>,
}
