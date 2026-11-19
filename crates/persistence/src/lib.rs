//! Shared persistence for Shirori services: the schema migrations and the durable job queue.

pub mod jobs;

use sqlx::migrate::Migrator;

/// All SQL migrations in `database/migrations`, embedded at compile time.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../database/migrations");
