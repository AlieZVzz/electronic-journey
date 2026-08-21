#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureDecision {
    Allow,
    Blocked(PrivacyReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyReason {
    RecordingDisabled,
    ScreenLocked,
    SystemSleeping,
    UserIdle,
    ExcludedApplication,
}

#[derive(Debug, Default)]
pub struct PrivacyContext {
    pub recording_enabled: bool,
    pub screen_locked: bool,
    pub system_sleeping: bool,
    pub user_idle: bool,
    pub excluded_application_active: bool,
}

pub fn evaluate(context: &PrivacyContext) -> CaptureDecision {
    let reason = if !context.recording_enabled {
        Some(PrivacyReason::RecordingDisabled)
    } else if context.screen_locked {
        Some(PrivacyReason::ScreenLocked)
    } else if context.system_sleeping {
        Some(PrivacyReason::SystemSleeping)
    } else if context.user_idle {
        Some(PrivacyReason::UserIdle)
    } else if context.excluded_application_active {
        Some(PrivacyReason::ExcludedApplication)
    } else {
        None
    };

    reason.map_or(CaptureDecision::Allow, CaptureDecision::Blocked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_screen_always_blocks_capture() {
        let context = PrivacyContext {
            recording_enabled: true,
            screen_locked: true,
            ..PrivacyContext::default()
        };
        assert_eq!(
            evaluate(&context),
            CaptureDecision::Blocked(PrivacyReason::ScreenLocked)
        );
    }

    #[test]
    fn blocker_priority_is_lock_then_sleep_then_idle() {
        let mut context = PrivacyContext {
            recording_enabled: true,
            screen_locked: true,
            system_sleeping: true,
            user_idle: true,
            excluded_application_active: false,
        };
        assert_eq!(
            evaluate(&context),
            CaptureDecision::Blocked(PrivacyReason::ScreenLocked)
        );
        context.screen_locked = false;
        assert_eq!(
            evaluate(&context),
            CaptureDecision::Blocked(PrivacyReason::SystemSleeping)
        );
        context.system_sleeping = false;
        assert_eq!(
            evaluate(&context),
            CaptureDecision::Blocked(PrivacyReason::UserIdle)
        );
    }
}
use serde::Serialize;
use std::path::Path;
use thiserror::Error;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationIdentity {
    pub platform: String,
    pub identifier: String,
    pub display_name: String,
}

#[derive(Debug, Error)]
pub enum ApplicationIdentityError {
    #[error("frontmost application is unavailable")]
    Unavailable,
    #[error("frontmost application identity is invalid")]
    Invalid,
    #[error("selected application is not supported")]
    UnsupportedSelection,
}

pub fn current_platform() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("macos")
    } else if cfg!(target_os = "windows") {
        Some("windows")
    } else {
        None
    }
}

pub fn frontmost_application() -> Result<ApplicationIdentity, ApplicationIdentityError> {
    #[cfg(target_os = "macos")]
    return macos::frontmost_application();
    #[cfg(target_os = "windows")]
    return windows::frontmost_application();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return unsupported::frontmost_application();
}

pub fn application_identity_from_path(
    path: &Path,
) -> Result<ApplicationIdentity, ApplicationIdentityError> {
    #[cfg(target_os = "macos")]
    return macos::application_identity_from_path(path);
    #[cfg(target_os = "windows")]
    return windows::application_identity_from_path(path);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return unsupported::application_identity_from_path(path);
}

pub fn application_is_allowed(
    excluded_identifiers: &std::collections::HashSet<String>,
    application: &ApplicationIdentity,
) -> bool {
    current_platform().is_some_and(|platform| {
        application.platform == platform && !excluded_identifiers.contains(&application.identifier)
    })
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod application_tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn excluded_application_identity_is_blocked_without_using_display_name() {
        let platform = current_platform().expect("desktop tests run on a supported platform");
        let application = ApplicationIdentity {
            platform: platform.to_string(),
            identifier: "stable.application.id".to_string(),
            display_name: "Sensitive Window Title Must Not Matter".to_string(),
        };
        let excluded = HashSet::from([application.identifier.clone()]);

        assert!(!application_is_allowed(&excluded, &application));
        assert!(application_is_allowed(&HashSet::new(), &application));
    }
}
