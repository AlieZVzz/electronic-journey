use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};
use thiserror::Error;
use uuid::Uuid;

use crate::database::{self, DatabaseError, NewCaptureRecord};

const MAX_CAPTURE_LENGTH: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineCapture {
    pub id: String,
    pub captured_at_utc: DateTime<Utc>,
    pub file_size: u64,
    pub upload_state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePage {
    pub items: Vec<TimelineCapture>,
    pub next_offset: Option<u32>,
}

#[derive(Debug, Error)]
pub enum TimelineError {
    #[error("application data directory is unavailable")]
    DataDirectory,
    #[error("capture inventory could not be read")]
    Inventory,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

pub async fn list_captures(
    pool: &SqlitePool,
    offset: u32,
    requested_limit: Option<u16>,
) -> Result<TimelinePage, TimelineError> {
    let page = database::list_capture_summaries(pool, offset, requested_limit).await?;
    Ok(TimelinePage {
        items: page
            .items
            .into_iter()
            .map(|capture| TimelineCapture {
                id: capture.id,
                captured_at_utc: capture.captured_at_utc,
                file_size: capture.file_size,
                upload_state: capture.upload_state,
            })
            .collect(),
        next_offset: page.next_offset,
    })
}

pub async fn reconcile_capture_index(
    app: &AppHandle,
    pool: &SqlitePool,
) -> Result<u64, TimelineError> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| TimelineError::DataDirectory)?;
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "Etc/Unknown".into());
    let mut inserted = 0_u64;

    let specification = ScanSpecification {
        directory: data_dir.join("captures"),
        extension: "webp",
        maximum_directory_depth: 3,
    };
    let paths = tauri::async_runtime::spawn_blocking(move || scan_capture_files(&specification))
        .await
        .map_err(|_| TimelineError::Inventory)??;
    let indexed_ids = database::capture_ids(pool).await?;
    for path in unindexed_capture_paths(paths, &indexed_ids) {
        if index_capture_path(pool, &data_dir, &path, &timezone).await? {
            inserted = inserted.saturating_add(1);
        }
    }

    Ok(inserted)
}

fn unindexed_capture_paths(paths: Vec<PathBuf>, indexed_ids: &HashSet<String>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|path| {
            capture_id_from_path(path, "webp")
                .is_some_and(|id| !indexed_ids.contains(&id.to_string()))
        })
        .collect()
}

#[derive(Clone)]
struct ScanSpecification {
    directory: PathBuf,
    extension: &'static str,
    maximum_directory_depth: usize,
}

fn scan_capture_files(specification: &ScanSpecification) -> Result<Vec<PathBuf>, TimelineError> {
    let mut paths = Vec::new();
    scan_directory(
        &specification.directory,
        specification.extension,
        specification.maximum_directory_depth,
        0,
        &mut paths,
    )?;
    Ok(paths)
}

fn scan_directory(
    directory: &Path,
    extension: &str,
    maximum_directory_depth: usize,
    current_depth: usize,
    paths: &mut Vec<PathBuf>,
) -> Result<(), TimelineError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(TimelineError::Inventory),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if current_depth < maximum_directory_depth {
                scan_directory(
                    &path,
                    extension,
                    maximum_directory_depth,
                    current_depth + 1,
                    paths,
                )?;
            }
            continue;
        }
        if metadata.is_file()
            && metadata.len() <= MAX_CAPTURE_LENGTH
            && path.extension().and_then(|value| value.to_str()) == Some(extension)
            && capture_id_from_path(&path, extension).is_some()
        {
            paths.push(path);
        }
    }
    Ok(())
}

async fn index_capture_path(
    pool: &SqlitePool,
    data_dir: &Path,
    path: &Path,
    timezone: &str,
) -> Result<bool, TimelineError> {
    let capture_id = capture_id_from_path(path, "webp").ok_or(TimelineError::Inventory)?;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| TimelineError::Inventory)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_CAPTURE_LENGTH
    {
        return Ok(false);
    }
    let bytes = std::fs::read(path).map_err(|_| TimelineError::Inventory)?;
    if !matches!(image::guess_format(&bytes), Ok(image::ImageFormat::WebP))
        || image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).is_err()
    {
        return Ok(false);
    }
    let relative_path = managed_relative_path(data_dir, path).ok_or(TimelineError::Inventory)?;
    let thumbnail_path = matching_thumbnail(data_dir, &relative_path, capture_id);
    let captured_at_utc = metadata
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(Utc::now);
    let content_sha256 = hex::encode(Sha256::digest(&bytes));
    database::insert_capture_if_missing(
        pool,
        &NewCaptureRecord {
            id: capture_id,
            device_id: "local",
            display_id: "unknown",
            captured_at_utc,
            timezone,
            local_path: &relative_path,
            thumbnail_path: thumbnail_path.as_deref(),
            file_size: metadata.len(),
            content_sha256: &content_sha256,
            thumbnail_state: if thumbnail_path.is_some() {
                "ready"
            } else {
                "pending"
            },
        },
    )
    .await
    .map_err(TimelineError::from)
}

fn matching_thumbnail(data_dir: &Path, local_path: &str, capture_id: Uuid) -> Option<String> {
    let suffix = local_path.strip_prefix("captures/")?;
    let relative = format!("thumbnails/{suffix}");
    let path = data_dir.join(&relative);
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    let expected_filename = format!("{capture_id}.webp");
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_CAPTURE_LENGTH
        || path.file_name().and_then(|value| value.to_str()) != Some(expected_filename.as_str())
    {
        return None;
    }
    Some(relative)
}

fn managed_relative_path(data_dir: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(data_dir).ok()?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return None;
        };
        parts.push(value.to_str()?.to_owned());
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn capture_id_from_path(path: &Path, expected_extension: &str) -> Option<Uuid> {
    if path.extension().and_then(|value| value.to_str()) != Some(expected_extension) {
        return None;
    }
    path.file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| Uuid::parse_str(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_uuid_named_supported_files_are_indexed() {
        let id = Uuid::new_v4();
        assert_eq!(
            capture_id_from_path(&PathBuf::from(format!("{id}.webp")), "webp"),
            Some(id)
        );
        assert!(capture_id_from_path(&PathBuf::from("../secret.webp"), "webp").is_none());
        assert!(capture_id_from_path(&PathBuf::from(format!("{id}.png")), "webp").is_none());
    }

    #[test]
    fn managed_paths_never_escape_the_data_directory() {
        let data_dir = Path::new("/data/electronic-journey");
        assert_eq!(
            managed_relative_path(
                data_dir,
                Path::new("/data/electronic-journey/captures/2026/07/29/id.webp")
            )
            .as_deref(),
            Some("captures/2026/07/29/id.webp")
        );
        assert!(managed_relative_path(data_dir, Path::new("/data/other.webp")).is_none());
    }

    #[test]
    fn recovery_only_validates_files_missing_from_the_index() {
        let indexed_id = Uuid::new_v4();
        let missing_id = Uuid::new_v4();
        let paths = vec![
            PathBuf::from(format!("{indexed_id}.webp")),
            PathBuf::from(format!("{missing_id}.webp")),
        ];
        let indexed_ids = HashSet::from([indexed_id.to_string()]);

        assert_eq!(
            unindexed_capture_paths(paths, &indexed_ids),
            vec![PathBuf::from(format!("{missing_id}.webp"))]
        );
    }
}
