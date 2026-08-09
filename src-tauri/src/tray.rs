use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(not(target_os = "windows"))]
use tauri::menu::{Menu, PredefinedMenuItem};
#[cfg(target_os = "windows")]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{
    menu::MenuItem,
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager,
};
#[cfg(target_os = "windows")]
use tauri::{PhysicalPosition, PhysicalSize};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

use crate::{
    capture::PermissionState,
    commands::{RecordingState, RuntimeState, RuntimeSummary, SuspensionReason},
    database,
};
#[cfg(not(target_os = "windows"))]
use crate::{commands, error::AppError};

const STATUS_ID: &str = "tray-status";
const PERMISSION_ID: &str = "tray-permission";
const TODAY_CAPTURED_ID: &str = "tray-today-captured";
const TODAY_UPLOADED_ID: &str = "tray-today-uploaded";
const START_ID: &str = "tray-start";
const PAUSE_ID: &str = "tray-pause";
const STOP_ID: &str = "tray-stop";
#[cfg(not(target_os = "windows"))]
const OPEN_ID: &str = "tray-open";
#[cfg(not(target_os = "windows"))]
const QUIT_ID: &str = "tray-quit";
#[cfg(target_os = "windows")]
const PANEL_LABEL: &str = "tray-panel";
#[cfg(target_os = "windows")]
const PANEL_WIDTH: f64 = 360.0;
#[cfg(target_os = "windows")]
const PANEL_HEIGHT: f64 = 440.0;

pub struct TrayState {
    _icon: TrayIcon,
    status: MenuItem<tauri::Wry>,
    permission: MenuItem<tauri::Wry>,
    today_captured: MenuItem<tauri::Wry>,
    today_uploaded: MenuItem<tauri::Wry>,
    stats_refresh_generation: AtomicU64,
    start: MenuItem<tauri::Wry>,
    pause: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
}

#[derive(Debug, PartialEq, Eq)]
struct TrayPresentation {
    status: String,
    permission: String,
    tooltip: String,
    permission_action_enabled: bool,
    start_enabled: bool,
    pause_enabled: bool,
    stop_enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraySnapshot {
    state: RecordingState,
    suspension_reason: Option<SuspensionReason>,
    permission_state: PermissionState,
    today_captured: u32,
    today_uploaded: u32,
}

fn today_captured_label(count: u32) -> String {
    format!("今日截图：{count} 张")
}

fn today_uploaded_label(count: u32) -> String {
    format!("今日已上传：{count} 张")
}

impl TrayPresentation {
    fn from_summary(summary: &RuntimeSummary) -> Self {
        let state_label = match (summary.state, summary.suspension_reason) {
            (RecordingState::Stopped, _) => "已停止",
            (RecordingState::Running, _) => "正在记录",
            (RecordingState::Paused, _) => "已暂停",
            (RecordingState::Suspended, Some(SuspensionReason::ScreenLocked)) => {
                "系统暂挂：屏幕已锁定"
            }
            (RecordingState::Suspended, Some(SuspensionReason::SystemSleeping)) => {
                "系统暂挂：系统休眠"
            }
            (RecordingState::Suspended, Some(SuspensionReason::UserIdle)) => "系统暂挂：用户空闲",
            (RecordingState::Suspended, None) => "系统暂挂",
            (RecordingState::Degraded, _) => "记录异常",
        };
        let (permission, permission_action_enabled) = match summary.permission_state {
            PermissionState::Granted => ("屏幕录制权限：已授权", false),
            PermissionState::NotDetermined => ("屏幕录制权限：待确认（点击处理）", true),
            PermissionState::Denied => ("屏幕录制权限：未授权（点击处理）", true),
        };
        let recording_active = matches!(
            summary.state,
            RecordingState::Running | RecordingState::Suspended
        );
        let tooltip = if summary.permission_state == PermissionState::Granted {
            format!("Electronic Journey · {state_label}")
        } else {
            format!("Electronic Journey · {state_label} · 需要屏幕录制权限")
        };

        Self {
            status: format!("状态：{state_label}"),
            permission: permission.into(),
            tooltip,
            permission_action_enabled,
            start_enabled: !recording_active,
            pause_enabled: recording_active,
            stop_enabled: summary.state != RecordingState::Stopped,
        }
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "windows")]
    install_panel_window(app)?;

    let summary = app
        .state::<RuntimeState>()
        .summary()
        .unwrap_or(RuntimeSummary {
            state: RecordingState::Degraded,
            suspension_reason: None,
            permission_state: PermissionState::NotDetermined,
        });
    let presentation = TrayPresentation::from_summary(&summary);
    let status = MenuItem::with_id(app, STATUS_ID, &presentation.status, false, None::<&str>)?;
    let permission = MenuItem::with_id(
        app,
        PERMISSION_ID,
        &presentation.permission,
        presentation.permission_action_enabled,
        None::<&str>,
    )?;
    let today_captured = MenuItem::with_id(
        app,
        TODAY_CAPTURED_ID,
        "今日截图：读取中…",
        false,
        None::<&str>,
    )?;
    let today_uploaded = MenuItem::with_id(
        app,
        TODAY_UPLOADED_ID,
        "今日已上传：读取中…",
        false,
        None::<&str>,
    )?;
    let start = MenuItem::with_id(
        app,
        START_ID,
        "开始记录",
        presentation.start_enabled,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(
        app,
        PAUSE_ID,
        "暂停记录",
        presentation.pause_enabled,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(
        app,
        STOP_ID,
        "停止记录",
        presentation.stop_enabled,
        None::<&str>,
    )?;
    #[cfg(not(target_os = "windows"))]
    let open = MenuItem::with_id(app, OPEN_ID, "打开主窗口", true, None::<&str>)?;
    #[cfg(not(target_os = "windows"))]
    let quit = MenuItem::with_id(app, QUIT_ID, "退出 Electronic Journey", true, None::<&str>)?;
    #[cfg(not(target_os = "windows"))]
    let menu = {
        let separator_one = PredefinedMenuItem::separator(app)?;
        let separator_two = PredefinedMenuItem::separator(app)?;
        let separator_three = PredefinedMenuItem::separator(app)?;
        Menu::with_items(
            app,
            &[
                &status,
                &permission,
                &today_captured,
                &today_uploaded,
                &separator_one,
                &start,
                &pause,
                &stop,
                &separator_two,
                &open,
                &separator_three,
                &quit,
            ],
        )?
    };

    let mut builder = TrayIconBuilder::with_id("electronic-journey-tray")
        .tooltip(&presentation.tooltip)
        .show_menu_on_left_click(false);

    #[cfg(target_os = "windows")]
    {
        builder = builder
            .on_tray_icon_event(|tray, event| handle_tray_icon_event(tray.app_handle(), event));
    }

    #[cfg(not(target_os = "windows"))]
    {
        builder = builder.menu(&menu).on_menu_event(handle_menu_event);
    }

    #[cfg(target_os = "macos")]
    {
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
        builder = builder.icon(icon).icon_as_template(true);
    }

    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let icon = builder.build(app)?;
    app.manage(TrayState {
        _icon: icon,
        status,
        permission,
        today_captured,
        today_uploaded,
        stats_refresh_generation: AtomicU64::new(0),
        start,
        pause,
        stop,
    });
    refresh(app);
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    let _ = app.emit("runtime-state-changed", ());
    let Some(tray) = app.try_state::<TrayState>() else {
        return;
    };
    let Ok(summary) = app.state::<RuntimeState>().summary() else {
        return;
    };
    let presentation = TrayPresentation::from_summary(&summary);
    let _ = tray.status.set_text(&presentation.status);
    let _ = tray.permission.set_text(&presentation.permission);
    let _ = tray
        .permission
        .set_enabled(presentation.permission_action_enabled);
    let _ = tray.start.set_enabled(presentation.start_enabled);
    let _ = tray.pause.set_enabled(presentation.pause_enabled);
    let _ = tray.stop.set_enabled(presentation.stop_enabled);
    let _ = tray._icon.set_tooltip(Some(&presentation.tooltip));

    let generation = tray.stats_refresh_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let pool = app.state::<sqlx::SqlitePool>().inner().clone();
        let Some(tray) = app.try_state::<TrayState>() else {
            return;
        };
        if tray.stats_refresh_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let stats = match database::today_capture_stats(&pool).await {
            Ok(stats) => stats,
            Err(_) => {
                let _ = tray.today_captured.set_text("今日截图：暂不可用");
                let _ = tray.today_uploaded.set_text("今日已上传：暂不可用");
                return;
            }
        };
        if tray.stats_refresh_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let _ = tray
            .today_captured
            .set_text(&today_captured_label(stats.captured));
        let _ = tray
            .today_uploaded
            .set_text(&today_uploaded_label(stats.uploaded));
    });
}

#[tauri::command]
pub async fn get_tray_snapshot(
    app: AppHandle,
    pool: tauri::State<'_, sqlx::SqlitePool>,
) -> Result<TraySnapshot, String> {
    let summary = app
        .state::<RuntimeState>()
        .summary()
        .map_err(|error| error.to_string())?;
    let stats = database::today_capture_stats(pool.inner())
        .await
        .map_err(|_| "无法读取今日统计。".to_string())?;

    Ok(TraySnapshot {
        state: summary.state,
        suspension_reason: summary.suspension_reason,
        permission_state: summary.permission_state,
        today_captured: stats.captured,
        today_uploaded: stats.uploaded,
    })
}

#[tauri::command]
pub fn open_main_window_from_tray(app: AppHandle) {
    show_main_window(&app);
}

#[tauri::command]
pub fn quit_from_tray(app: AppHandle) {
    app.exit(0);
}

#[cfg(target_os = "windows")]
fn install_panel_window(app: &AppHandle) -> tauri::Result<()> {
    tauri::WebviewWindowBuilder::new(
        app,
        PANEL_LABEL,
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Electronic Journey")
    .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .focused(false)
    .visible(false)
    .build()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn handle_tray_icon_event(app: &AppHandle, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        position,
        button: MouseButton::Right,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        show_panel_at(app, position);
    }
}

#[cfg(target_os = "windows")]
fn show_panel_at(app: &AppHandle, pointer: PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window(PANEL_LABEL) else {
        return;
    };
    let Ok(panel_size) = window.outer_size() else {
        return;
    };
    let Ok(Some(monitor)) = window.monitor_from_point(pointer.x, pointer.y) else {
        return;
    };
    let position = panel_position(pointer, panel_size, *monitor.position(), *monitor.size());
    let _ = window.set_position(position);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(PANEL_LABEL, "tray-panel-opened", ());
}

#[cfg(target_os = "windows")]
fn panel_position(
    pointer: PhysicalPosition<f64>,
    panel: PhysicalSize<u32>,
    monitor_position: PhysicalPosition<i32>,
    monitor_size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    const GAP: f64 = 10.0;
    let left = f64::from(monitor_position.x);
    let top = f64::from(monitor_position.y);
    let right = left + f64::from(monitor_size.width);
    let bottom = top + f64::from(monitor_size.height);
    let panel_width = f64::from(panel.width);
    let panel_height = f64::from(panel.height);
    let x = if pointer.x > left + f64::from(monitor_size.width) / 2.0 {
        pointer.x - panel_width
    } else {
        pointer.x
    }
    .clamp(left + GAP, (right - panel_width - GAP).max(left + GAP));
    let y = if pointer.y > top + f64::from(monitor_size.height) / 2.0 {
        pointer.y - panel_height - GAP
    } else {
        pointer.y + GAP
    }
    .clamp(top + GAP, (bottom - panel_height - GAP).max(top + GAP));

    PhysicalPosition::new(x.round() as i32, y.round() as i32)
}

#[cfg(not(target_os = "windows"))]
fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        START_ID => match commands::apply_recording_state(app, RecordingState::Running) {
            Ok(_) => refresh(app),
            Err(AppError::CapturePermissionRequired) => show_permission_prompt(app),
            Err(error) => show_action_error(app, &error),
        },
        PAUSE_ID => {
            if let Err(error) = commands::apply_recording_state(app, RecordingState::Paused) {
                show_action_error(app, &error);
            }
            refresh(app);
        }
        STOP_ID => {
            if let Err(error) = commands::apply_recording_state(app, RecordingState::Stopped) {
                show_action_error(app, &error);
            }
            refresh(app);
        }
        PERMISSION_ID => show_permission_prompt(app),
        OPEN_ID => show_main_window(app),
        QUIT_ID => app.exit(0),
        _ => {}
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(not(target_os = "windows"))]
fn show_permission_prompt(app: &AppHandle) {
    show_main_window(app);
    app.dialog()
        .message(
            "开始记录前需要屏幕录制权限。为了保持授权明确，托盘不会直接触发系统授权；请在主窗口阅读访问说明并点击“检查或申请权限”。",
        )
        .title("需要屏幕录制权限")
        .kind(MessageDialogKind::Warning)
        .show(|_| {});
}

#[cfg(not(target_os = "windows"))]
fn show_action_error(app: &AppHandle, error: &AppError) {
    show_main_window(app);
    let message = match error {
        AppError::SystemMonitorUnavailable => {
            "系统活动监听不可用。为避免在锁屏或休眠状态下截图，记录不会启动；请重新启动应用。"
        }
        AppError::CapturePermissionRequired => "开始记录前需要在主窗口完成屏幕录制授权。",
        AppError::InvalidStateTransition => "当前记录状态不允许执行这个操作。",
        AppError::InvalidSettings(_) => "当前截图设置无效，请在主窗口中检查。",
        AppError::StatePoisoned => "应用状态暂时不可用，请重新启动应用。",
    };
    app.dialog()
        .message(message)
        .title("无法更新记录状态")
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(
        state: RecordingState,
        suspension_reason: Option<SuspensionReason>,
        permission_state: PermissionState,
    ) -> RuntimeSummary {
        RuntimeSummary {
            state,
            suspension_reason,
            permission_state,
        }
    }

    #[test]
    fn running_state_enables_pause_and_stop_only() {
        let presentation = TrayPresentation::from_summary(&summary(
            RecordingState::Running,
            None,
            PermissionState::Granted,
        ));

        assert_eq!(presentation.status, "状态：正在记录");
        assert!(!presentation.start_enabled);
        assert!(presentation.pause_enabled);
        assert!(presentation.stop_enabled);
        assert!(!presentation.permission_action_enabled);
    }

    #[test]
    fn suspended_state_reports_the_real_reason_and_can_be_paused() {
        let presentation = TrayPresentation::from_summary(&summary(
            RecordingState::Suspended,
            Some(SuspensionReason::ScreenLocked),
            PermissionState::Granted,
        ));

        assert_eq!(presentation.status, "状态：系统暂挂：屏幕已锁定");
        assert!(!presentation.start_enabled);
        assert!(presentation.pause_enabled);
        assert!(presentation.stop_enabled);
    }

    #[test]
    fn missing_permission_is_actionable_without_claiming_recording_is_ready() {
        let presentation = TrayPresentation::from_summary(&summary(
            RecordingState::Stopped,
            None,
            PermissionState::Denied,
        ));

        assert_eq!(presentation.permission, "屏幕录制权限：未授权（点击处理）");
        assert!(presentation.permission_action_enabled);
        assert!(presentation.start_enabled);
        assert!(presentation.tooltip.contains("需要屏幕录制权限"));
    }

    #[test]
    fn today_stats_use_explicit_count_labels() {
        assert_eq!(today_captured_label(12), "今日截图：12 张");
        assert_eq!(today_uploaded_label(7), "今日已上传：7 张");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn panel_position_opens_above_a_bottom_right_tray() {
        let position = panel_position(
            PhysicalPosition::new(1900.0, 1060.0),
            PhysicalSize::new(360, 500),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1080),
        );

        assert_eq!(position, PhysicalPosition::new(1540, 550));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn panel_position_stays_inside_a_negative_origin_monitor() {
        let position = panel_position(
            PhysicalPosition::new(-1900.0, 10.0),
            PhysicalSize::new(360, 500),
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(1920, 1080),
        );

        assert_eq!(position, PhysicalPosition::new(-1900, 20));
    }
}
