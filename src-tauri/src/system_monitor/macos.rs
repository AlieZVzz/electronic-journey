use std::{ffi::c_void, ptr, time::Duration};

use block2::RcBlock;
use core_foundation::{base::TCFType, string::CFString};
use core_foundation_sys::notification_center::{
    CFNotificationCenterAddObserver, CFNotificationCenterGetDistributedCenter,
    CFNotificationCenterRef, CFNotificationName,
    CFNotificationSuspensionBehaviorDeliverImmediately,
};
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceSessionDidBecomeActiveNotification,
    NSWorkspaceSessionDidResignActiveNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use tauri::AppHandle;
use tokio::sync::mpsc;

use super::SystemEvent;

const COMBINED_SESSION_STATE: i32 = 0;
const ANY_INPUT_EVENT: u32 = u32::MAX;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
}

struct DistributedContext {
    sender: mpsc::UnboundedSender<SystemEvent>,
    event: SystemEvent,
}

extern "C" fn distributed_notification(
    _center: CFNotificationCenterRef,
    observer: *mut c_void,
    _name: CFNotificationName,
    _object: *const c_void,
    _user_info: core_foundation_sys::dictionary::CFDictionaryRef,
) {
    if observer.is_null() {
        return;
    }
    // SAFETY: install_distributed_observer allocates this context for the
    // process lifetime before registering its pointer with Core Foundation.
    let context = unsafe { &*observer.cast::<DistributedContext>() };
    let _ = context.sender.send(context.event);
}

pub fn install_native_listeners(
    _app: &AppHandle,
    sender: mpsc::UnboundedSender<SystemEvent>,
) -> Result<(), ()> {
    install_distributed_observer(
        "com.apple.screenIsLocked",
        SystemEvent::ScreenLocked,
        sender.clone(),
    )?;
    install_distributed_observer(
        "com.apple.screenIsUnlocked",
        SystemEvent::ScreenUnlocked,
        sender.clone(),
    )?;

    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    observe(
        &center,
        // SAFETY: AppKit exports this immutable notification-name constant.
        unsafe { NSWorkspaceWillSleepNotification },
        SystemEvent::Sleep,
        sender.clone(),
    );
    observe(
        &center,
        // SAFETY: AppKit exports this immutable notification-name constant.
        unsafe { NSWorkspaceDidWakeNotification },
        SystemEvent::Wake,
        sender.clone(),
    );
    // Session notifications cover fast-user switching and provide a public
    // fallback for lock/unlock transitions.
    observe(
        &center,
        // SAFETY: AppKit exports this immutable notification-name constant.
        unsafe { NSWorkspaceSessionDidResignActiveNotification },
        SystemEvent::ScreenLocked,
        sender.clone(),
    );
    observe(
        &center,
        // SAFETY: AppKit exports this immutable notification-name constant.
        unsafe { NSWorkspaceSessionDidBecomeActiveNotification },
        SystemEvent::ScreenUnlocked,
        sender,
    );
    Ok(())
}

fn observe(
    center: &NSNotificationCenter,
    name: &objc2_foundation::NSNotificationName,
    event: SystemEvent,
    sender: mpsc::UnboundedSender<SystemEvent>,
) {
    let block = RcBlock::new(move |_notification: std::ptr::NonNull<NSNotification>| {
        let _ = sender.send(event);
    });
    // SAFETY: the notification name is provided by AppKit, no object or
    // operation queue is supplied, and the copied block is Send.
    let _observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
    };
}

fn install_distributed_observer(
    name: &str,
    event: SystemEvent,
    sender: mpsc::UnboundedSender<SystemEvent>,
) -> Result<(), ()> {
    // SAFETY: this returns the process-global distributed center.
    let center = unsafe { CFNotificationCenterGetDistributedCenter() };
    if center.is_null() {
        return Err(());
    }
    let name = CFString::new(name);
    let context = Box::into_raw(Box::new(DistributedContext { sender, event }));
    // SAFETY: context remains allocated for the process lifetime, the callback
    // matches Core Foundation's ABI, and the notification name is valid.
    unsafe {
        CFNotificationCenterAddObserver(
            center,
            context.cast(),
            distributed_notification,
            name.as_concrete_TypeRef(),
            ptr::null(),
            CFNotificationSuspensionBehaviorDeliverImmediately,
        );
    }
    Ok(())
}

pub fn idle_duration() -> Result<Duration, ()> {
    // SAFETY: both constants are documented CoreGraphics enum values.
    let seconds =
        unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(());
    }
    Ok(Duration::from_secs_f64(seconds))
}
