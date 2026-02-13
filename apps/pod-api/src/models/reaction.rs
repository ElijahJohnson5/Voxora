use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Serialize, Serializer};
use utoipa::ToSchema;

use crate::db::schema::reactions;

fn serialize_i64_as_string<S: Serializer>(val: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&val.to_string())
}

#[derive(Debug, Clone, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = reactions)]
pub struct Reaction {
    #[serde(serialize_with = "serialize_i64_as_string")]
    #[schema(value_type = String)]
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
