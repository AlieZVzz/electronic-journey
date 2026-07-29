use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Local, Utc};
use image::{imageops::FilterType, DynamicImage, ImageBuffer, ImageEncoder, Rgba};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    capture::CapturedImage,
    database::{self, CaptureFileRecord, NewCaptureRecord},
    vault,
};

const MAXIMUM_CAPTURE_LENGTH: u64 = 128 * 1024 * 1024;
const THUMBNAIL_MAX_WIDTH: u32 = 1440;

#[derive(Debug)]
pub struct StoredCapture {
    pub storage_size: u64,
}

pub struct DeletedCapture {
    pub storage_size: u64,
    pub captured_at_utc: DateTime<Utc>,
}

struct StagedDeletion {
    original_path: PathBuf,
    staged_path: PathBuf,
    size: u64,
}

#[derive(Debug, Error)]
pub enum CapturePipelineError {
    #[error("captured pixels were invalid")]
    InvalidPixels,
    #[error("image encoding failed")]
    ImageEncoding,
    #[error("application data directory is unavailable")]
    DataDirectory,
    #[error("capture could not be written")]
    WriteFailed,
    #[error("capture could not be read")]
    ReadFailed,
    #[error("capture file is invalid")]
    InvalidContainer,
    #[error("capture index could not be updated")]
    Database,
    #[error("capture has an upload in progress")]
    CaptureUploadInProgress,
    #[error("capture does not exist")]
    CaptureNotFound,
    #[error("capture files could not be deleted")]
    DeleteFailed,
    #[error("capture deletion could not be fully verified")]
    DeleteIncomplete,
}

pub async fn persist_capture(
    app: &AppHandle,
    pool: &SqlitePool,
    captured: CapturedImage,
    display_id: &str,
    captured_at_utc: DateTime<Utc>,
    timezone: &str,
) -> Result<StoredCapture, CapturePipelineError> {
    let (original, thumbnail) = encode_webp_variants(captured)?;
    let capture_id = Uuid::new_v4();
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?;
    let local_date = captured_at_utc.with_timezone(&Local);
    let date_path = local_date.format("%Y/%m/%d");
    let original_relative = format!("captures/{date_path}/{capture_id}.webp");
    let thumbnail_relative = format!("thumbnails/{date_path}/{capture_id}.webp");
    let original_path = data_dir.join(&original_relative);
    let thumbnail_path = data_dir.join(&thumbnail_relative);

    vault::write_atomic(&original_path, &original)
        .await
        .map_err(|_| CapturePipelineError::WriteFailed)?;
    let (thumbnail_size, stored_thumbnail_path, thumbnail_state) =
        match vault::write_atomic(&thumbnail_path, &thumbnail).await {
            Ok(()) => (
                thumbnail.len() as u64,
                Some(thumbnail_relative.as_str()),
                "ready",
            ),
            Err(_) => {
                // The original is already durable and remains a valid capture.
                // Timeline reads can derive a temporary thumbnail from it.
                tracing::warn!(
                    error_code = "thumbnail_write_failed",
                    capture_id = %capture_id,
                    "thumbnail could not be persisted"
                );
                (0, None, "failed")
            }
        };

    // Read and decode the durable original before reporting success. This
    // keeps a partial or corrupt local write from becoming a successful item.
    let verified = read_validated_webp(&original_path)?;
    let content_sha256 = hex::encode(Sha256::digest(&verified));
    database::insert_capture(
        pool,
        &NewCaptureRecord {
            id: capture_id,
            device_id: "local",
            display_id,
            captured_at_utc,
            timezone,
            local_path: &original_relative,
            thumbnail_path: stored_thumbnail_path,
            file_size: original.len() as u64,
            content_sha256: &content_sha256,
            thumbnail_state,
        },
    )
    .await
    .map_err(|_| CapturePipelineError::Database)?;

    Ok(StoredCapture {
        storage_size: original.len() as u64 + thumbnail_size,
    })
}

pub fn read_saved_capture(
    app: &AppHandle,
    capture_id: Uuid,
    record: &CaptureFileRecord,
) -> Result<Vec<u8>, CapturePipelineError> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?;
    let path = resolve_capture_path(
        &data_dir,
        &record.local_path,
        capture_id,
        "webp",
        &["captures"],
    )?;
    read_integrity_checked_webp(&path, record.file_size, &record.content_sha256)
}

pub fn read_saved_thumbnail(
    app: &AppHandle,
    capture_id: Uuid,
    record: &CaptureFileRecord,
) -> Result<Vec<u8>, CapturePipelineError> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?;
    if let Some(relative_path) = &record.thumbnail_path {
        let thumbnail_path = resolve_capture_path(
            &data_dir,
            relative_path,
            capture_id,
            "webp",
            &["thumbnails"],
        )?;
        match read_webp_container(&thumbnail_path) {
            Ok(bytes) => return Ok(bytes),
            Err(CapturePipelineError::ReadFailed) if !thumbnail_path.exists() => {}
            Err(error) => return Err(error),
        }
    }

    let original = read_saved_capture(app, capture_id, record)?;
    thumbnail_from_webp(&original)
}

pub async fn delete_saved_capture(
    app: &AppHandle,
    pool: &SqlitePool,
    capture_id: Uuid,
    record: &CaptureFileRecord,
) -> Result<DeletedCapture, CapturePipelineError> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?;
    let original_path = resolve_capture_path(
        &data_dir,
        &record.local_path,
        capture_id,
        "webp",
        &["captures"],
    )?;
    let thumbnail_path = record
        .thumbnail_path
        .as_deref()
        .map(|path| resolve_capture_path(&data_dir, path, capture_id, "webp", &["thumbnails"]))
        .transpose()?;

    let original = stage_file_for_deletion(&original_path, capture_id, true)
        .await?
        .ok_or(CapturePipelineError::DeleteFailed)?;
    let mut staged = vec![original];
    if let Some(path) = thumbnail_path.as_deref() {
        match stage_file_for_deletion(path, capture_id, false).await {
            Ok(Some(file)) => staged.push(file),
            Ok(None) => {}
            Err(error) => {
                rollback_staged_files(&staged).await?;
                return Err(error);
            }
        }
    }

    if let Err(error) = database::delete_capture(pool, capture_id).await {
        rollback_staged_files(&staged).await?;
        return Err(match error {
            database::DatabaseError::CaptureUploadInProgress => {
                CapturePipelineError::CaptureUploadInProgress
            }
            database::DatabaseError::CaptureNotFound => CapturePipelineError::CaptureNotFound,
            _ => CapturePipelineError::Database,
        });
    }

    let storage_size = staged.iter().map(|file| file.size).sum();
    remove_staged_files(&staged).await?;
    let record_absent = database::capture_file(pool, capture_id)
        .await
        .map_err(|_| CapturePipelineError::DeleteIncomplete)?
        .is_none();
    let files_absent = staged
        .iter()
        .all(|file| !file.original_path.exists() && !file.staged_path.exists());
    if !record_absent || !files_absent {
        return Err(CapturePipelineError::DeleteIncomplete);
    }

    Ok(DeletedCapture {
        storage_size,
        captured_at_utc: record.captured_at_utc,
    })
}

async fn stage_file_for_deletion(
    path: &Path,
    capture_id: Uuid,
    required: bool,
) -> Result<Option<StagedDeletion>, CapturePipelineError> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => return Ok(None),
        Err(_) => return Err(CapturePipelineError::DeleteFailed),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAXIMUM_CAPTURE_LENGTH
    {
        return Err(CapturePipelineError::InvalidContainer);
    }
    let parent = path
        .parent()
        .ok_or(CapturePipelineError::InvalidContainer)?;
    let staged_path = parent.join(format!(".{capture_id}.deleting"));
    if staged_path.exists() {
        return Err(CapturePipelineError::DeleteFailed);
    }
    tokio::fs::rename(path, &staged_path)
        .await
        .map_err(|_| CapturePipelineError::DeleteFailed)?;
    Ok(Some(StagedDeletion {
        original_path: path.to_path_buf(),
        staged_path,
        size: metadata.len(),
    }))
}

async fn remove_staged_files(staged: &[StagedDeletion]) -> Result<(), CapturePipelineError> {
    let mut removal_failed = false;
    for file in staged {
        if let Err(error) = tokio::fs::remove_file(&file.staged_path).await {
            if error.kind() != std::io::ErrorKind::NotFound {
                removal_failed = true;
            }
        }
    }
    if removal_failed {
        Err(CapturePipelineError::DeleteIncomplete)
    } else {
        Ok(())
    }
}

async fn rollback_staged_files(staged: &[StagedDeletion]) -> Result<(), CapturePipelineError> {
    let mut rollback_failed = false;
    for file in staged.iter().rev() {
        if tokio::fs::rename(&file.staged_path, &file.original_path)
            .await
            .is_err()
        {
            rollback_failed = true;
        }
    }
    if rollback_failed {
        Err(CapturePipelineError::DeleteIncomplete)
    } else {
        Ok(())
    }
}

fn resolve_capture_path(
    data_dir: &Path,
    relative_path: &str,
    capture_id: Uuid,
    expected_extension: &str,
    allowed_prefixes: &[&str],
) -> Result<PathBuf, CapturePipelineError> {
    let relative = Path::new(relative_path);
    let expected_filename = format!("{capture_id}.{expected_extension}");
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || !allowed_prefixes
            .iter()
            .any(|prefix| relative.starts_with(Path::new(prefix)))
        || relative.file_name().and_then(|value| value.to_str()) != Some(expected_filename.as_str())
    {
        return Err(CapturePipelineError::InvalidContainer);
    }
    Ok(data_dir.join(relative))
}

fn read_validated_webp(path: &Path) -> Result<Vec<u8>, CapturePipelineError> {
    let container = read_webp_container(path)?;
    if image::load_from_memory_with_format(&container, image::ImageFormat::WebP).is_err() {
        return Err(CapturePipelineError::InvalidContainer);
    }
    Ok(container)
}

fn read_webp_container(path: &Path) -> Result<Vec<u8>, CapturePipelineError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| CapturePipelineError::ReadFailed)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAXIMUM_CAPTURE_LENGTH
    {
        return Err(CapturePipelineError::InvalidContainer);
    }

    let container = std::fs::read(path).map_err(|_| CapturePipelineError::ReadFailed)?;
    if container.len() as u64 > MAXIMUM_CAPTURE_LENGTH
        || !matches!(
            image::guess_format(&container),
            Ok(image::ImageFormat::WebP)
        )
    {
        return Err(CapturePipelineError::InvalidContainer);
    }
    Ok(container)
}

fn read_integrity_checked_webp(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<Vec<u8>, CapturePipelineError> {
    let container = read_webp_container(path)?;
    if container.len() as u64 != expected_size
        || hex::encode(Sha256::digest(&container)) != expected_sha256
    {
        return Err(CapturePipelineError::InvalidContainer);
    }
    Ok(container)
}

pub fn capture_inventory(app: &AppHandle) -> Result<(u32, u64), CapturePipelineError> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| CapturePipelineError::DataDirectory)?;

    let mut count = 0_u32;
    let mut bytes = 0_u64;
    for (directory, extension, maximum_depth, count_as_capture) in [
        (data_dir.join("captures"), "webp", 3, true),
        (data_dir.join("thumbnails"), "webp", 3, false),
    ] {
        for path in inventory_files(&directory, extension, maximum_depth)? {
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            bytes = bytes.saturating_add(metadata.len());
            if !count_as_capture {
                continue;
            }
            let Some(_id) = path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|value| Uuid::parse_str(value).ok())
            else {
                continue;
            };
            if metadata
                .modified()
                .ok()
                .map(DateTime::<Local>::from)
                .is_some_and(|modified| modified.date_naive() == Local::now().date_naive())
            {
                count = count.saturating_add(1);
            }
        }
    }
    Ok((count, bytes))
}

fn inventory_files(
    directory: &Path,
    extension: &str,
    maximum_depth: usize,
) -> Result<Vec<PathBuf>, CapturePipelineError> {
    let mut paths = Vec::new();
    inventory_directory(directory, extension, maximum_depth, 0, &mut paths)?;
    Ok(paths)
}

fn inventory_directory(
    directory: &Path,
    extension: &str,
    maximum_depth: usize,
    current_depth: usize,
    paths: &mut Vec<PathBuf>,
) -> Result<(), CapturePipelineError> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(CapturePipelineError::ReadFailed),
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
            if current_depth < maximum_depth {
                inventory_directory(&path, extension, maximum_depth, current_depth + 1, paths)?;
            }
        } else if metadata.is_file()
            && path.extension().and_then(|value| value.to_str()) == Some(extension)
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn encode_webp_variants(
    captured: CapturedImage,
) -> Result<(Vec<u8>, Vec<u8>), CapturePipelineError> {
    let mut image =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(captured.width, captured.height, captured.rgba)
            .ok_or(CapturePipelineError::InvalidPixels)?;
    for pixel in image.pixels_mut() {
        pixel.0[3] = u8::MAX;
    }
    let original = DynamicImage::ImageRgba8(image);
    let thumbnail = resize_to_max_width(original.clone(), THUMBNAIL_MAX_WIDTH);
    Ok((
        encode_dynamic_webp(&original)?,
        encode_dynamic_webp(&thumbnail)?,
    ))
}

fn encode_dynamic_webp(image: &DynamicImage) -> Result<Vec<u8>, CapturePipelineError> {
    let rgba = image.to_rgba8();
    let mut encoded = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| CapturePipelineError::ImageEncoding)?;
    Ok(encoded)
}

fn resize_to_max_width(image: DynamicImage, max_width: u32) -> DynamicImage {
    if image.width() <= max_width {
        return image;
    }
    let height = ((image.height() as u64 * max_width as u64) / image.width() as u64) as u32;
    image.resize_exact(max_width, height.max(1), FilterType::Lanczos3)
}

fn thumbnail_from_webp(encoded: &[u8]) -> Result<Vec<u8>, CapturePipelineError> {
    let image = image::load_from_memory_with_format(encoded, image::ImageFormat::WebP)
        .map_err(|_| CapturePipelineError::InvalidContainer)?;
    encode_dynamic_webp(&resize_to_max_width(image, THUMBNAIL_MAX_WIDTH))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_capture(width: u32, height: u32) -> CapturedImage {
        CapturedImage {
            width,
            height,
            rgba: vec![255; width as usize * height as usize * 4],
        }
    }

    fn temporary_test_directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("electronic-journey-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn encoding_rejects_an_invalid_pixel_buffer() {
        let captured = CapturedImage {
            width: 2,
            height: 2,
            rgba: vec![0; 15],
        };

        assert!(matches!(
            encode_webp_variants(captured),
            Err(CapturePipelineError::InvalidPixels)
        ));
    }

    #[test]
    fn encoding_preserves_the_original_dimensions() {
        let captured = CapturedImage {
            width: 4,
            height: 2,
            rgba: vec![255; 4 * 2 * 4],
        };
        let (encoded, thumbnail) = encode_webp_variants(captured).unwrap();
        let decoded =
            image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP).unwrap();
        let decoded_thumbnail =
            image::load_from_memory_with_format(&thumbnail, image::ImageFormat::WebP).unwrap();

        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 2);
        assert_eq!(decoded_thumbnail.width(), 4);
        assert_eq!(decoded_thumbnail.height(), 2);
    }

    #[test]
    fn encoding_makes_screen_captures_fully_opaque() {
        let captured = CapturedImage {
            width: 2,
            height: 1,
            rgba: vec![12, 34, 56, 0, 78, 90, 123, 128],
        };
        let (encoded, thumbnail) = encode_webp_variants(captured).unwrap();
        let decoded = image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        let decoded_thumbnail =
            image::load_from_memory_with_format(&thumbnail, image::ImageFormat::WebP)
                .unwrap()
                .to_rgba8();

        assert_eq!(decoded.as_raw(), &[12, 34, 56, 255, 78, 90, 123, 255]);
        assert!(decoded_thumbnail
            .pixels()
            .all(|pixel| pixel.0[3] == u8::MAX));
    }

    #[test]
    fn encoding_creates_a_bounded_thumbnail() {
        let captured = solid_capture(2880, 1620);
        let (encoded, thumbnail) = encode_webp_variants(captured).unwrap();
        let decoded =
            image::load_from_memory_with_format(&encoded, image::ImageFormat::WebP).unwrap();
        let decoded_thumbnail =
            image::load_from_memory_with_format(&thumbnail, image::ImageFormat::WebP).unwrap();

        assert_eq!((decoded.width(), decoded.height()), (2880, 1620));
        assert_eq!(
            (decoded_thumbnail.width(), decoded_thumbnail.height()),
            (THUMBNAIL_MAX_WIDTH, 810)
        );
    }

    #[test]
    fn validated_webp_accepts_only_decodable_webp_files() {
        let directory = temporary_test_directory();
        let valid_path = directory.join("valid.webp");
        let invalid_path = directory.join("invalid.webp");
        let (encoded, _) = encode_webp_variants(solid_capture(4, 2)).unwrap();
        std::fs::write(&valid_path, &encoded).unwrap();
        std::fs::write(&invalid_path, b"not an image").unwrap();

        assert_eq!(read_validated_webp(&valid_path).unwrap(), encoded);
        assert!(matches!(
            read_validated_webp(&invalid_path),
            Err(CapturePipelineError::InvalidContainer)
        ));
        assert_eq!(
            read_integrity_checked_webp(
                &valid_path,
                encoded.len() as u64,
                &hex::encode(Sha256::digest(&encoded)),
            )
            .unwrap(),
            encoded
        );
        assert!(matches!(
            read_integrity_checked_webp(&valid_path, 1, "invalid"),
            Err(CapturePipelineError::InvalidContainer)
        ));

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn capture_paths_are_limited_to_the_expected_managed_directory() {
        let id = Uuid::new_v4();
        let relative = format!("captures/2026/07/29/{id}.webp");
        assert!(
            resolve_capture_path(Path::new("data"), &relative, id, "webp", &["captures"],).is_ok()
        );
        assert!(resolve_capture_path(
            Path::new("data"),
            &format!("exports/{id}.webp"),
            id,
            "webp",
            &["captures"],
        )
        .is_err());
        assert!(resolve_capture_path(
            Path::new("data"),
            &format!("captures/../{id}.webp"),
            id,
            "webp",
            &["captures"],
        )
        .is_err());
    }

    #[tokio::test]
    async fn staged_deletion_can_be_rolled_back_before_database_commit() {
        let directory = temporary_test_directory();
        let capture_id = Uuid::new_v4();
        let original_path = directory.join(format!("{capture_id}.webp"));
        std::fs::write(&original_path, b"test image bytes").unwrap();

        let staged = stage_file_for_deletion(&original_path, capture_id, true)
            .await
            .unwrap()
            .unwrap();
        assert!(!original_path.exists());
        assert!(staged.staged_path.exists());
        rollback_staged_files(&[staged]).await.unwrap();
        assert!(original_path.exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn committed_deletion_removes_every_staged_file() {
        let directory = temporary_test_directory();
        let capture_id = Uuid::new_v4();
        let original_path = directory.join(format!("{capture_id}.webp"));
        std::fs::write(&original_path, b"test image bytes").unwrap();

        let staged = stage_file_for_deletion(&original_path, capture_id, true)
            .await
            .unwrap()
            .unwrap();
        let staged_path = staged.staged_path.clone();
        remove_staged_files(&[staged]).await.unwrap();
        assert!(!original_path.exists());
        assert!(!staged_path.exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn validated_webp_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let directory = temporary_test_directory();
        let target_path = directory.join("target.webp");
        let link_path = directory.join("link.webp");
        let (encoded, _) = encode_webp_variants(solid_capture(4, 2)).unwrap();
        std::fs::write(&target_path, encoded).unwrap();
        symlink(&target_path, &link_path).unwrap();

        assert!(matches!(
            read_validated_webp(&link_path),
            Err(CapturePipelineError::InvalidContainer)
        ));

        std::fs::remove_dir_all(directory).unwrap();
    }
}
