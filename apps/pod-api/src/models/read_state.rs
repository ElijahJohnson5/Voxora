use chrono::{DateTime, Utc};
use diesel::prelude::*;

use crate::db::schema::read_states;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = read_states)]
pub struct ReadState {
    pub user_id: String,
    pub channel_id: String,
    pub community_id: String,
    pub last_read_id: i64,
    pub mention_count: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = read_states)]
pub struct NewReadState<'a> {
    pub user_id: &'a str,
    pub channel_id: &'a str,
    pub community_id: &'a str,
    pub last_read_id: i64,
    pub mention_count: i32,
}
