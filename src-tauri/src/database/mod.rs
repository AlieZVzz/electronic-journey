use std::{collections::HashSet, path::Path, time::Duration};

use chrono::{DateTime, Utc};
use sqlx::migrate::MigrateError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, SqlitePool};
use thiserror::Error;
use uuid::Uuid;

const MAX_TIMELINE_PAGE_SIZE: u16 = 50;
const MAX_UPLOAD_BATCH_SIZE: usize = 500;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("database connection failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] MigrateError),
    #[error("database contains an invalid capture size")]
    InvalidFileSize,
    #[error("capture has an upload in progress")]
    CaptureUploadInProgress,
    #[error("capture does not exist")]
    CaptureNotFound,
    #[error("upload selection is invalid")]
    InvalidUploadSelection,
    #[error("another upload batch is already active")]
    UploadAlreadyInProgress,
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
    recover_interrupted_uploads(&pool).await?;
    Ok(pool)
}

async fn recover_interrupted_uploads(pool: &SqlitePool) -> Result<(), DatabaseError> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE upload_items
        SET state = 'failed', last_error_code = 'interrupted', updated_at_utc = ?
        WHERE state IN ('pending', 'uploading')
        "#,
    )
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        UPDATE remote_profiles
        SET last_auto_sync_state = 'partial_failed'
        WHERE last_auto_sync_state = 'running'
        "#,
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        UPDATE upload_batches
        SET
            state = 'partial_failed',
            completed_items = (
                SELECT COUNT(*) FROM upload_items
                WHERE upload_items.batch_id = upload_batches.id
                  AND upload_items.state = 'uploaded'
            ),
            failed_items = (
                SELECT COUNT(*) FROM upload_items
                WHERE upload_items.batch_id = upload_batches.id
                  AND upload_items.state = 'failed'
            ),
            updated_at_utc = ?
        WHERE state IN ('pending', 'uploading')
        "#,
    )
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
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
    pub pixel_sha256: Option<&'a str>,
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
            pixel_sha256,
            thumbnail_state,
            created_at_utc
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(capture.pixel_sha256)
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
            pixel_sha256,
            thumbnail_state,
            created_at_utc
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    .bind(capture.pixel_sha256)
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

pub async fn latest_capture_pixel_sha256(
    pool: &SqlitePool,
    device_id: &str,
    display_id: &str,
) -> Result<Option<Option<String>>, DatabaseError> {
    sqlx::query_scalar(
        r#"
        SELECT pixel_sha256
        FROM captures
        WHERE device_id = ? AND display_id = ?
        ORDER BY captured_at_utc DESC, id DESC
        LIMIT 1
        "#,
    )
    .bind(device_id)
    .bind(display_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn active_upload_count(pool: &SqlitePool) -> Result<u32, DatabaseError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM upload_items WHERE state IN ('pending', 'uploading')",
    )
    .fetch_one(pool)
    .await?;
    u32::try_from(count).map_err(|_| DatabaseError::InvalidUploadSelection)
}

#[derive(Debug, FromRow)]
struct CaptureSummaryRow {
    id: String,
    captured_at_utc: DateTime<Utc>,
    file_size: i64,
    upload_state: String,
}

#[derive(Debug, Clone)]
pub struct CaptureSummary {
    pub id: String,
    pub captured_at_utc: DateTime<Utc>,
    pub file_size: u64,
    pub upload_state: String,
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
        SELECT
            captures.id,
            captures.captured_at_utc,
            captures.file_size,
            COALESCE((
                SELECT upload_items.state
                FROM upload_items
                WHERE upload_items.capture_id = captures.id
                ORDER BY upload_items.created_at_utc DESC, upload_items.id DESC
                LIMIT 1
            ), 'not_uploaded') AS upload_state
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
            upload_state: row.upload_state,
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
    let active_uploads: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM upload_items WHERE capture_id = ? AND state IN ('pending', 'uploading')",
    )
            .bind(capture_id.to_string())
            .fetch_one(&mut *transaction)
            .await?;
    if active_uploads > 0 {
        return Err(DatabaseError::CaptureUploadInProgress);
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

#[derive(Debug, Clone, FromRow)]
pub struct RemoteProfileRecord {
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub private_key_path: String,
    pub host_key_fingerprint: String,
    pub remote_root: String,
    pub has_passphrase: bool,
    pub auto_sync_enabled: bool,
    pub sync_interval_minutes: i64,
    pub next_auto_sync_at_utc: Option<DateTime<Utc>>,
    pub last_auto_sync_attempt_at_utc: Option<DateTime<Utc>>,
    pub last_auto_sync_state: Option<String>,
    pub last_auto_sync_completed_items: i64,
    pub last_auto_sync_failed_items: i64,
    pub auto_sync_suspended_reason: Option<String>,
}

pub struct SaveRemoteProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub host: &'a str,
    pub port: u16,
    pub username: &'a str,
    pub private_key_path: &'a str,
    pub host_key_fingerprint: &'a str,
    pub remote_root: &'a str,
    pub has_passphrase: bool,
}

pub async fn save_remote_profile(
    pool: &SqlitePool,
    profile: &SaveRemoteProfile<'_>,
) -> Result<(), DatabaseError> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query(
        r#"
        INSERT INTO remote_profiles (
            id, name, host, port, username, private_key_path,
            host_key_fingerprint, remote_root, has_passphrase,
            created_at_utc, updated_at_utc
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            host = excluded.host,
            port = excluded.port,
            username = excluded.username,
            private_key_path = excluded.private_key_path,
            host_key_fingerprint = excluded.host_key_fingerprint,
            remote_root = excluded.remote_root,
            has_passphrase = excluded.has_passphrase,
            updated_at_utc = excluded.updated_at_utc
        "#,
    )
    .bind(profile.id)
    .bind(profile.name)
    .bind(profile.host)
    .bind(i64::from(profile.port))
    .bind(profile.username)
    .bind(profile.private_key_path)
    .bind(profile.host_key_fingerprint)
    .bind(profile.remote_root)
    .bind(profile.has_passphrase)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remote_profile(
    pool: &SqlitePool,
    profile_id: &str,
) -> Result<Option<RemoteProfileRecord>, DatabaseError> {
    sqlx::query_as(
        r#"
        SELECT
            name, host, port, username, private_key_path,
            host_key_fingerprint, remote_root, has_passphrase,
            auto_sync_enabled, sync_interval_minutes,
            next_auto_sync_at_utc, last_auto_sync_attempt_at_utc,
            last_auto_sync_state, last_auto_sync_completed_items,
            last_auto_sync_failed_items, auto_sync_suspended_reason
        FROM remote_profiles
        WHERE id = ?
        "#,
    )
    .bind(profile_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn save_auto_sync_settings(
    pool: &SqlitePool,
    profile_id: &str,
    enabled: bool,
    interval_minutes: u16,
    next_run_at: Option<DateTime<Utc>>,
) -> Result<(), DatabaseError> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let next_run_at =
        next_run_at.map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
    sqlx::query(
        r#"
        UPDATE remote_profiles
        SET
            auto_sync_enabled = ?,
            sync_interval_minutes = ?,
            next_auto_sync_at_utc = ?,
            auto_sync_suspended_reason = NULL,
            last_auto_sync_state = CASE
                WHEN ? = 0 THEN NULL
                WHEN last_auto_sync_state = 'suspended' THEN NULL
                ELSE last_auto_sync_state
            END,
            updated_at_utc = ?
        WHERE id = ?
        "#,
    )
    .bind(enabled)
    .bind(i64::from(interval_minutes))
    .bind(next_run_at)
    .bind(enabled)
    .bind(now)
    .bind(profile_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn claim_auto_sync(
    pool: &SqlitePool,
    profile_id: &str,
    now: DateTime<Utc>,
    force: bool,
) -> Result<Option<RemoteProfileRecord>, DatabaseError> {
    let mut transaction = pool.begin().await?;
    let profile = sqlx::query_as::<_, RemoteProfileRecord>(
        r#"
        SELECT
            name, host, port, username, private_key_path,
            host_key_fingerprint, remote_root, has_passphrase,
            auto_sync_enabled, sync_interval_minutes,
            next_auto_sync_at_utc, last_auto_sync_attempt_at_utc,
            last_auto_sync_state, last_auto_sync_completed_items,
            last_auto_sync_failed_items, auto_sync_suspended_reason
        FROM remote_profiles
        WHERE id = ?
          AND auto_sync_enabled = 1
          AND auto_sync_suspended_reason IS NULL
          AND (last_auto_sync_state IS NULL OR last_auto_sync_state <> 'running')
          AND (? = 1 OR next_auto_sync_at_utc <= ?)
        "#,
    )
    .bind(profile_id)
    .bind(force)
    .bind(now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(mut profile) = profile else {
        transaction.commit().await?;
        return Ok(None);
    };
    let interval = u16::try_from(profile.sync_interval_minutes)
        .ok()
        .filter(|value| matches!(value, 15 | 30 | 60 | 120 | 240))
        .ok_or(DatabaseError::InvalidUploadSelection)?;
    let next_run = now + chrono::Duration::minutes(i64::from(interval));
    let now_text = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let next_text = next_run.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let result = sqlx::query(
        r#"
        UPDATE remote_profiles
        SET
            last_auto_sync_attempt_at_utc = ?,
            next_auto_sync_at_utc = ?,
            last_auto_sync_state = 'running',
            last_auto_sync_completed_items = 0,
            last_auto_sync_failed_items = 0
        WHERE id = ?
          AND auto_sync_enabled = 1
          AND auto_sync_suspended_reason IS NULL
          AND (last_auto_sync_state IS NULL OR last_auto_sync_state <> 'running')
          AND (? = 1 OR next_auto_sync_at_utc <= ?)
        "#,
    )
    .bind(&now_text)
    .bind(&next_text)
    .bind(profile_id)
    .bind(force)
    .bind(&now_text)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        transaction.rollback().await?;
        return Ok(None);
    }
    transaction.commit().await?;
    profile.last_auto_sync_attempt_at_utc = Some(now);
    profile.next_auto_sync_at_utc = Some(next_run);
    profile.last_auto_sync_state = Some("running".to_string());
    profile.last_auto_sync_completed_items = 0;
    profile.last_auto_sync_failed_items = 0;
    Ok(Some(profile))
}

pub async fn record_auto_sync_result(
    pool: &SqlitePool,
    profile_id: &str,
    state: &str,
    completed_items: usize,
    failed_items: usize,
    suspended_reason: Option<&str>,
) -> Result<(), DatabaseError> {
    sqlx::query(
        r#"
        UPDATE remote_profiles
        SET
            last_auto_sync_state = ?,
            last_auto_sync_completed_items = ?,
            last_auto_sync_failed_items = ?,
            auto_sync_suspended_reason = ?,
            next_auto_sync_at_utc = CASE WHEN ? IS NULL
                THEN next_auto_sync_at_utc
                ELSE NULL
            END
        WHERE id = ?
        "#,
    )
    .bind(state)
    .bind(i64::try_from(completed_items).map_err(|_| DatabaseError::InvalidUploadSelection)?)
    .bind(i64::try_from(failed_items).map_err(|_| DatabaseError::InvalidUploadSelection)?)
    .bind(suspended_reason)
    .bind(suspended_reason)
    .bind(profile_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn unsynced_capture_ids(
    pool: &SqlitePool,
    start_utc: DateTime<Utc>,
    end_utc: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<Uuid>, DatabaseError> {
    let ids: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT captures.id
        FROM captures
        WHERE captures.captured_at_utc >= ?
          AND captures.captured_at_utc < ?
          AND NOT EXISTS (
              SELECT 1
              FROM upload_items
              WHERE upload_items.capture_id = captures.id
                AND upload_items.state IN ('pending', 'uploading', 'uploaded')
          )
        ORDER BY captures.captured_at_utc, captures.id
        LIMIT ?
        "#,
    )
    .bind(start_utc.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .bind(end_utc.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    .bind(i64::try_from(limit).map_err(|_| DatabaseError::InvalidUploadSelection)?)
    .fetch_all(pool)
    .await?;
    ids.into_iter()
        .map(|value| Uuid::parse_str(&value).map_err(|_| DatabaseError::InvalidUploadSelection))
        .collect()
}

#[derive(Debug, Clone, FromRow)]
pub struct UploadItemRecord {
    pub id: String,
    pub capture_id: String,
    pub remote_path: String,
    pub file_size: i64,
    pub content_sha256: String,
    pub local_path: String,
}

pub struct NewUploadBatch {
    pub id: Uuid,
}

pub async fn create_upload_batch(
    pool: &SqlitePool,
    profile_id: &str,
    capture_ids: &[Uuid],
    source: &str,
) -> Result<NewUploadBatch, DatabaseError> {
    let unique_ids: HashSet<_> = capture_ids.iter().copied().collect();
    if !matches!(source, "manual" | "automatic")
        || unique_ids.is_empty()
        || unique_ids.len() != capture_ids.len()
        || unique_ids.len() > MAX_UPLOAD_BATCH_SIZE
    {
        return Err(DatabaseError::InvalidUploadSelection);
    }

    let batch_id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut transaction = pool.begin().await?;
    let active_batches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM upload_batches WHERE state IN ('pending', 'uploading')",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if active_batches > 0 {
        return Err(DatabaseError::UploadAlreadyInProgress);
    }
    let mut total_bytes = 0_u64;
    let mut captures = Vec::with_capacity(capture_ids.len());
    for capture_id in capture_ids {
        let row: Option<(DateTime<Utc>, i64, String)> = sqlx::query_as(
            "SELECT captured_at_utc, file_size, content_sha256 FROM captures WHERE id = ?",
        )
        .bind(capture_id.to_string())
        .fetch_optional(&mut *transaction)
        .await?;
        let Some((captured_at_utc, file_size, content_sha256)) = row else {
            return Err(DatabaseError::CaptureNotFound);
        };
        let file_size = u64::try_from(file_size).map_err(|_| DatabaseError::InvalidFileSize)?;
        total_bytes = total_bytes.saturating_add(file_size);
        captures.push((capture_id, captured_at_utc, file_size, content_sha256));
    }

    sqlx::query(
        r#"
        INSERT INTO upload_batches (
            id, profile_id, state, total_items, total_bytes,
            completed_items, failed_items, created_at_utc, updated_at_utc,
            source
        )
        VALUES (?, ?, 'pending', ?, ?, 0, 0, ?, ?, ?)
        "#,
    )
    .bind(batch_id.to_string())
    .bind(profile_id)
    .bind(i64::try_from(captures.len()).map_err(|_| DatabaseError::InvalidUploadSelection)?)
    .bind(i64::try_from(total_bytes).map_err(|_| DatabaseError::InvalidFileSize)?)
    .bind(&now)
    .bind(&now)
    .bind(source)
    .execute(&mut *transaction)
    .await?;

    for (capture_id, captured_at_utc, file_size, content_sha256) in captures {
        let date_path = captured_at_utc.format("%Y/%m/%d");
        let timestamp = captured_at_utc.format("%Y%m%dT%H%M%S%3fZ");
        let remote_path = format!("{date_path}/{timestamp}_{capture_id}.webp");
        sqlx::query(
            r#"
            INSERT INTO upload_items (
                id, batch_id, capture_id, remote_path, file_size,
                content_sha256, state, attempt_count,
                created_at_utc, updated_at_utc
            )
            VALUES (?, ?, ?, ?, ?, ?, 'pending', 0, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(batch_id.to_string())
        .bind(capture_id.to_string())
        .bind(remote_path)
        .bind(i64::try_from(file_size).map_err(|_| DatabaseError::InvalidFileSize)?)
        .bind(content_sha256)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(NewUploadBatch { id: batch_id })
}

pub async fn upload_batch_items(
    pool: &SqlitePool,
    batch_id: Uuid,
) -> Result<Vec<UploadItemRecord>, DatabaseError> {
    sqlx::query_as(
        r#"
        SELECT
            upload_items.id,
            upload_items.capture_id,
            upload_items.remote_path,
            upload_items.file_size,
            upload_items.content_sha256,
            captures.local_path
        FROM upload_items
        JOIN captures ON captures.id = upload_items.capture_id
        WHERE upload_items.batch_id = ?
        ORDER BY upload_items.created_at_utc, upload_items.id
        "#,
    )
    .bind(batch_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn start_upload_batch(pool: &SqlitePool, batch_id: Uuid) -> Result<(), DatabaseError> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query("UPDATE upload_batches SET state = 'uploading', updated_at_utc = ? WHERE id = ?")
        .bind(now)
        .bind(batch_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_upload_item_state(
    pool: &SqlitePool,
    item_id: &str,
    state: &str,
    error_code: Option<&str>,
) -> Result<(), DatabaseError> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query(
        r#"
        UPDATE upload_items
        SET
            state = ?,
            attempt_count = attempt_count + CASE WHEN ? = 'uploading' THEN 1 ELSE 0 END,
            last_error_code = ?,
            updated_at_utc = ?
        WHERE id = ?
        "#,
    )
    .bind(state)
    .bind(state)
    .bind(error_code)
    .bind(now)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_upload_batch(
    pool: &SqlitePool,
    batch_id: Uuid,
    completed_items: usize,
    failed_items: usize,
) -> Result<(), DatabaseError> {
    let state = if failed_items == 0 {
        "completed"
    } else {
        "partial_failed"
    };
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    sqlx::query(
        r#"
        UPDATE upload_batches
        SET
            state = ?,
            completed_items = ?,
            failed_items = ?,
            updated_at_utc = ?
        WHERE id = ?
        "#,
    )
    .bind(state)
    .bind(i64::try_from(completed_items).map_err(|_| DatabaseError::InvalidUploadSelection)?)
    .bind(i64::try_from(failed_items).map_err(|_| DatabaseError::InvalidUploadSelection)?)
    .bind(now)
    .bind(batch_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow)]
pub struct UploadBatchStatusRecord {
    pub id: String,
    pub state: String,
    pub total_items: i64,
    pub total_bytes: i64,
    pub completed_items: i64,
    pub failed_items: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct UploadItemStatusRecord {
    pub capture_id: String,
    pub state: String,
    pub last_error_code: Option<String>,
}

pub struct UploadBatchStatus {
    pub batch: UploadBatchStatusRecord,
    pub items: Vec<UploadItemStatusRecord>,
}

pub async fn upload_batch_status(
    pool: &SqlitePool,
    batch_id: Uuid,
) -> Result<Option<UploadBatchStatus>, DatabaseError> {
    let batch = sqlx::query_as::<_, UploadBatchStatusRecord>(
        r#"
        SELECT
            id,
            state,
            total_items,
            total_bytes,
            (
                SELECT COUNT(*) FROM upload_items
                WHERE upload_items.batch_id = upload_batches.id
                  AND upload_items.state = 'uploaded'
            ) AS completed_items,
            (
                SELECT COUNT(*) FROM upload_items
                WHERE upload_items.batch_id = upload_batches.id
                  AND upload_items.state = 'failed'
            ) AS failed_items
        FROM upload_batches
        WHERE id = ?
        "#,
    )
    .bind(batch_id.to_string())
    .fetch_optional(pool)
    .await?;
    let Some(batch) = batch else {
        return Ok(None);
    };
    let items = sqlx::query_as::<_, UploadItemStatusRecord>(
        r#"
        SELECT capture_id, state, last_error_code
        FROM upload_items
        WHERE batch_id = ?
        ORDER BY created_at_utc, id
        "#,
    )
    .bind(batch_id.to_string())
    .fetch_all(pool)
    .await?;
    Ok(Some(UploadBatchStatus { batch, items }))
}

pub async fn active_upload_batch_id(pool: &SqlitePool) -> Result<Option<Uuid>, DatabaseError> {
    let id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM upload_batches
        WHERE state IN ('pending', 'uploading')
        ORDER BY created_at_utc DESC, id DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;
    id.map(|value| Uuid::parse_str(&value).map_err(|_| DatabaseError::InvalidUploadSelection))
        .transpose()
}

pub async fn fail_active_upload_batch(
    pool: &SqlitePool,
    batch_id: Uuid,
    error_code: &str,
) -> Result<(), DatabaseError> {
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE upload_items
        SET state = 'failed', last_error_code = ?, updated_at_utc = ?
        WHERE batch_id = ? AND state IN ('pending', 'uploading')
        "#,
    )
    .bind(error_code)
    .bind(&now)
    .bind(batch_id.to_string())
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        UPDATE upload_batches
        SET
            state = 'partial_failed',
            completed_items = (
                SELECT COUNT(*) FROM upload_items
                WHERE upload_items.batch_id = upload_batches.id
                  AND upload_items.state = 'uploaded'
            ),
            failed_items = (
                SELECT COUNT(*) FROM upload_items
                WHERE upload_items.batch_id = upload_batches.id
                  AND upload_items.state = 'failed'
            ),
            updated_at_utc = ?
        WHERE id = ? AND state IN ('pending', 'uploading')
        "#,
    )
    .bind(&now)
    .bind(batch_id.to_string())
    .execute(&mut *transaction)
    .await?;
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
            pixel_sha256: Some("pixels"),
            thumbnail_state: "pending",
        }
    }

    async fn save_test_remote_profile(pool: &SqlitePool) {
        save_remote_profile(
            pool,
            &SaveRemoteProfile {
                id: "primary",
                name: "Personal server",
                host: "example.test",
                port: 22,
                username: "journey",
                private_key_path: "/Users/test/.ssh/id_ed25519",
                host_key_fingerprint: "SHA256:test",
                remote_root: "/srv/journey",
                has_passphrase: false,
            },
        )
        .await
        .unwrap();
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
    async fn latest_pixel_fingerprint_is_scoped_to_the_display() {
        let pool = migrated_memory_pool().await;
        let captured_at = DateTime::parse_from_rfc3339("2026-07-29T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first_id = Uuid::new_v4();
        let mut first = record(first_id, captured_at);
        first.local_path = "captures/first.webp";
        first.pixel_sha256 = Some("first-pixels");
        insert_capture(&pool, &first).await.unwrap();

        let second_id = Uuid::new_v4();
        let mut second = record(second_id, captured_at + chrono::Duration::seconds(1));
        second.local_path = "captures/second.webp";
        second.display_id = "second-display";
        second.pixel_sha256 = Some("second-pixels");
        insert_capture(&pool, &second).await.unwrap();

        assert_eq!(
            latest_capture_pixel_sha256(&pool, "local", "display")
                .await
                .unwrap(),
            Some(Some("first-pixels".into()))
        );
        assert_eq!(
            latest_capture_pixel_sha256(&pool, "local", "second-display")
                .await
                .unwrap(),
            Some(Some("second-pixels".into()))
        );
        assert_eq!(
            latest_capture_pixel_sha256(&pool, "local", "missing-display")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn fresh_schema_replaces_ai_jobs_with_upload_queue() {
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
        assert!(columns.contains(&"pixel_sha256".to_string()));

        let upload_tables: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE '%upload%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(upload_tables.0, 2);
        let ai_tables: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'ai_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ai_tables.0, 0);
    }

    #[tokio::test]
    async fn automatic_sync_is_opt_in_and_a_due_run_is_claimed_once() {
        let pool = migrated_memory_pool().await;
        save_test_remote_profile(&pool).await;
        let now = Utc::now();

        let stored = remote_profile(&pool, "primary").await.unwrap().unwrap();
        assert!(!stored.auto_sync_enabled);
        assert_eq!(stored.sync_interval_minutes, 30);
        assert!(claim_auto_sync(&pool, "primary", now, false)
            .await
            .unwrap()
            .is_none());

        save_auto_sync_settings(
            &pool,
            "primary",
            true,
            30,
            Some(now - chrono::Duration::seconds(1)),
        )
        .await
        .unwrap();
        let claimed = claim_auto_sync(&pool, "primary", now, false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.last_auto_sync_state.as_deref(), Some("running"));
        assert!(claimed.next_auto_sync_at_utc.unwrap() > now);
        assert!(claim_auto_sync(&pool, "primary", now, true)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn automatic_sync_selects_only_unsynced_captures_in_the_day() {
        let pool = migrated_memory_pool().await;
        save_test_remote_profile(&pool).await;
        let start = DateTime::parse_from_rfc3339("2026-07-29T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let uploaded_id = Uuid::new_v4();
        let pending_id = Uuid::new_v4();
        let previous_day_id = Uuid::new_v4();
        let mut uploaded = record(uploaded_id, start + chrono::Duration::hours(1));
        uploaded.local_path = "captures/uploaded.webp";
        let mut pending = record(pending_id, start + chrono::Duration::hours(2));
        pending.local_path = "captures/pending.webp";
        let mut previous = record(previous_day_id, start - chrono::Duration::seconds(1));
        previous.local_path = "captures/previous.webp";
        insert_capture(&pool, &uploaded).await.unwrap();
        insert_capture(&pool, &pending).await.unwrap();
        insert_capture(&pool, &previous).await.unwrap();
        let batch = create_upload_batch(&pool, "primary", &[uploaded_id], "manual")
            .await
            .unwrap();
        let item = upload_batch_items(&pool, batch.id).await.unwrap().remove(0);
        set_upload_item_state(&pool, &item.id, "uploaded", None)
            .await
            .unwrap();
        finish_upload_batch(&pool, batch.id, 1, 0).await.unwrap();

        let selected = unsynced_capture_ids(&pool, start, start + chrono::Duration::days(1), 500)
            .await
            .unwrap();
        assert_eq!(selected, vec![pending_id]);
    }

    #[tokio::test]
    async fn deletion_waits_for_active_upload_but_keeps_finished_history() {
        let pool = migrated_memory_pool().await;
        let capture_id = Uuid::new_v4();
        let captured_at = DateTime::parse_from_rfc3339("2026-07-29T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        insert_capture(&pool, &record(capture_id, captured_at))
            .await
            .unwrap();
        save_remote_profile(
            &pool,
            &SaveRemoteProfile {
                id: "primary",
                name: "Personal server",
                host: "example.test",
                port: 22,
                username: "journey",
                private_key_path: "/Users/test/.ssh/id_ed25519",
                host_key_fingerprint: "SHA256:test",
                remote_root: "/srv/journey",
                has_passphrase: false,
            },
        )
        .await
        .unwrap();
        let batch = create_upload_batch(&pool, "primary", &[capture_id], "manual")
            .await
            .unwrap();
        let item = upload_batch_items(&pool, batch.id).await.unwrap().remove(0);

        assert!(matches!(
            delete_capture(&pool, capture_id).await,
            Err(DatabaseError::CaptureUploadInProgress)
        ));
        set_upload_item_state(&pool, &item.id, "uploaded", None)
            .await
            .unwrap();
        finish_upload_batch(&pool, batch.id, 1, 0).await.unwrap();
        delete_capture(&pool, capture_id).await.unwrap();
        assert!(capture_file(&pool, capture_id).await.unwrap().is_none());
        let history: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM upload_items WHERE capture_id = ?")
                .bind(capture_id.to_string())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(history.0, 1);
        assert!(matches!(
            delete_capture(&pool, capture_id).await,
            Err(DatabaseError::CaptureNotFound)
        ));
    }

    #[tokio::test]
    async fn upload_batch_has_stable_date_paths_and_rejects_duplicates() {
        let pool = migrated_memory_pool().await;
        let capture_id = Uuid::new_v4();
        let captured_at = DateTime::parse_from_rfc3339("2026-07-29T02:03:04.567Z")
            .unwrap()
            .with_timezone(&Utc);
        insert_capture(&pool, &record(capture_id, captured_at))
            .await
            .unwrap();
        save_remote_profile(
            &pool,
            &SaveRemoteProfile {
                id: "primary",
                name: "Personal server",
                host: "example.test",
                port: 22,
                username: "journey",
                private_key_path: "/Users/test/.ssh/id_ed25519",
                host_key_fingerprint: "SHA256:test",
                remote_root: "/srv/journey",
                has_passphrase: false,
            },
        )
        .await
        .unwrap();

        let batch = create_upload_batch(&pool, "primary", &[capture_id], "manual")
            .await
            .unwrap();
        let item = upload_batch_items(&pool, batch.id).await.unwrap().remove(0);
        assert_eq!(
            item.remote_path,
            format!("2026/07/29/20260729T020304567Z_{capture_id}.webp")
        );
        assert!(matches!(
            create_upload_batch(&pool, "primary", &[capture_id, capture_id], "manual",).await,
            Err(DatabaseError::InvalidUploadSelection)
        ));
    }

    #[tokio::test]
    async fn upload_queue_allows_only_one_active_batch_and_reports_live_progress() {
        let pool = migrated_memory_pool().await;
        let first_capture_id = Uuid::new_v4();
        let second_capture_id = Uuid::new_v4();
        let captured_at = DateTime::parse_from_rfc3339("2026-07-29T02:03:04Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut first_capture = record(first_capture_id, captured_at);
        first_capture.local_path = "captures/first.webp";
        let mut second_capture = record(second_capture_id, captured_at);
        second_capture.local_path = "captures/second.webp";
        insert_capture(&pool, &first_capture).await.unwrap();
        insert_capture(&pool, &second_capture).await.unwrap();
        save_remote_profile(
            &pool,
            &SaveRemoteProfile {
                id: "primary",
                name: "Personal server",
                host: "example.test",
                port: 22,
                username: "journey",
                private_key_path: "/Users/test/.ssh/id_ed25519",
                host_key_fingerprint: "SHA256:test",
                remote_root: "/srv/journey",
                has_passphrase: false,
            },
        )
        .await
        .unwrap();

        let batch = create_upload_batch(
            &pool,
            "primary",
            &[first_capture_id, second_capture_id],
            "manual",
        )
        .await
        .unwrap();
        assert_eq!(active_upload_batch_id(&pool).await.unwrap(), Some(batch.id));
        assert!(matches!(
            create_upload_batch(&pool, "primary", &[first_capture_id], "manual").await,
            Err(DatabaseError::UploadAlreadyInProgress)
        ));

        start_upload_batch(&pool, batch.id).await.unwrap();
        let items = upload_batch_items(&pool, batch.id).await.unwrap();
        set_upload_item_state(&pool, &items[0].id, "uploading", None)
            .await
            .unwrap();
        set_upload_item_state(&pool, &items[0].id, "uploaded", None)
            .await
            .unwrap();
        let progress = upload_batch_status(&pool, batch.id).await.unwrap().unwrap();
        assert_eq!(progress.batch.state, "uploading");
        assert_eq!(progress.batch.completed_items, 1);
        assert_eq!(progress.batch.failed_items, 0);

        fail_active_upload_batch(&pool, batch.id, "connection")
            .await
            .unwrap();
        let progress = upload_batch_status(&pool, batch.id).await.unwrap().unwrap();
        assert_eq!(progress.batch.state, "partial_failed");
        assert_eq!(progress.batch.completed_items, 1);
        assert_eq!(progress.batch.failed_items, 1);
        assert_eq!(
            progress.items[1].last_error_code.as_deref(),
            Some("connection")
        );
        assert_eq!(active_upload_batch_id(&pool).await.unwrap(), None);

        create_upload_batch(&pool, "primary", &[first_capture_id], "manual")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn startup_recovery_fails_active_items_without_replaying_network_work() {
        let pool = migrated_memory_pool().await;
        let capture_id = Uuid::new_v4();
        let captured_at = DateTime::parse_from_rfc3339("2026-07-29T02:03:04Z")
            .unwrap()
            .with_timezone(&Utc);
        insert_capture(&pool, &record(capture_id, captured_at))
            .await
            .unwrap();
        save_remote_profile(
            &pool,
            &SaveRemoteProfile {
                id: "primary",
                name: "Personal server",
                host: "example.test",
                port: 22,
                username: "journey",
                private_key_path: "/Users/test/.ssh/id_ed25519",
                host_key_fingerprint: "SHA256:test",
                remote_root: "/srv/journey",
                has_passphrase: false,
            },
        )
        .await
        .unwrap();
        let batch = create_upload_batch(&pool, "primary", &[capture_id], "manual")
            .await
            .unwrap();
        start_upload_batch(&pool, batch.id).await.unwrap();
        let item = upload_batch_items(&pool, batch.id).await.unwrap().remove(0);
        set_upload_item_state(&pool, &item.id, "uploading", None)
            .await
            .unwrap();

        recover_interrupted_uploads(&pool).await.unwrap();

        let item_state: (String, Option<String>) =
            sqlx::query_as("SELECT state, last_error_code FROM upload_items WHERE id = ?")
                .bind(item.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(item_state.0, "failed");
        assert_eq!(item_state.1.as_deref(), Some("interrupted"));
        let batch_state: (String, i64) =
            sqlx::query_as("SELECT state, failed_items FROM upload_batches WHERE id = ?")
                .bind(batch.id.to_string())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(batch_state, ("partial_failed".to_string(), 1));
        delete_capture(&pool, capture_id).await.unwrap();
    }
}
