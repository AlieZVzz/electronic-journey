mod capture;
mod capture_pipeline;
mod commands;
mod database;
mod error;
mod privacy;
mod scheduler;
mod timeline;
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

fn spawn_startup_recovery(app_handle: tauri::AppHandle) {
    if !app_handle.state::<RuntimeState>().begin_startup_recovery() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        let pool = app_handle.state::<sqlx::SqlitePool>().inner().clone();
        let mut recovery_failed = timeline::reconcile_capture_index(&app_handle, &pool)
            .await
            .is_err();
        let inventory_app = app_handle.clone();
        match tauri::async_runtime::spawn_blocking(move || {
            capture_pipeline::capture_inventory(&inventory_app)
        })
        .await
        {
            Ok(Ok((count, bytes))) => app_handle
                .state::<RuntimeState>()
                .set_inventory(count, bytes),
            _ => recovery_failed = true,
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
        .manage(RuntimeState::default())
        .setup(|app| {
            trace_startup("setup entered");
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let pool = tauri::async_runtime::block_on(database::connect(
                &data_dir.join("electronic-journey.sqlite3"),
            ))?;
            app.manage(pool);
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
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::delete_timeline_capture,
            commands::list_timeline_captures,
            commands::read_timeline_capture,
            commands::read_timeline_thumbnail,
            commands::refresh_screen_capture_permission,
            commands::request_screen_capture_permission,
            commands::set_recording_state,
            commands::update_capture_settings,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Electronic Journey");
}
