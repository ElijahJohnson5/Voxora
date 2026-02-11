//! Standalone migration runner for hub-api.
//!
//! Usage:
//!   cargo run -p hub-api --bin migrate
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

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL env var is required");

    println!("Connecting to database...");
    let mut conn = PgConnection::establish(&database_url).expect("failed to connect to database");

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
