use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplayId(pub String);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DisplayInfo {
    pub id: DisplayId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    NotDetermined,
    Granted,
    Denied,
}

#[derive(Debug)]
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("screen capture is not implemented for this platform adapter yet")]
    NotImplemented,
    #[error("screen capture permission was not granted")]
    PermissionDenied,
    #[error("screen capture permission state could not be checked")]
    PermissionCheckFailed,
    #[error("screen capture permission request could not be completed")]
    PermissionRequestFailed,
    #[error("display is no longer available: {0}")]
    DisplayUnavailable(String),
    #[error("screen capture failed")]
    CaptureFailed,
}

#[async_trait]
pub trait ScreenCapture: Send + Sync {
    async fn permission_state(&self) -> Result<PermissionState, CaptureError>;
    async fn request_permission(&self) -> Result<PermissionState, CaptureError>;
    async fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;
    async fn capture(&self, display_id: &DisplayId) -> Result<CapturedImage, CaptureError>;
}

#[cfg(target_os = "macos")]
pub use macos::PlatformCapture;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use unsupported::PlatformCapture;
#[cfg(target_os = "windows")]
pub use windows::PlatformCapture;
