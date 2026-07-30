use std::time::Duration;

use async_trait::async_trait;
use core_graphics::{access::ScreenCaptureAccess, display::CGDisplay};
use objc2_app_kit::NSScreen;
use objc2_foundation::MainThreadMarker;
use tauri::AppHandle;

use super::{
    CaptureError, CapturedImage, DisplayId, DisplayInfo, PermissionState, PixelRect, ScreenCapture,
};

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
            comparison_exclusions: None,
        })
    }

    async fn comparison_exclusions(
        &self,
        app: &AppHandle,
        display_id: &DisplayId,
        capture_width: u32,
        capture_height: u32,
    ) -> Option<Vec<PixelRect>> {
        let native_id = display_id.0.parse::<u32>().ok()?;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let exclusions = menu_bar_exclusions(native_id, capture_width, capture_height);
            let _ = sender.send(exclusions);
        })
        .ok()?;
        tokio::time::timeout(Duration::from_secs(2), receiver)
            .await
            .ok()?
            .ok()
            .flatten()
    }
}

fn menu_bar_exclusions(
    display_id: u32,
    capture_width: u32,
    capture_height: u32,
) -> Option<Vec<PixelRect>> {
    let main_thread = MainThreadMarker::new()?;
    let screens = NSScreen::screens(main_thread);
    let screen = screens
        .iter()
        .find(|screen| screen.CGDirectDisplayID() == display_id)?;
    let frame = screen.frame();
    let visible = screen.visibleFrame();
    if frame.size.width <= 0.0 || frame.size.height <= 0.0 {
        return None;
    }

    let frame_top = frame.origin.y + frame.size.height;
    let visible_top = visible.origin.y + visible.size.height;
    let top_gap_points = frame_top - visible_top;
    top_edge_exclusion(
        frame.size.height,
        top_gap_points,
        capture_width,
        capture_height,
    )
}

fn top_edge_exclusion(
    frame_height_points: f64,
    top_gap_points: f64,
    capture_width: u32,
    capture_height: u32,
) -> Option<Vec<PixelRect>> {
    if !frame_height_points.is_finite()
        || !top_gap_points.is_finite()
        || frame_height_points <= 0.0
        || top_gap_points < 0.0
        || capture_width == 0
        || capture_height == 0
    {
        return None;
    }
    let top_gap_pixels =
        (top_gap_points * f64::from(capture_height) / frame_height_points).round() as u32;
    if top_gap_pixels > (capture_height / 10).max(1) {
        return None;
    }

    let mut exclusions = Vec::with_capacity(1);
    if top_gap_pixels > 0 {
        exclusions.push(PixelRect {
            x: 0,
            y: 0,
            width: capture_width,
            height: top_gap_pixels,
        });
    }
    Some(exclusions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_bar_points_scale_to_capture_pixels() {
        assert_eq!(
            top_edge_exclusion(900.0, 24.0, 2880, 1800),
            Some(vec![PixelRect {
                x: 0,
                y: 0,
                width: 2880,
                height: 48,
            }])
        );
        assert!(top_edge_exclusion(900.0, 100.0, 1440, 900).is_none());
    }

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
