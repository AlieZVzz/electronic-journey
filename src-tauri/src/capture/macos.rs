use async_trait::async_trait;
use core_graphics::{access::ScreenCaptureAccess, display::CGDisplay};

use super::{CaptureError, CapturedImage, DisplayId, DisplayInfo, PermissionState, ScreenCapture};

/// macOS capture adapter boundary.
///
/// CoreGraphics provides the current single-frame compatibility path while
/// keeping platform types out of the scheduler and storage layers.
pub struct PlatformCapture;

#[async_trait]
impl ScreenCapture for PlatformCapture {
    async fn permission_state(&self) -> Result<PermissionState, CaptureError> {
        Ok(if ScreenCaptureAccess.preflight() {
            PermissionState::Granted
        } else {
            // macOS does not expose whether a missing grant has never been
            // requested or was denied without showing the consent prompt.
            PermissionState::NotDetermined
        })
    }

    async fn request_permission(&self) -> Result<PermissionState, CaptureError> {
        Ok(if ScreenCaptureAccess.request() {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        })
    }

    async fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        let display_ids = CGDisplay::active_displays().map_err(|_| CaptureError::CaptureFailed)?;
        let main_display_id = CGDisplay::main().id;

        Ok(display_ids
            .into_iter()
            .map(|display_id| {
                let display = CGDisplay::new(display_id);
                DisplayInfo {
                    id: DisplayId(display_id.to_string()),
                    name: if display_id == main_display_id {
                        "主显示器".into()
                    } else {
                        format!("显示器 {display_id}")
                    },
                    width: display.pixels_wide() as u32,
                    height: display.pixels_high() as u32,
                    is_primary: display_id == main_display_id,
                }
            })
            .collect())
    }

    async fn capture(&self, display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        if !ScreenCaptureAccess.preflight() {
            return Err(CaptureError::PermissionDenied);
        }

        let native_id = display_id
            .0
            .parse::<u32>()
            .map_err(|_| CaptureError::DisplayUnavailable(display_id.0.clone()))?;
        let active_displays =
            CGDisplay::active_displays().map_err(|_| CaptureError::CaptureFailed)?;
        if !active_displays.contains(&native_id) {
            return Err(CaptureError::DisplayUnavailable(display_id.0.clone()));
        }
        let display = CGDisplay::new(native_id);
        let image = display
            .image()
            .ok_or_else(|| CaptureError::DisplayUnavailable(display_id.0.clone()))?;
        let width = image.width() as u32;
        let height = image.height() as u32;
        if image.bits_per_pixel() != 32 {
            return Err(CaptureError::CaptureFailed);
        }
        let bytes_per_row = image.bytes_per_row();
        let source = image.data();
        let source = &source[..];
        if source.len() < bytes_per_row * height as usize {
            return Err(CaptureError::CaptureFailed);
        }
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for row in source.chunks(bytes_per_row).take(height as usize) {
            for pixel in row[..width as usize * 4].chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
            }
        }

        Ok(CapturedImage {
            width,
            height,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires an interactive macOS session with Screen Recording permission"]
    async fn captures_real_pixels_from_the_primary_display() {
        let displays = PlatformCapture.list_displays().await.unwrap();
        let display = displays
            .iter()
            .find(|display| display.is_primary)
            .or_else(|| displays.first())
            .unwrap();
        let captured = PlatformCapture.capture(&display.id).await.unwrap();

        assert_eq!(captured.width, display.width);
        assert_eq!(captured.height, display.height);
        assert_eq!(
            captured.rgba.len(),
            captured.width as usize * captured.height as usize * 4
        );
    }
}
