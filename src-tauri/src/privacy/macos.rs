use std::path::Path;

use objc2_app_kit::NSWorkspace;
use plist::Value;

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

pub fn application_identity_from_path(
    path: &Path,
) -> Result<ApplicationIdentity, ApplicationIdentityError> {
    let is_app = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("app"));
    if !is_app || !path.is_dir() {
        return Err(ApplicationIdentityError::UnsupportedSelection);
    }

    let canonical = path
        .canonicalize()
        .map_err(|_| ApplicationIdentityError::UnsupportedSelection)?;
    let info = Value::from_file(canonical.join("Contents/Info.plist"))
        .map_err(|_| ApplicationIdentityError::UnsupportedSelection)?;
    let dictionary = info
        .as_dictionary()
        .ok_or(ApplicationIdentityError::Invalid)?;
    let identifier = dictionary
        .get("CFBundleIdentifier")
        .and_then(Value::as_string)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ApplicationIdentityError::Invalid)?
        .to_string();
    let display_name = ["CFBundleDisplayName", "CFBundleName"]
        .into_iter()
        .find_map(|key| dictionary.get(key).and_then(Value::as_string))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            canonical
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .ok_or(ApplicationIdentityError::Invalid)?;

    Ok(ApplicationIdentity {
        platform: "macos".to_string(),
        identifier,
        display_name,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn selected_app_uses_bundle_metadata_without_exposing_the_path() {
        let root = std::env::temp_dir().join(format!("ej-app-selection-{}", uuid::Uuid::new_v4()));
        let app = root.join("Sensitive Location.app");
        fs::create_dir_all(app.join("Contents")).expect("create app fixture");
        let dictionary = plist::Dictionary::from_iter([
            (
                "CFBundleIdentifier".to_string(),
                Value::String("com.example.safe".to_string()),
            ),
            (
                "CFBundleDisplayName".to_string(),
                Value::String("Safe App".to_string()),
            ),
        ]);
        plist::to_file_xml(
            app.join("Contents/Info.plist"),
            &Value::Dictionary(dictionary),
        )
        .expect("write plist");

        let identity = application_identity_from_path(&app).expect("read application identity");

        assert_eq!(identity.identifier, "com.example.safe");
        assert_eq!(identity.display_name, "Safe App");
        assert!(!identity
            .identifier
            .contains(root.to_string_lossy().as_ref()));
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn selected_non_app_directory_is_rejected() {
        let root = std::env::temp_dir().join(format!("ej-app-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create fixture");
        assert!(matches!(
            application_identity_from_path(&root),
            Err(ApplicationIdentityError::UnsupportedSelection)
        ));
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
