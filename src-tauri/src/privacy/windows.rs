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
    let display_name = Path::new(&path)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("未命名应用")
        .to_string();
    let identifier = format!("win32:{:x}", Sha256::digest(path.to_lowercase().as_bytes()));
    Ok(ApplicationIdentity {
        platform: "windows".to_string(),
        identifier,
        display_name,
    })
}
