use std::path::Path;

use super::{ApplicationIdentity, ApplicationIdentityError};

pub fn frontmost_application() -> Result<ApplicationIdentity, ApplicationIdentityError> {
    Err(ApplicationIdentityError::Unavailable)
}

pub fn application_identity_from_path(
    _path: &Path,
) -> Result<ApplicationIdentity, ApplicationIdentityError> {
    Err(ApplicationIdentityError::Unavailable)
}
