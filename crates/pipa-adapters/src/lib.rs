// SQLite repository + disk storage adapters (Phase 1).
// Postgres/S3 adapters arrive in Phase 5 behind Cargo features.

pub mod config;
pub mod crypto;
pub mod disk;
pub mod sqlite;

pub use config::{Config, ZoneConfig, load_config};
pub use crypto::{HmacKey, hash_password, verify_password};
pub use disk::DiskStorage;
pub use sqlite::{SqliteAuthStore, SqliteRepository, open_pool, run_migrations};
