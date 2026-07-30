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
