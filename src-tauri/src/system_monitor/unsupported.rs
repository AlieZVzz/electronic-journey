use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::mpsc;

use super::SystemEvent;

pub fn install_native_listeners(
    _app: &AppHandle,
    _sender: mpsc::UnboundedSender<SystemEvent>,
) -> Result<(), ()> {
    Err(())
}

pub fn idle_duration() -> Result<Duration, ()> {
    Err(())
}
