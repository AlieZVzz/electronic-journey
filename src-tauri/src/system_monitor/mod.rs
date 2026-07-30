use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SystemEvent {
    ScreenLocked,
    ScreenUnlocked,
    Sleep,
    Wake,
    UserIdle(bool),
}

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use unsupported as platform;
#[cfg(target_os = "windows")]
use windows as platform;

pub fn start(app: &AppHandle) {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    if platform::install_native_listeners(app, sender).is_err() {
        app.state::<crate::commands::RuntimeState>()
            .system_monitor_failed();
        crate::tray::refresh(app);
        return;
    }
    app.state::<crate::commands::RuntimeState>()
        .system_monitor_ready();

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut idle_ticker = tokio::time::interval(Duration::from_secs(1));
        idle_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_idle = false;
        loop {
            tokio::select! {
                event = receiver.recv() => {
                    let Some(event) = event else {
                        app.state::<crate::commands::RuntimeState>()
                            .system_monitor_failed();
                        crate::tray::refresh(&app);
                        return;
                    };
                    crate::commands::handle_system_event(&app, event);
                }
                _ = idle_ticker.tick() => {
                    let threshold = app
                        .state::<crate::commands::RuntimeState>()
                        .idle_pause_threshold();
                    let idle = match threshold {
                        None => false,
                        Some(threshold) => match platform::idle_duration() {
                            Ok(duration) => duration >= threshold,
                            Err(()) => {
                                app.state::<crate::commands::RuntimeState>()
                                    .system_monitor_failed();
                                crate::tray::refresh(&app);
                                return;
                            }
                        },
                    };
                    if idle != last_idle {
                        last_idle = idle;
                        crate::commands::handle_system_event(
                            &app,
                            SystemEvent::UserIdle(idle),
                        );
                    }
                }
            }
        }
    });
}
