use std::path::Path;

use sqlx::migrate::MigrateError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database connection failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] MigrateError),
}

pub async fn connect(path: &Path) -> Result<SqlitePool, DatabaseError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
