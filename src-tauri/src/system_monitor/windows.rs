use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{
        RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION},
        SystemInformation::GetTickCount64,
    },
    UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMECRITICAL, PBT_APMRESUMESTANDBY,
            PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
            WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
        },
    },
};

use super::SystemEvent;

const SUBCLASS_ID: usize = 0x454A_5359;

struct NativeContext {
    sender: mpsc::UnboundedSender<SystemEvent>,
}

pub fn install_native_listeners(
    app: &AppHandle,
    sender: mpsc::UnboundedSender<SystemEvent>,
) -> Result<(), ()> {
    let window = app.get_webview_window("main").ok_or(())?;
    let hwnd = window.hwnd().map_err(|_| ())?;
    let context = Box::into_raw(Box::new(NativeContext { sender }));

    // SAFETY: hwnd is the live Tauri window, the callback has the required
    // ABI, and context remains allocated until process exit.
    if !unsafe { SetWindowSubclass(hwnd, Some(window_subclass), SUBCLASS_ID, context as usize) }
        .as_bool()
    {
        // SAFETY: the pointer has not been registered and is still uniquely
        // owned by this function.
        drop(unsafe { Box::from_raw(context) });
        return Err(());
    }

    // SAFETY: hwnd is a valid top-level window owned by this process.
    if unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }.is_err() {
        // SAFETY: removes the callback registered above before reclaiming its
        // context pointer.
        unsafe {
            RemoveWindowSubclass(hwnd, Some(window_subclass), SUBCLASS_ID);
            drop(Box::from_raw(context));
        }
        return Err(());
    }
    Ok(())
}

unsafe extern "system" fn window_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    context: usize,
) -> LRESULT {
    let event = match message {
        WM_POWERBROADCAST if wparam.0 == PBT_APMSUSPEND as usize => Some(SystemEvent::Sleep),
        WM_POWERBROADCAST
            if [
                PBT_APMRESUMEAUTOMATIC,
                PBT_APMRESUMECRITICAL,
                PBT_APMRESUMESTANDBY,
                PBT_APMRESUMESUSPEND,
            ]
            .contains(&(wparam.0 as u32)) =>
        {
            Some(SystemEvent::Wake)
        }
        WM_WTSSESSION_CHANGE if wparam.0 == WTS_SESSION_LOCK as usize => {
            Some(SystemEvent::ScreenLocked)
        }
        WM_WTSSESSION_CHANGE if wparam.0 == WTS_SESSION_UNLOCK as usize => {
            Some(SystemEvent::ScreenUnlocked)
        }
        _ => None,
    };
    if let Some(event) = event {
        let context = context as *const NativeContext;
        if !context.is_null() {
            // SAFETY: install_native_listeners keeps the context allocated for
            // at least as long as the installed subclass callback.
            let _ = unsafe { &*context }.sender.send(event);
        }
    }
    // SAFETY: forwarding unhandled messages is required by the subclass API.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn idle_duration() -> Result<Duration, ()> {
    let mut last_input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: last_input points to a correctly sized writable structure.
    if !unsafe { GetLastInputInfo(&mut last_input) }.as_bool() {
        return Err(());
    }
    // GetLastInputInfo returns a 32-bit tick value. Wrapping subtraction
    // correctly handles the approximately 49.7-day rollover.
    let current = unsafe { GetTickCount64() } as u32;
    let idle_millis = current.wrapping_sub(last_input.dwTime);
    Ok(Duration::from_millis(u64::from(idle_millis)))
}
