use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    Pending,
    Uploading,
    Retry,
    Completed,
    Failed,
}

#[derive(Debug)]
pub struct UploadJob {
    pub id: Uuid,
    pub capture_id: Uuid,
    pub state: UploadState,
    pub attempt_count: u32,
    pub next_attempt_at_utc: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
}

pub const DEFAULT_MAX_CONCURRENT_UPLOADS: usize = 2;
