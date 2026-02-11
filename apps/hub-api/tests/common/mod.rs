use std::sync::Arc;

use axum::Router;
use hub_api::auth::keys::SigningKeys;
use hub_api::config::Config;
use hub_api::db::pool::DbPool;
use hub_api::AppState;

/// Build an [`AppState`] connected to the real dev database and Redis.
///
/// Reads connection strings from the `.env` file at `CARGO_MANIFEST_DIR`.
pub async fn test_state() -> AppState {
    // Load .env from the hub-api crate root so tests work from any cwd.
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    let _ = dotenvy::from_path(env_path);

    let mut config = Config::from_env();
    config.database_url = with_test_db_suffix(&config.database_url);
    let db = hub_api::db::pool::connect(&config.database_url).await;

    let redis_client = redis::Client::open(config.redis_url.as_str()).expect("invalid REDIS_URL");
    let redis = redis::aio::ConnectionManager::new(redis_client)
        .await
        .expect("failed to connect to Redis");

    let keys = Arc::new(SigningKeys::from_seed(&config.signing_key_seed));

    AppState {
        db,
        redis,
        keys,
        config: Arc::new(config),
    }
}

fn with_test_db_suffix(database_url: &str) -> String {
    let mut parts = database_url.splitn(2, '?');
    let base = parts.next().unwrap_or(database_url);
    let query = parts.next();

    let mut base_parts = base.rsplitn(2, '/');
    let db_name = base_parts.next().unwrap_or("");
    let prefix = base_parts.next().unwrap_or("");

    if db_name.is_empty() || db_name.ends_with("_test") {
        return database_url.to_string();
    }

    let mut updated = format!("{}/{}", prefix, format!("{db_name}_test"));
    if let Some(query) = query {
        updated.push('?');
        updated.push_str(query);
    }
    updated
}

/// Build the full application [`Router`] wired to the test state.
pub async fn test_app() -> (Router, AppState) {
    let state = test_state().await;
    let app = hub_api::routes::router().with_state(state.clone());
    (app, state)
}

/// Create a unique test user and return its ID.
///
/// Uses a random suffix so tests don't clash.
pub async fn create_test_user(db: &DbPool, password: &str) -> TestUser {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    let suffix: u32 = rand::random();
    let username = format!("testuser_{suffix}");
    let email = format!("test_{suffix}@example.com");

    // Hash the password with Argon2id (same as the real registration route).
    let password_hash = {
        use argon2::Argon2;
        use password_hash::rand_core::OsRng;
        use password_hash::{PasswordHasher, SaltString};
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .expect("argon2 hash")
            .to_string()
    };

    let id = voxora_common::id::prefixed_ulid(voxora_common::id::prefix::USER);

    let mut conn = db.get().await.expect("pool");

    diesel::insert_into(hub_api::db::schema::users::table)
        .values((
            hub_api::db::schema::users::id.eq(&id),
            hub_api::db::schema::users::username.eq(&username),
            hub_api::db::schema::users::username_lower.eq(&username.to_lowercase()),
            hub_api::db::schema::users::display_name.eq(&username),
            hub_api::db::schema::users::email.eq(&email),
            hub_api::db::schema::users::password_hash.eq(&password_hash),
        ))
        .execute(&mut conn)
        .await
        .expect("insert test user");

    TestUser {
        id,
        username,
        email,
        password: password.to_string(),
    }
}

pub struct TestUser {
    pub id: String,
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Clean up a test user and their sessions.
pub async fn cleanup_test_user(db: &DbPool, user_id: &str) {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;

    let mut conn = db.get().await.expect("pool");
    diesel::delete(
        hub_api::db::schema::sessions::table
            .filter(hub_api::db::schema::sessions::user_id.eq(user_id)),
    )
    .execute(&mut conn)
    .await
    .ok();
    diesel::delete(
        hub_api::db::schema::users::table.filter(hub_api::db::schema::users::id.eq(user_id)),
    )
    .execute(&mut conn)
    .await
    .ok();
}
