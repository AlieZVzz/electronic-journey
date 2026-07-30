use std::{
    mem::size_of,
    slice, thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use windows::{
    core::{factory, Interface, HSTRING, PCWSTR},
    Graphics::{
        Capture::{
            Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureAccess,
            GraphicsCaptureAccessKind, GraphicsCaptureItem, GraphicsCaptureSession,
        },
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Security::Authorization::AppCapabilityAccess::{AppCapability, AppCapabilityAccessStatus},
    Win32::{
        Foundation::{HMODULE, LPARAM, RECT},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
                D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::{
                Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIAdapter, IDXGIDevice,
            },
            Gdi::{
                EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW,
                ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
            },
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
            RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
        },
    },
};

use super::{
    edge_exclusions_from_work_area, CaptureError, CapturedImage, DisplayId, DisplayInfo,
    PermissionState, PixelRect, ScreenCapture, ScreenRect,
};

const PRIMARY_MONITOR_FLAG: u32 = 1;
const FRAME_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Windows.Graphics.Capture adapter boundary.
pub struct PlatformCapture;

#[derive(Clone)]
struct NativeDisplay {
    handle: HMONITOR,
    id: String,
    width: u32,
    height: u32,
    is_primary: bool,
}

struct WindowsRuntimeGuard;

impl WindowsRuntimeGuard {
    fn initialize() -> Result<Self, CaptureError> {
        // Capture runs on a dedicated blocking worker, so it owns a balanced
        // multithreaded Windows Runtime initialization for that worker.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map(|_| Self)
            .map_err(|_| CaptureError::CaptureFailed)
    }
}

impl Drop for WindowsRuntimeGuard {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

fn map_access_status(status: AppCapabilityAccessStatus) -> PermissionState {
    if status == AppCapabilityAccessStatus::Allowed {
        PermissionState::Granted
    } else if status == AppCapabilityAccessStatus::UserPromptRequired {
        PermissionState::NotDetermined
    } else {
        PermissionState::Denied
    }
}

fn utf16_string(value: &[u16]) -> Option<String> {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    let value = String::from_utf16(&value[..length]).ok()?;
    (!value.is_empty()).then_some(value)
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _device_context: HDC,
    _monitor_rect: *mut RECT,
    data: LPARAM,
) -> windows::core::BOOL {
    let displays = unsafe { &mut *(data.0 as *mut Vec<NativeDisplay>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }.as_bool() {
        return true.into();
    }

    let Some(id) = utf16_string(&info.szDevice) else {
        return true.into();
    };
    let mut mode = DEVMODEW::default();
    mode.dmSize = size_of::<DEVMODEW>() as u16;
    let (width, height) = if unsafe {
        EnumDisplaySettingsW(
            PCWSTR(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
        )
    }
    .as_bool()
    {
        (mode.dmPelsWidth, mode.dmPelsHeight)
    } else {
        let rect = info.monitorInfo.rcMonitor;
        let Ok(width) = u32::try_from(rect.right.saturating_sub(rect.left)) else {
            return true.into();
        };
        let Ok(height) = u32::try_from(rect.bottom.saturating_sub(rect.top)) else {
            return true.into();
        };
        (width, height)
    };
    if width == 0 || height == 0 {
        return true.into();
    }

    displays.push(NativeDisplay {
        handle: monitor,
        id,
        width,
        height,
        is_primary: info.monitorInfo.dwFlags & PRIMARY_MONITOR_FLAG != 0,
    });
    true.into()
}

fn native_displays() -> Result<Vec<NativeDisplay>, CaptureError> {
    let mut displays = Vec::new();
    let data = LPARAM((&mut displays as *mut Vec<NativeDisplay>) as isize);
    if !unsafe { EnumDisplayMonitors(None, None, Some(collect_monitor), data) }.as_bool() {
        return Err(CaptureError::CaptureFailed);
    }
    Ok(displays)
}

fn display_handle(display_id: &DisplayId) -> Result<HMONITOR, CaptureError> {
    native_displays()?
        .into_iter()
        .find(|display| display.id == display_id.0)
        .map(|display| display.handle)
        .ok_or_else(|| CaptureError::DisplayUnavailable(display_id.0.clone()))
}

fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext), CaptureError> {
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            None::<&IDXGIAdapter>,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|_| CaptureError::CaptureFailed)?;

    match (device, context) {
        (Some(device), Some(context)) => Ok((device, context)),
        _ => Err(CaptureError::CaptureFailed),
    }
}

fn create_capture_item(monitor: HMONITOR) -> Result<GraphicsCaptureItem, CaptureError> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|_| CaptureError::CaptureFailed)?;
    unsafe { interop.CreateForMonitor(monitor) }.map_err(|_| CaptureError::CaptureFailed)
}

fn wait_for_frame(
    frame_pool: &Direct3D11CaptureFramePool,
) -> Result<Direct3D11CaptureFrame, CaptureError> {
    let deadline = Instant::now() + FRAME_WAIT_TIMEOUT;
    loop {
        if let Ok(frame) = frame_pool.TryGetNextFrame() {
            return Ok(frame);
        }
        if Instant::now() >= deadline {
            return Err(CaptureError::CaptureFailed);
        }
        thread::sleep(FRAME_POLL_INTERVAL);
    }
}

fn rgba_from_bgra_rows(
    source: &[u8],
    row_pitch: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    let width = usize::try_from(width).map_err(|_| CaptureError::CaptureFailed)?;
    let height = usize::try_from(height).map_err(|_| CaptureError::CaptureFailed)?;
    let row_bytes = width.checked_mul(4).ok_or(CaptureError::CaptureFailed)?;
    let source_length = row_pitch
        .checked_mul(height)
        .ok_or(CaptureError::CaptureFailed)?;
    let output_length = row_bytes
        .checked_mul(height)
        .ok_or(CaptureError::CaptureFailed)?;
    if row_pitch < row_bytes || source.len() < source_length {
        return Err(CaptureError::CaptureFailed);
    }

    let mut rgba = Vec::with_capacity(output_length);
    for row in source.chunks(row_pitch).take(height) {
        for pixel in row[..row_bytes].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(rgba)
}

fn read_frame_pixels(
    frame: &Direct3D11CaptureFrame,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
) -> Result<CapturedImage, CaptureError> {
    let content_size = frame
        .ContentSize()
        .map_err(|_| CaptureError::CaptureFailed)?;
    let width = u32::try_from(content_size.Width).map_err(|_| CaptureError::CaptureFailed)?;
    let height = u32::try_from(content_size.Height).map_err(|_| CaptureError::CaptureFailed)?;
    if width == 0 || height == 0 {
        return Err(CaptureError::CaptureFailed);
    }

    let surface = frame.Surface().map_err(|_| CaptureError::CaptureFailed)?;
    let access: IDirect3DDxgiInterfaceAccess =
        surface.cast().map_err(|_| CaptureError::CaptureFailed)?;
    let source_texture: ID3D11Texture2D =
        unsafe { access.GetInterface() }.map_err(|_| CaptureError::CaptureFailed)?;
    let mut source_desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { source_texture.GetDesc(&mut source_desc) };
    if width > source_desc.Width
        || height > source_desc.Height
        || source_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        || source_desc.SampleDesc.Count != 1
    {
        return Err(CaptureError::CaptureFailed);
    }

    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: source_desc.Width,
        Height: source_desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: source_desc.Format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging_texture = None;
    unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging_texture)) }
        .map_err(|_| CaptureError::CaptureFailed)?;
    let staging_texture = staging_texture.ok_or(CaptureError::CaptureFailed)?;
    unsafe { context.CopyResource(&staging_texture, &source_texture) };

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging_texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
        .map_err(|_| CaptureError::CaptureFailed)?;

    let source_length = usize::try_from(mapped.RowPitch)
        .ok()
        .and_then(|pitch| pitch.checked_mul(height as usize));
    let pixels = match source_length {
        Some(length) if !mapped.pData.is_null() => {
            let source = unsafe { slice::from_raw_parts(mapped.pData.cast::<u8>(), length) };
            rgba_from_bgra_rows(source, mapped.RowPitch as usize, width, height)
        }
        _ => Err(CaptureError::CaptureFailed),
    };
    unsafe { context.Unmap(&staging_texture, 0) };

    pixels.map(|rgba| CapturedImage {
        width,
        height,
        rgba,
        comparison_exclusions: None,
    })
}

fn capture_display(display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
    let _runtime = WindowsRuntimeGuard::initialize()?;
    let monitor = display_handle(display_id)?;
    let item = create_capture_item(monitor)?;
    let size = item.Size().map_err(|_| CaptureError::CaptureFailed)?;
    if size.Width <= 0 || size.Height <= 0 {
        return Err(CaptureError::DisplayUnavailable(display_id.0.clone()));
    }

    let (device, context) = create_d3d_device()?;
    let dxgi_device: IDXGIDevice = device.cast().map_err(|_| CaptureError::CaptureFailed)?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }
        .map_err(|_| CaptureError::CaptureFailed)?;
    let direct3d_device: IDirect3DDevice = inspectable
        .cast()
        .map_err(|_| CaptureError::CaptureFailed)?;
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &direct3d_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        SizeInt32 {
            Width: size.Width,
            Height: size.Height,
        },
    )
    .map_err(|_| CaptureError::CaptureFailed)?;
    let session: GraphicsCaptureSession = frame_pool
        .CreateCaptureSession(&item)
        .map_err(|_| CaptureError::CaptureFailed)?;

    let result = (|| {
        session
            .StartCapture()
            .map_err(|_| CaptureError::CaptureFailed)?;
        let frame = wait_for_frame(&frame_pool)?;
        let pixels = read_frame_pixels(&frame, &device, &context);
        let _ = frame.Close();
        pixels
    })();
    let _ = session.Close();
    let _ = frame_pool.Close();
    result
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
        native_displays().map(|displays| {
            displays
                .into_iter()
                .map(|display| DisplayInfo {
                    name: if display.is_primary {
                        "主显示器".into()
                    } else {
                        display.id.clone()
                    },
                    id: DisplayId(display.id),
                    width: display.width,
                    height: display.height,
                    is_primary: display.is_primary,
                })
                .collect()
        })
    }

    async fn capture(&self, display_id: &DisplayId) -> Result<CapturedImage, CaptureError> {
        if self.permission_state().await? != PermissionState::Granted {
            return Err(CaptureError::PermissionDenied);
        }

        let display_id = display_id.clone();
        tokio::task::spawn_blocking(move || capture_display(&display_id))
            .await
            .map_err(|_| CaptureError::CaptureFailed)?
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

#[cfg(test)]
mod tests {
    use std::future::IntoFuture;

    use windows_future::IAsyncOperation;

    use super::*;

    #[test]
    fn permission_request_operation_supports_await() {
        fn assert_into_future<T: IntoFuture>() {}

        assert_into_future::<IAsyncOperation<AppCapabilityAccessStatus>>();
    }

    #[test]
    fn converts_padded_bgra_rows_to_tight_rgba() {
        let source = [
            3, 2, 1, 4, 7, 6, 5, 8, 0, 0, 0, 0, 30, 20, 10, 40, 70, 60, 50, 80, 0, 0, 0, 0,
        ];

        assert_eq!(
            rgba_from_bgra_rows(&source, 12, 2, 2).unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 8, 10, 20, 30, 40, 50, 60, 70, 80]
        );
    }

    #[test]
    fn rejects_a_row_pitch_smaller_than_the_pixel_row() {
        assert!(matches!(
            rgba_from_bgra_rows(&[0; 8], 4, 2, 1),
            Err(CaptureError::CaptureFailed)
        ));
    }

    #[tokio::test]
    #[ignore = "requires an interactive Windows 11 session with graphics capture permission"]
    async fn captures_real_pixels_from_the_primary_display() {
        assert_eq!(
            PlatformCapture.permission_state().await.unwrap(),
            PermissionState::Granted,
            "grant programmatic graphics capture permission before running this test"
        );
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
