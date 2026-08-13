mod auto_sync;
mod autostart;
mod capture;
mod capture_pipeline;
mod commands;
mod database;
mod error;
mod image_fingerprint;
mod privacy;
mod scheduler;
mod system_monitor;
mod timeline;
mod tray;
mod upload;
mod vault;

use std::{sync::OnceLock, time::Instant};

use commands::RuntimeState;
use tauri::Manager;

static STARTUP_STARTED: OnceLock<Instant> = OnceLock::new();

pub(crate) fn trace_startup(stage: &str) {
    if std::env::var_os("ELECTRONIC_JOURNEY_STARTUP_TRACE").is_some() {
        let elapsed = STARTUP_STARTED
            .get_or_init(Instant::now)
            .elapsed()
            .as_millis();
        eprintln!("startup {stage}: {elapsed} ms");
    }
}

fn launched_at_login() -> bool {
    std::env::args_os()
        .any(|argument| argument == std::ffi::OsStr::new(autostart::AUTOSTART_ARGUMENT))
}

fn spawn_startup_recovery(app_handle: tauri::AppHandle) {
    if !app_handle.state::<RuntimeState>().begin_startup_recovery() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let pool = app_handle.state::<sqlx::SqlitePool>().inner().clone();
        let mut recovery_failed = timeline::reconcile_capture_index(&app_handle, &pool)
            .await
            .is_err();
        if commands::refresh_local_inventory(&app_handle, true)
            .await
            .is_err()
        {
            recovery_failed = true;
        }
        if recovery_failed {
            app_handle
                .state::<RuntimeState>()
                .set_startup_recovery_error();
        }
        trace_startup("background recovery finished");
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    STARTUP_STARTED.get_or_init(Instant::now);
    trace_startup("process entered");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(RuntimeState::default())
        .manage(upload::UploadDiagnosticsRegistry::default())
        .setup(|app| {
            trace_startup("setup entered");
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                window.set_decorations(false)?;
            }
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let pool = tauri::async_runtime::block_on(database::connect(
                &data_dir.join("electronic-journey.sqlite3"),
            ))?;
            let stored_capture_settings =
                tauri::async_runtime::block_on(database::capture_settings(&pool));
            app.manage(pool);
            let runtime = app.state::<RuntimeState>();
            match stored_capture_settings {
                Ok(Some(settings)) => {
                    if runtime
                        .restore_settings(commands::CaptureSettings {
                            interval_minutes: settings.interval_minutes,
                            idle_pause_minutes: settings.idle_pause_minutes,
                            capture_mode: settings.capture_mode,
                        })
                        .is_err()
                    {
                        runtime.set_settings_recovery_error();
                    }
                }
                Ok(None) => {}
                Err(_) => runtime.set_settings_recovery_error(),
            }
            tray::install(app.handle())?;
            if !launched_at_login() {
                if let Some(window) = app.get_webview_window("main") {
                    window.show()?;
                }
            }
            system_monitor::start(app.handle());
            auto_sync::spawn_scheduler(app.handle().clone());
            trace_startup("database ready");
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                trace_startup("page loaded");
                spawn_startup_recovery(webview.app_handle().clone());
            }
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::set_launch_at_login,
            commands::get_active_upload_batch,
            commands::get_remote_profile,
            commands::get_upload_batch_status,
            commands::delete_timeline_capture,
            commands::list_timeline_day_selection,
            commands::list_timeline_captures,
            commands::pick_private_key_file,
            commands::read_timeline_capture,
            commands::read_timeline_thumbnail,
            commands::probe_remote_host_key,
            commands::refresh_screen_capture_permission,
            commands::request_screen_capture_permission,
            commands::set_recording_state,
            commands::save_remote_profile,
            commands::sync_today_now,
            commands::test_remote_profile,
            commands::update_capture_settings,
            commands::upload_selected_captures,
            commands::retry_failed_upload_items,
            commands::cancel_upload_batch,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Electronic Journey");
}
