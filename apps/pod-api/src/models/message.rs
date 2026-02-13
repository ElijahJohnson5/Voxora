use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;

use crate::db::schema::messages;

fn serialize_i64_as_string<S: Serializer>(val: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&val.to_string())
}

fn serialize_option_i64_as_string<S: Serializer>(
    val: &Option<i64>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match val {
        Some(v) => s.serialize_some(&v.to_string()),
        None => s.serialize_none(),
    }
}

/// Deserialize an i64 from either a JSON number or a JSON string.
pub fn deserialize_string_or_number<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        Num(i64),
        Str(String),
    }

    let val: Option<StringOrNumber> = Option::deserialize(d)?;
    match val {
        None => Ok(None),
        Some(StringOrNumber::Num(n)) => Ok(Some(n)),
        Some(StringOrNumber::Str(s)) => {
            s.parse::<i64>().map(Some).map_err(serde::de::Error::custom)
        }
    }
}

#[derive(Debug, Clone, Queryable, Selectable, Serialize, ToSchema)]
#[diesel(table_name = messages)]
pub struct Message {
    #[serde(serialize_with = "serialize_i64_as_string")]
    #[schema(value_type = String)]
    pub id: i64,
    pub channel_id: String,
    pub author_id: String,
    pub content: Option<String>,
    #[serde(rename = "type")]
    pub type_: i16,
    pub flags: i32,
    #[serde(serialize_with = "serialize_option_i64_as_string")]
    #[schema(value_type = Option<String>)]
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
