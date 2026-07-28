use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("application state lock was poisoned")]
    StatePoisoned,
    #[error("the requested state transition is not available to the frontend")]
    InvalidStateTransition,
    #[error("capture settings are invalid: {0}")]
    InvalidSettings(String),
    #[error("screen recording permission is required before recording can start")]
    CapturePermissionRequired,
}
