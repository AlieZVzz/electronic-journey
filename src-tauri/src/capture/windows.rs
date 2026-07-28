use async_trait::async_trait;
use windows::{
    core::HSTRING,
    Graphics::Capture::{GraphicsCaptureAccess, GraphicsCaptureAccessKind, GraphicsCaptureSession},
    Security::Authorization::AppCapabilityAccess::{AppCapability, AppCapabilityAccessStatus},
};

use super::{CaptureError, CapturedImage, DisplayId, DisplayInfo, PermissionState, ScreenCapture};

/// Windows.Graphics.Capture adapter boundary.
pub struct PlatformCapture;

fn map_access_status(status: AppCapabilityAccessStatus) -> PermissionState {
    if status == AppCapabilityAccessStatus::Allowed {
        PermissionState::Granted
    } else if status == AppCapabilityAccessStatus::UserPromptRequired {
        PermissionState::NotDetermined
    } else {
        PermissionState::Denied
    }
}

#[async_trait]
impl ScreenCapture for PlatformCapture {
    async fn permission_state(&self) -> Result<PermissionState, CaptureError> {
        if !GraphicsCaptureSession::IsSupported()
            .map_err(|_| CaptureError::PermissionCheckFailed)?
        {
            return Ok(PermissionState::Denied);
        }

        let capability = AppCapability::Create(&HSTRING::from("graphicsCaptureProgrammatic"))
            .map_err(|_| CaptureError::PermissionCheckFailed)?;
        capability
            .CheckAccess()
            .map(map_access_status)
            .map_err(|_| CaptureError::PermissionCheckFailed)
    }

    async fn request_permission(&self) -> Result<PermissionState, CaptureError> {
        if !GraphicsCaptureSession::IsSupported()
            .map_err(|_| CaptureError::PermissionRequestFailed)?
        {
            return Ok(PermissionState::Denied);
        }

        let operation =
            GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Programmatic)
                .map_err(|_| CaptureError::PermissionRequestFailed)?;
        operation
            .await
            .map(map_access_status)
            .map_err(|_| CaptureError::PermissionRequestFailed)
    }

    async fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Err(CaptureError::NotImplemented)
    }

    async fn capture(&self, _display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        Err(CaptureError::NotImplemented)
    }
}
