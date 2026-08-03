use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use thiserror::Error;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
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
    pub comparison_exclusions: Option<Vec<PixelRect>>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[error("screen capture is not implemented for this platform adapter yet")]
    NotImplemented,
    #[error("screen capture permission was not granted")]
    PermissionDenied,
    #[cfg(target_os = "windows")]
    #[error("screen capture permission state could not be checked")]
    PermissionCheckFailed,
    #[cfg(target_os = "windows")]
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

    /// Returns the display containing the current foreground window when the
    /// platform can determine it. Callers must fall back to the primary
    /// display when this is unavailable.
    async fn active_display(&self) -> Result<Option<DisplayId>, CaptureError> {
        Ok(None)
    }

    async fn comparison_exclusions(
        &self,
        _app: &AppHandle,
        _display_id: &DisplayId,
        _capture_width: u32,
        _capture_height: u32,
    ) -> Option<Vec<PixelRect>> {
        None
    }
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn edge_exclusions_from_work_area(
    monitor: ScreenRect,
    work: ScreenRect,
    capture_width: u32,
    capture_height: u32,
) -> Option<Vec<PixelRect>> {
    let monitor_width = monitor.right.checked_sub(monitor.left)?;
    let monitor_height = monitor.bottom.checked_sub(monitor.top)?;
    if monitor_width <= 0
        || monitor_height <= 0
        || capture_width == 0
        || capture_height == 0
        || work.left < monitor.left
        || work.top < monitor.top
        || work.right > monitor.right
        || work.bottom > monitor.bottom
        || work.left > work.right
        || work.top > work.bottom
    {
        return None;
    }

    let scale_x = f64::from(capture_width) / f64::from(monitor_width);
    let scale_y = f64::from(capture_height) / f64::from(monitor_height);
    let left = (f64::from(work.left - monitor.left) * scale_x).round() as u32;
    let top = (f64::from(work.top - monitor.top) * scale_y).round() as u32;
    let right = (f64::from(monitor.right - work.right) * scale_x).round() as u32;
    let bottom = (f64::from(monitor.bottom - work.bottom) * scale_y).round() as u32;

    if left > (capture_width / 10).max(1)
        || right > (capture_width / 10).max(1)
        || top > (capture_height / 10).max(1)
        || bottom > (capture_height / 10).max(1)
        || left.checked_add(right)? > capture_width
        || top.checked_add(bottom)? > capture_height
    {
        return None;
    }

    let mut exclusions = Vec::with_capacity(4);
    if top > 0 {
        exclusions.push(PixelRect {
            x: 0,
            y: 0,
            width: capture_width,
            height: top,
        });
    }
    if bottom > 0 {
        exclusions.push(PixelRect {
            x: 0,
            y: capture_height - bottom,
            width: capture_width,
            height: bottom,
        });
    }

    let middle_height = capture_height - top - bottom;
    if left > 0 && middle_height > 0 {
        exclusions.push(PixelRect {
            x: 0,
            y: top,
            width: left,
            height: middle_height,
        });
    }
    if right > 0 && middle_height > 0 {
        exclusions.push(PixelRect {
            x: capture_width - right,
            y: top,
            width: right,
            height: middle_height,
        });
    }
    exclusions.sort_unstable();
    Some(exclusions)
}

#[cfg(target_os = "macos")]
pub use macos::PlatformCapture;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use unsupported::PlatformCapture;
#[cfg(target_os = "windows")]
pub use windows::PlatformCapture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_area_edges_scale_to_capture_pixels_without_overlap() {
        let exclusions = edge_exclusions_from_work_area(
            ScreenRect {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
            },
            ScreenRect {
                left: -1872,
                top: 24,
                right: 0,
                bottom: 1032,
            },
            3840,
            2160,
        )
        .unwrap();

        assert_eq!(
            exclusions,
            vec![
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 3840,
                    height: 48,
                },
                PixelRect {
                    x: 0,
                    y: 48,
                    width: 96,
                    height: 2016,
                },
                PixelRect {
                    x: 0,
                    y: 2064,
                    width: 3840,
                    height: 96,
                },
            ]
        );
    }

    #[test]
    fn anomalous_or_out_of_bounds_work_areas_are_rejected() {
        let monitor = ScreenRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(edge_exclusions_from_work_area(
            monitor,
            ScreenRect {
                left: 0,
                top: 200,
                right: 1920,
                bottom: 1080,
            },
            1920,
            1080,
        )
        .is_none());
        assert!(edge_exclusions_from_work_area(
            monitor,
            ScreenRect {
                left: -1,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            1920,
            1080,
        )
        .is_none());
    }
}
