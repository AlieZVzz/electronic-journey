use std::path::Path;

use sha2::{Digest, Sha256};
use windows::{
    core::PWSTR,
    Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
};

use super::{ApplicationIdentity, ApplicationIdentityError};

pub fn frontmost_application() -> Result<ApplicationIdentity, ApplicationIdentityError> {
    let window = unsafe { GetForegroundWindow() };
    if window.0.is_null() {
        return Err(ApplicationIdentityError::Unavailable);
    }
    let mut process_id = 0u32;
    if unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) } == 0 || process_id == 0 {
        return Err(ApplicationIdentityError::Unavailable);
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|_| ApplicationIdentityError::Unavailable)?;
    let mut buffer = vec![0u16; 32_768];
    let mut length = u32::try_from(buffer.len()).map_err(|_| ApplicationIdentityError::Invalid)?;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.map_err(|_| ApplicationIdentityError::Unavailable)?;
    let path = String::from_utf16(&buffer[..length as usize])
        .map_err(|_| ApplicationIdentityError::Invalid)?;
    identity_from_executable_path(Path::new(&path))
}

pub fn application_identity_from_path(
    path: &Path,
) -> Result<ApplicationIdentity, ApplicationIdentityError> {
    let is_executable = path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("exe"));
    if !is_executable || !path.is_file() {
        return Err(ApplicationIdentityError::UnsupportedSelection);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| ApplicationIdentityError::UnsupportedSelection)?;
    identity_from_executable_path(&canonical)
}

fn identity_from_executable_path(
    path: &Path,
) -> Result<ApplicationIdentity, ApplicationIdentityError> {
    let display_name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or(ApplicationIdentityError::Invalid)?
        .to_string();
    let normalized = normalize_executable_path(path);
    let identifier = format!("win32:{:x}", Sha256::digest(normalized.as_bytes()));
    Ok(ApplicationIdentity {
        platform: "windows".to_string(),
        identifier,
        display_name,
    })
}

fn normalize_executable_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
    if let Some(rest) = normalized.strip_prefix(r"\\?\unc\") {
        format!(r"\\{rest}")
    } else {
        normalized
            .strip_prefix(r"\\?\")
            .unwrap_or(&normalized)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_windows_paths_match_regular_process_paths() {
        assert_eq!(
            normalize_executable_path(Path::new(r"\\?\C:\Program Files\App\APP.exe")),
            normalize_executable_path(Path::new(r"C:\Program Files\App\APP.exe")),
        );
        assert_eq!(
            normalize_executable_path(Path::new(r"\\?\UNC\server\share\app.exe")),
            normalize_executable_path(Path::new(r"\\server\share\app.exe")),
        );
    }
}
