//! Standalone migration runner for pod-api.
//!
//! Usage:
//!   cargo run -p pod-api --bin pod-migrate
//!   cargo run -p pod-api --bin pod-migrate -- --test
//!
//! Reads DATABASE_URL from the environment (or .env via dotenvy).

use diesel::pg::PgConnection;
use diesel::Connection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use std::path::Path;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

fn main() {
    if dotenvy::dotenv().is_err() {
        let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
        let _ = dotenvy::from_path(env_path);
    }

    let mut database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL env var is required");

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--test") {
        database_url = with_test_db_suffix(&database_url);
    }

    println!("Connecting to database...");
    let mut conn =
        PgConnection::establish(&database_url).expect("failed to connect to database");

    println!("Running pending migrations...");
    let applied = conn
        .run_pending_migrations(MIGRATIONS)
        .expect("failed to run migrations");

    if applied.is_empty() {
        println!("No pending migrations.");
    } else {
        for migration in &applied {
            println!("  Applied: {migration}");
        }
        println!("{} migration(s) applied.", applied.len());
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
