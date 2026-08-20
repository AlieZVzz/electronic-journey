use objc2_app_kit::NSWorkspace;

use super::{ApplicationIdentity, ApplicationIdentityError};

pub fn frontmost_application() -> Result<ApplicationIdentity, ApplicationIdentityError> {
    let workspace = NSWorkspace::sharedWorkspace();
    let application = workspace
        .frontmostApplication()
        .ok_or(ApplicationIdentityError::Unavailable)?;
    let identifier = application
        .bundleIdentifier()
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or(ApplicationIdentityError::Invalid)?;
    let display_name = application
        .localizedName()
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "未命名应用".to_string());
    Ok(ApplicationIdentity {
        platform: "macos".to_string(),
        identifier,
        display_name,
    })
}
