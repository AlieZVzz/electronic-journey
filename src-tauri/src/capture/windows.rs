use async_trait::async_trait;
use windows::{
    core::HSTRING,
    Graphics::Capture::{GraphicsCaptureAccess, GraphicsCaptureAccessKind, GraphicsCaptureSession},
    Security::Authorization::AppCapabilityAccess::{AppCapability, AppCapabilityAccessStatus},
    Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO},
};

use super::{
    edge_exclusions_from_work_area, CaptureError, CapturedImage, DisplayId, DisplayInfo,
    PermissionState, PixelRect, ScreenCapture, ScreenRect,
};

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

    async fn comparison_exclusions(
        &self,
        _app: &tauri::AppHandle,
        display_id: &DisplayId,
        capture_width: u32,
        capture_height: u32,
    ) -> Option<Vec<PixelRect>> {
        let handle = display_id.0.parse::<isize>().ok()?;
        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: the monitor handle comes from the platform display adapter,
        // and monitor_info points to a correctly sized writable structure.
        if !unsafe {
            GetMonitorInfoW(
                HMONITOR(handle as *mut core::ffi::c_void),
                &mut monitor_info,
            )
        }
        .as_bool()
        {
            return None;
        }

        edge_exclusions_from_work_area(
            ScreenRect {
                left: monitor_info.rcMonitor.left,
                top: monitor_info.rcMonitor.top,
                right: monitor_info.rcMonitor.right,
                bottom: monitor_info.rcMonitor.bottom,
            },
            ScreenRect {
                left: monitor_info.rcWork.left,
                top: monitor_info.rcWork.top,
                right: monitor_info.rcWork.right,
                bottom: monitor_info.rcWork.bottom,
            },
            capture_width,
            capture_height,
        )
    }
}
