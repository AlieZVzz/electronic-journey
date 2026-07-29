use std::{collections::HashSet, path::Path, time::Duration};

use chrono::{DateTime, Utc};
use sqlx::migrate::MigrateError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use uuid::Uuid;

const MAX_TIMELINE_PAGE_SIZE: u16 = 50;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database connection failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] MigrateError),
    #[error("database contains an invalid capture size")]
    InvalidFileSize,
    #[error("capture is still referenced by an AI task")]
    CaptureInUse,
    #[error("capture does not exist")]
    CaptureNotFound,
}

pub async fn connect(path: &Path) -> Result<SqlitePool, DatabaseError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub struct NewCaptureRecord<'a> {
    pub id: Uuid,
    pub device_id: &'a str,
    pub display_id: &'a str,
    pub captured_at_utc: DateTime<Utc>,
    pub timezone: &'a str,
    pub local_path: &'a str,
    pub thumbnail_path: Option<&'a str>,
    pub file_size: u64,
    pub content_sha256: &'a str,
    pub thumbnail_state: &'a str,
}

pub async fn insert_capture(
    pool: &SqlitePool,
    capture: &NewCaptureRecord<'_>,
) -> Result<(), DatabaseError> {
    let file_size = i64::try_from(capture.file_size).map_err(|_| DatabaseError::InvalidFileSize)?;
    let timestamp = capture
        .captured_at_utc
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO captures (
            id,
            device_id,
            display_id,
            captured_at_utc,
            timezone,
            local_path,
            thumbnail_path,
            file_size,
            content_sha256,
            thumbnail_state,
            created_at_utc
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(capture.id.to_string())
    .bind(capture.device_id)
    .bind(capture.display_id)
    .bind(&timestamp)
    .bind(capture.timezone)
    .bind(capture.local_path)
    .bind(capture.thumbnail_path)
    .bind(file_size)
    .bind(capture.content_sha256)
    .bind(capture.thumbnail_state)
    .bind(&timestamp)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn insert_capture_if_missing(
    pool: &SqlitePool,
    capture: &NewCaptureRecord<'_>,
) -> Result<bool, DatabaseError> {
    let file_size = i64::try_from(capture.file_size).map_err(|_| DatabaseError::InvalidFileSize)?;
    let timestamp = capture
        .captured_at_utc
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO captures (
            id,
            device_id,
            display_id,
            captured_at_utc,
            timezone,
            local_path,
            thumbnail_path,
            file_size,
            content_sha256,
            thumbnail_state,
            created_at_utc
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(capture.id.to_string())
    .bind(capture.device_id)
    .bind(capture.display_id)
    .bind(&timestamp)
    .bind(capture.timezone)
    .bind(capture.local_path)
    .bind(capture.thumbnail_path)
    .bind(file_size)
    .bind(capture.content_sha256)
    .bind(capture.thumbnail_state)
    .bind(&timestamp)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn capture_ids(pool: &SqlitePool) -> Result<HashSet<String>, DatabaseError> {
    let ids = sqlx::query_scalar::<_, String>("SELECT id FROM captures")
        .fetch_all(pool)
        .await?;
    Ok(ids.into_iter().collect())
}

#[derive(Debug, FromRow)]
struct CaptureSummaryRow {
    id: String,
    captured_at_utc: DateTime<Utc>,
    file_size: i64,
}

#[derive(Debug, Clone)]
pub struct CaptureSummary {
    pub id: String,
    pub captured_at_utc: DateTime<Utc>,
    pub file_size: u64,
}

pub struct CaptureSummaryPage {
    pub items: Vec<CaptureSummary>,
    pub next_offset: Option<u32>,
}

pub async fn list_capture_summaries(
    pool: &SqlitePool,
    offset: u32,
    requested_limit: Option<u16>,
) -> Result<CaptureSummaryPage, DatabaseError> {
    let limit = requested_limit
        .unwrap_or(18)
        .clamp(1, MAX_TIMELINE_PAGE_SIZE);
    let query_limit = i64::from(limit) + 1;
    let rows = sqlx::query_as::<_, CaptureSummaryRow>(
        r#"
        SELECT id, captured_at_utc, file_size
        FROM captures
        ORDER BY captured_at_utc DESC, id DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(query_limit)
    .bind(i64::from(offset))
    .fetch_all(pool)
    .await?;

    let has_more = rows.len() > usize::from(limit);
    let mut items = Vec::with_capacity(rows.len().min(usize::from(limit)));
    for row in rows.into_iter().take(usize::from(limit)) {
        items.push(CaptureSummary {
            id: row.id,
            captured_at_utc: row.captured_at_utc,
            file_size: u64::try_from(row.file_size).map_err(|_| DatabaseError::InvalidFileSize)?,
        });
    }
    let next_offset = has_more.then_some(offset.saturating_add(items.len() as u32));
    Ok(CaptureSummaryPage { items, next_offset })
}

#[derive(Debug, FromRow)]
struct CaptureFileRow {
    local_path: String,
    thumbnail_path: Option<String>,
    file_size: i64,
    content_sha256: String,
    captured_at_utc: DateTime<Utc>,
}

pub struct CaptureFileRecord {
    pub local_path: String,
    pub thumbnail_path: Option<String>,
    pub file_size: u64,
    pub content_sha256: String,
    pub captured_at_utc: DateTime<Utc>,
}

pub async fn capture_file(
    pool: &SqlitePool,
    capture_id: Uuid,
) -> Result<Option<CaptureFileRecord>, DatabaseError> {
    let row = sqlx::query_as::<_, CaptureFileRow>(
        "SELECT local_path, thumbnail_path, file_size, content_sha256, captured_at_utc FROM captures WHERE id = ?",
    )
    .bind(capture_id.to_string())
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(CaptureFileRecord {
            local_path: row.local_path,
            thumbnail_path: row.thumbnail_path,
            file_size: u64::try_from(row.file_size).map_err(|_| DatabaseError::InvalidFileSize)?,
            content_sha256: row.content_sha256,
            captured_at_utc: row.captured_at_utc,
        })
    })
    .transpose()
}

pub async fn delete_capture(pool: &SqlitePool, capture_id: Uuid) -> Result<(), DatabaseError> {
    let mut transaction = pool.begin().await?;
    let linked_jobs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_job_captures WHERE capture_id = ?")
            .bind(capture_id.to_string())
            .fetch_one(&mut *transaction)
            .await?;
    if linked_jobs > 0 {
        return Err(DatabaseError::CaptureInUse);
    }

    let result = sqlx::query("DELETE FROM captures WHERE id = ?")
        .bind(capture_id.to_string())
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() != 1 {
        return Err(DatabaseError::CaptureNotFound);
    }
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn migrated_memory_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    fn record(id: Uuid, captured_at_utc: DateTime<Utc>) -> NewCaptureRecord<'static> {
        NewCaptureRecord {
            id,
            device_id: "local",
            display_id: "display",
            captured_at_utc,
            timezone: "Asia/Singapore",
            local_path: "captures/test.webp",
            thumbnail_path: None,
            file_size: 10,
            content_sha256: "abc",
            thumbnail_state: "pending",
        }
    }

    #[tokio::test]
    async fn timeline_uses_captured_time_and_stable_pagination() {
        let pool = migrated_memory_pool().await;
        let older_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let newer_id = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
        let mut older = record(
            older_id,
            DateTime::parse_from_rfc3339("2026-07-29T01:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        let mut newer = record(
            newer_id,
            DateTime::parse_from_rfc3339("2026-07-29T02:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        older.local_path = "captures/older.webp";
        newer.local_path = "captures/newer.webp";
        insert_capture(&pool, &older).await.unwrap();
        insert_capture(&pool, &newer).await.unwrap();

        let file = capture_file(&pool, newer_id).await.unwrap().unwrap();
        assert_eq!(file.file_size, 10);
        assert_eq!(file.content_sha256, "abc");

        let first = list_capture_summaries(&pool, 0, Some(1)).await.unwrap();
        assert_eq!(first.items[0].id, newer_id.to_string());
        assert_eq!(first.next_offset, Some(1));
        let second = list_capture_summaries(&pool, 1, Some(1)).await.unwrap();
        assert_eq!(second.items[0].id, older_id.to_string());
        assert_eq!(second.next_offset, None);
    }

    #[tokio::test]
    async fn fresh_schema_has_no_legacy_capture_or_upload_columns() {
        let pool = migrated_memory_pool().await;
        let columns: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM pragma_table_info('captures')")
                .fetch_all(&pool)
                .await
                .unwrap();
        let columns: Vec<String> = columns.into_iter().map(|(name,)| name).collect();

        assert!(!columns.contains(&"cipher_size".to_string()));
        assert!(!columns.contains(&"key_version".to_string()));
        assert!(!columns.contains(&"storage_format".to_string()));

        let upload_tables: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE '%upload%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(upload_tables.0, 0);
    }

    #[tokio::test]
    async fn deletion_rejects_linked_captures_and_verifies_the_removed_record() {
        let pool = migrated_memory_pool().await;
        let capture_id = Uuid::new_v4();
        let captured_at = DateTime::parse_from_rfc3339("2026-07-29T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        insert_capture(&pool, &record(capture_id, captured_at))
            .await
            .unwrap();
        sqlx::query(
            r#"
            INSERT INTO ai_jobs (
                id, provider, model, question, state, created_at_utc, updated_at_utc
            )
            VALUES ('job', 'provider', 'model', 'question', 'completed', ?, ?)
            "#,
        )
        .bind(captured_at)
        .bind(captured_at)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ai_job_captures (job_id, capture_id, ordinal) VALUES ('job', ?, 0)",
        )
        .bind(capture_id.to_string())
        .execute(&pool)
        .await
        .unwrap();

        assert!(matches!(
            delete_capture(&pool, capture_id).await,
            Err(DatabaseError::CaptureInUse)
        ));
        sqlx::query("DELETE FROM ai_job_captures WHERE capture_id = ?")
            .bind(capture_id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        delete_capture(&pool, capture_id).await.unwrap();
        assert!(capture_file(&pool, capture_id).await.unwrap().is_none());
        assert!(matches!(
            delete_capture(&pool, capture_id).await,
            Err(DatabaseError::CaptureNotFound)
        ));
    }
}
