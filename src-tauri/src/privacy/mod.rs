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
}
