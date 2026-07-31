use std::{ptr::NonNull, sync::mpsc, time::Duration};

use async_trait::async_trait;
use block2::RcBlock;
use core_graphics::{access::ScreenCaptureAccess, display::CGDisplay};
use objc2::{rc::Retained, AnyThread};
use objc2_app_kit::NSScreen;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::CGImage;
use objc2_foundation::{MainThreadMarker, NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
};
use tauri::AppHandle;

use super::{
    CaptureError, CapturedImage, DisplayId, DisplayInfo, PermissionState, PixelRect, ScreenCapture,
};

/// macOS capture adapter boundary.
///
/// CoreGraphics remains the explicit permission boundary while
/// ScreenCaptureKit provides shareable displays and single-frame capture.
pub struct PlatformCapture;

const SCREEN_CAPTURE_CALLBACK_TIMEOUT: Duration = Duration::from_secs(10);

fn shareable_content() -> Result<Retained<SCShareableContent>, CaptureError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let completion = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            let retained = if error.is_null() {
                // SAFETY: ScreenCaptureKit provides a valid borrowed object
                // for the duration of this completion handler.
                unsafe { Retained::retain(content) }
            } else {
                None
            };
            let raw = retained.map(Retained::into_raw).map(|value| value as usize);
            if let Err(error) = sender.send(raw) {
                if let Some(raw) = error.0 {
                    // SAFETY: Reclaim the +1 retain count when the waiting
                    // receiver has already timed out.
                    drop(unsafe { Retained::from_raw(raw as *mut SCShareableContent) });
                }
            }
        },
    );

    // SAFETY: The copied completion block owns its channel sender, and the
    // callback pointer is retained before it leaves the callback lifetime.
    unsafe {
        SCShareableContent::getShareableContentWithCompletionHandler(&completion);
    }
    let raw = receiver
        .recv_timeout(SCREEN_CAPTURE_CALLBACK_TIMEOUT)
        .map_err(|_| CaptureError::CaptureFailed)?
        .ok_or(CaptureError::CaptureFailed)?;
    // SAFETY: The callback transferred one +1 retain count through `raw`.
    unsafe { Retained::from_raw(raw as *mut SCShareableContent) }.ok_or(CaptureError::CaptureFailed)
}

fn list_shareable_displays() -> Result<Vec<DisplayInfo>, CaptureError> {
    // Do not let content discovery become an implicit permission prompt.
    // Permission requests must remain behind the explicit user action.
    if !ScreenCaptureAccess.preflight() {
        return Err(CaptureError::PermissionDenied);
    }

    let content = shareable_content()?;
    let main_display_id = CGDisplay::main().id;
    let displays = unsafe { content.displays() };
    let mut result = Vec::with_capacity(displays.len());
    for index in 0..displays.len() {
        let display = displays.objectAtIndex(index);
        let display_id = unsafe { display.displayID() };
        let native_display = CGDisplay::new(display_id);
        let width =
            u32::try_from(native_display.pixels_wide()).map_err(|_| CaptureError::CaptureFailed)?;
        let height =
            u32::try_from(native_display.pixels_high()).map_err(|_| CaptureError::CaptureFailed)?;
        result.push(DisplayInfo {
            id: DisplayId(display_id.to_string()),
            name: if display_id == main_display_id {
                "主显示器".into()
            } else {
                format!("显示器 {display_id}")
            },
            width,
            height,
            is_primary: display_id == main_display_id,
        });
    }
    Ok(result)
}

fn screenshot_image(
    filter: &SCContentFilter,
    configuration: &SCStreamConfiguration,
) -> Result<CFRetained<CGImage>, CaptureError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let completion = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let retained = if error.is_null() {
            NonNull::new(image).map(|image| {
                // SAFETY: ScreenCaptureKit provides a valid borrowed CGImage
                // for the duration of this completion handler.
                unsafe { CFRetained::retain(image) }
            })
        } else {
            None
        };
        let raw = retained
            .map(CFRetained::into_raw)
            .map(|value| value.as_ptr() as usize);
        if let Err(error) = sender.send(raw) {
            if let Some(raw) = error.0 {
                let raw = NonNull::new(raw as *mut CGImage)
                    .expect("retained ScreenCaptureKit image pointer became null");
                // SAFETY: Reclaim the +1 retain count when the waiting
                // receiver has already timed out.
                drop(unsafe { CFRetained::from_raw(raw) });
            }
        }
    });

    // SAFETY: The filter and configuration remain alive until the callback,
    // and the callback retains the returned image before transferring it.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            filter,
            configuration,
            Some(&completion),
        );
    }
    let raw = receiver
        .recv_timeout(SCREEN_CAPTURE_CALLBACK_TIMEOUT)
        .map_err(|_| CaptureError::CaptureFailed)?
        .ok_or(CaptureError::CaptureFailed)?;
    let raw = NonNull::new(raw as *mut CGImage).ok_or(CaptureError::CaptureFailed)?;
    // SAFETY: The callback transferred one +1 retain count through `raw`.
    Ok(unsafe { CFRetained::from_raw(raw) })
}

fn bgra_rows_to_rgba(
    source: &[u8],
    width: usize,
    height: usize,
    bytes_per_row: usize,
) -> Result<Vec<u8>, CaptureError> {
    let row_pixel_bytes = width.checked_mul(4).ok_or(CaptureError::CaptureFailed)?;
    let required_len = bytes_per_row
        .checked_mul(height)
        .ok_or(CaptureError::CaptureFailed)?;
    if width == 0 || height == 0 || bytes_per_row < row_pixel_bytes || source.len() < required_len {
        return Err(CaptureError::CaptureFailed);
    }

    let pixel_len = row_pixel_bytes
        .checked_mul(height)
        .ok_or(CaptureError::CaptureFailed)?;
    let mut rgba = Vec::with_capacity(pixel_len);
    for row in source.chunks(bytes_per_row).take(height) {
        for pixel in row[..row_pixel_bytes].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(rgba)
}

fn capture_display(display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
    if !ScreenCaptureAccess.preflight() {
        return Err(CaptureError::PermissionDenied);
    }

    let native_id = display_id
        .0
        .parse::<u32>()
        .map_err(|_| CaptureError::DisplayUnavailable(display_id.0.clone()))?;
    let content = shareable_content()?;
    let displays = unsafe { content.displays() };
    let display = (0..displays.len())
        .map(|index| displays.objectAtIndex(index))
        .find(|display| unsafe { display.displayID() } == native_id)
        .ok_or_else(|| CaptureError::DisplayUnavailable(display_id.0.clone()))?;
    let native_display = CGDisplay::new(native_id);
    let width =
        usize::try_from(native_display.pixels_wide()).map_err(|_| CaptureError::CaptureFailed)?;
    let height =
        usize::try_from(native_display.pixels_high()).map_err(|_| CaptureError::CaptureFailed)?;
    if width == 0 || height == 0 {
        return Err(CaptureError::DisplayUnavailable(display_id.0.clone()));
    }
    let excluded_windows = NSArray::<SCWindow>::new();
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            &display,
            &excluded_windows,
        )
    };
    let configuration = unsafe { SCStreamConfiguration::new() };
    unsafe {
        configuration.setWidth(width);
        configuration.setHeight(height);
        configuration.setPixelFormat(u32::from_be_bytes(*b"BGRA"));
        configuration.setShowsCursor(false);
    }
    let image = screenshot_image(&filter, &configuration)?;
    let captured_width = CGImage::width(Some(&image));
    let captured_height = CGImage::height(Some(&image));
    if CGImage::bits_per_pixel(Some(&image)) != 32 {
        return Err(CaptureError::CaptureFailed);
    }
    let bytes_per_row = CGImage::bytes_per_row(Some(&image));
    let provider = CGImage::data_provider(Some(&image)).ok_or(CaptureError::CaptureFailed)?;
    let data = objc2_core_graphics::CGDataProvider::data(Some(&provider))
        .ok_or(CaptureError::CaptureFailed)?;
    let source = data.to_vec();
    let rgba = bgra_rows_to_rgba(&source, captured_width, captured_height, bytes_per_row)?;

    Ok(CapturedImage {
        width: u32::try_from(captured_width).map_err(|_| CaptureError::CaptureFailed)?,
        height: u32::try_from(captured_height).map_err(|_| CaptureError::CaptureFailed)?,
        rgba,
        comparison_exclusions: None,
    })
}

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
        tokio::task::spawn_blocking(list_shareable_displays)
            .await
            .map_err(|_| CaptureError::CaptureFailed)?
    }

    async fn capture(&self, display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        let display_id = display_id.clone();
        tokio::task::spawn_blocking(move || capture_display(&display_id))
            .await
            .map_err(|_| CaptureError::CaptureFailed)?
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
    fn converts_bgra_rows_to_tightly_packed_rgba() {
        let source = [
            3, 2, 1, 4, 7, 6, 5, 8, 0, 0, 0, 0, 11, 10, 9, 12, 15, 14, 13, 16, 0, 0, 0, 0,
        ];

        assert_eq!(
            bgra_rows_to_rgba(&source, 2, 2, 12).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn rejects_truncated_or_invalid_bgra_rows() {
        assert!(bgra_rows_to_rgba(&[0; 7], 2, 1, 8).is_err());
        assert!(bgra_rows_to_rgba(&[0; 8], 2, 1, 7).is_err());
        assert!(bgra_rows_to_rgba(&[], 0, 1, 0).is_err());
    }

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
