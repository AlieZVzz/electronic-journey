use async_trait::async_trait;

use super::{CaptureError, CapturedImage, DisplayId, DisplayInfo, PermissionState, ScreenCapture};

pub struct PlatformCapture;

#[async_trait]
impl ScreenCapture for PlatformCapture {
    async fn permission_state(&self) -> Result<PermissionState, CaptureError> {
        Err(CaptureError::NotImplemented)
    }

    async fn request_permission(&self) -> Result<PermissionState, CaptureError> {
        Err(CaptureError::NotImplemented)
    }

    async fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Err(CaptureError::NotImplemented)
    }

    async fn capture(&self, _display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::NotImplemented)
    }
}
