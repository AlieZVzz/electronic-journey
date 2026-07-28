use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use thiserror::Error;
use uuid::Uuid;

const DEFAULT_PAGE_SIZE: usize = 18;
const MAX_PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineCapture {
    pub id: String,
    pub captured_at_utc: DateTime<Utc>,
    pub cipher_size: u64,
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
}

pub fn list_captures(
    app: &AppHandle,
    offset: u32,
    requested_limit: Option<u16>,
) -> Result<TimelinePage, TimelineError> {
    let entries = match std::fs::read_dir(captures_directory(app)?) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TimelinePage {
                items: Vec::new(),
                next_offset: None,
            });
        }
        Err(_) => return Err(TimelineError::Inventory),
    };

    let mut captures = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        let Some(id) = capture_id_from_path(&path) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        captures.push(TimelineCapture {
            id: id.to_string(),
            captured_at_utc: DateTime::<Utc>::from(modified),
            cipher_size: metadata.len(),
        });
    }

    captures.sort_by(|left, right| {
        right
            .captured_at_utc
            .cmp(&left.captured_at_utc)
            .then_with(|| right.id.cmp(&left.id))
    });

    Ok(paginate_captures(captures, offset, requested_limit))
}

pub(crate) fn captures_directory(app: &AppHandle) -> Result<PathBuf, TimelineError> {
    app.path()
        .app_local_data_dir()
        .map_err(|_| TimelineError::DataDirectory)
        .map(|path| path.join("vault").join("captures"))
}

fn capture_id_from_path(path: &std::path::Path) -> Option<Uuid> {
    if path.extension().and_then(|value| value.to_str()) != Some("ejourney") {
        return None;
    }
    path.file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn paginate_captures(
    captures: Vec<TimelineCapture>,
    offset: u32,
    requested_limit: Option<u16>,
) -> TimelinePage {
    let limit = requested_limit
        .map(usize::from)
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = offset as usize;
    let total = captures.len();
    let items = captures.into_iter().skip(offset).take(limit).collect();
    let consumed = offset.saturating_add(limit).min(total);
    let next_offset = (consumed < total).then_some(consumed as u32);

    TimelinePage { items, next_offset }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(id: &str, second: i64) -> TimelineCapture {
        TimelineCapture {
            id: id.into(),
            captured_at_utc: DateTime::from_timestamp(second, 0).unwrap(),
            cipher_size: 10,
        }
    }

    #[test]
    fn pagination_returns_a_bounded_page_and_next_offset() {
        let captures = vec![capture("a", 3), capture("b", 2), capture("c", 1)];
        let page = paginate_captures(captures, 1, Some(1));

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "b");
        assert_eq!(page.next_offset, Some(2));
    }

    #[test]
    fn only_uuid_named_encrypted_files_are_indexed() {
        let id = Uuid::new_v4();
        assert_eq!(
            capture_id_from_path(&PathBuf::from(format!("{id}.ejourney"))),
            Some(id)
        );
        assert!(capture_id_from_path(&PathBuf::from("../secret.ejourney")).is_none());
        assert!(capture_id_from_path(&PathBuf::from(format!("{id}.webp"))).is_none());
    }
}
