mod capture;
mod capture_pipeline;
mod commands;
mod crypto;
mod database;
mod error;
mod image;
mod privacy;
mod scheduler;
mod timeline;
mod upload;
mod vault;

use capture::{PlatformCapture, ScreenCapture};
use commands::RuntimeState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Startup recovery checks existing permission without showing a system
    // prompt. Consent is requested only from an explicit frontend action.
    let permission_result = tauri::async_runtime::block_on(PlatformCapture.permission_state());

    tauri::Builder::default()
        .manage(RuntimeState::from_permission_result(permission_result))
        .setup(|app| {
            if let Ok((count, bytes)) = capture_pipeline::capture_inventory(app.handle()) {
                app.state::<RuntimeState>().set_inventory(count, bytes);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::list_timeline_captures,
            commands::read_timeline_capture,
            commands::request_screen_capture_permission,
            commands::set_recording_state,
            commands::update_capture_settings,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Electronic Journey");
}
